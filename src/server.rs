use std::{
    collections::{HashMap, HashSet},
    fmt::{Display, Formatter},
};

use common_structs::{
    leaf::{LeafCommand, LeafEvent},
    message::Message,
};
use crossbeam_channel::{select_biased, Receiver, SendError, Sender};
use either::Either::{self, Left, Right};
use log::{info, warn};
use wg_2024::{
    network::{NodeId, SourceRoutingHeader},
    packet::{Ack, FloodResponse, Fragment, Nack, NackType, NodeType, Packet, PacketType},
};

pub struct UnknownNodeIdError {
    pub node_id: NodeId,
}

impl Display for UnknownNodeIdError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Unknown Node Id {}", self.node_id)
    }
}

pub struct UnknownNodeInfoError {
    pub node_id: NodeId,
}

impl Display for UnknownNodeInfoError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Unknown Node Info {}", self.node_id)
    }
}

pub type PacketSendLookup = HashMap<NodeId, Sender<Packet>>;
pub type NodePathLookup = HashMap<NodeId, SourceRoutingHeader>;
pub type PacketHistory = HashMap<(u64, u64), Packet>;
pub struct ServerSenders {
    controller_send: Sender<LeafEvent>,
    packet_send: PacketSendLookup,

    session_id: u64,
    node_path: NodePathLookup,
    history: PacketHistory,
}

impl ServerSenders {
    pub fn new(controller_send: Sender<LeafEvent>, packet_send: PacketSendLookup) -> Self {
        ServerSenders {
            controller_send,
            packet_send,

            session_id: 0,
            node_path: HashMap::new(),
            history: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    pub fn with_node_path(
        controller_send: Sender<LeafEvent>,
        packet_send: PacketSendLookup,
        node_path: NodePathLookup,
    ) -> Self {
        ServerSenders {
            controller_send,
            packet_send,
            node_path,

            session_id: 0,
            history: HashMap::new(),
        }
    }
}

pub struct ServerReceivers {
    controller_recv: Receiver<LeafCommand>,
    packet_recv: Receiver<Packet>,
}

impl ServerReceivers {
    pub fn new(controller_recv: Receiver<LeafCommand>, packet_recv: Receiver<Packet>) -> Self {
        ServerReceivers {
            controller_recv,
            packet_recv,
        }
    }
}

pub trait ServerLogic {
    fn on_message(
        &mut self,
        senders: &mut ServerSenders,
        from: NodeId,
        message: Message,
        session_id: u64,
    ) -> ();
}

pub type PendingFragmentsLookup = HashMap<(u64, NodeId), Vec<Fragment>>;
pub type SeenFloodRequests = HashSet<(u64, NodeId)>;
pub struct Server<T: ServerLogic> {
    running: bool,
    id: NodeId,
    senders: ServerSenders,
    receivers: ServerReceivers,
    implementation: T,
    pending_fragments: PendingFragmentsLookup,
    seen_flood_requests: SeenFloodRequests,
}

impl<T: ServerLogic> Server<T> {
    pub fn create(
        id: NodeId,
        controller_send: Sender<LeafEvent>,
        controller_recv: Receiver<LeafCommand>,
        packet_recv: Receiver<Packet>,
        packet_send: PacketSendLookup,
        implementation: T,
    ) -> Self {
        Server {
            running: true,
            id,
            senders: ServerSenders::new(controller_send, packet_send),
            receivers: ServerReceivers::new(controller_recv, packet_recv),
            implementation,
            pending_fragments: HashMap::new(),
            seen_flood_requests: HashSet::new(),
        }
    }

    pub fn update(&mut self) {
        select_biased! {
            recv(self.receivers.controller_recv) -> res => {
                if let Ok(packet) = res {
                    match packet {
                        LeafCommand::RemoveSender(node_id) => {
                            if self.senders.packet_send.remove(&node_id).is_some() {
                                self.senders.node_path.remove(&node_id);
                            }
                        },
                        LeafCommand::AddSender(node_id, sender) => {
                            self.senders.packet_send.insert(node_id, sender);
                            let route = SourceRoutingHeader::with_first_hop(vec![node_id]);
                            self.senders.node_path.insert(node_id, route);
                        },
                        LeafCommand::Kill => self.running = false,
                    };
                }
            },
            recv(self.receivers.packet_recv) -> res => {
                if let Ok(packet) = res {
                    match packet.pack_type {
                        PacketType::MsgFragment(fragment) => {
                            info!("Received fragment: {:?}", fragment);

                            let src_id = packet.routing_header.source();
                            match src_id {
                                Some(node_id) => {
                                    let des_id = packet.routing_header.destination();
                                    if des_id.is_none() || des_id.is_some_and(|id| id != self.id) {
                                        // Packet is not meant for us
                                        if let Err(e) = Self::send_packet(&mut self.senders, node_id, PacketType::Nack(Nack{ fragment_index: fragment.fragment_index, nack_type: NackType::UnexpectedRecipient(self.id)}), Some(packet.session_id)) {
                                            warn!("WARNING: Could not send nack. {}", e);
                                        }
                                        return;
                                    }

                                    // Update route to latest
                                    let mut node_path = packet.routing_header.get_reversed();
                                    node_path.increase_hop_index();
                                    self.senders.node_path.insert(node_id, node_path);

                                    if let Err(e) = Self::send_packet(&mut self.senders, node_id, PacketType::Ack(Ack{fragment_index: fragment.fragment_index}), Some(packet.session_id)) {
                                        warn!("WARNING: Could not send ack. {}", e);
                                    }

                                    // Collect fragment parts until the full message is received
                                    let expected_fragment_count = fragment.total_n_fragments.try_into().unwrap();
                                    let key = (packet.session_id, node_id);
                                    let fragments = self.pending_fragments.entry(key).or_insert(Vec::with_capacity(expected_fragment_count));
                                    fragments.push(fragment);
                                    if fragments.len() == expected_fragment_count {
                                        match self.pending_fragments.remove(&key) {
                                            Some(fragments) => {
                                                match Message::from_fragments(fragments) {
                                                    Ok(message) => {
                                                        info!("Fragments parsed to message: {:?}", message);
                                                        self.implementation.on_message(&mut self.senders, node_id, message, packet.session_id);
                                                    },
                                                    Err(e) => {warn!("WARNING: Fragments could not be parsed to message. {}", e);},
                                                };
                                            }
                                            None => {warn!("WARNING: Fragments are ready to be processed but could not be removed from HashMap.");}
                                        }
                                    }
                                }
                                None => {warn!("WARNING: Received packet with invalid routing_header. {}", packet.routing_header);}
                            }
                        },
                        PacketType::FloodRequest(mut req) => {
                            info!("Received flood request: {:?}", req);
                            let key = (req.flood_id, req.initiator_id);
                            let from = req.path_trace.last().cloned();

                            req.increment(self.id, NodeType::Server);
                            match from {
                                Some((from_id, _)) => {
                                    if !self.seen_flood_requests.contains(&key) && self.senders.packet_send.len() > 1 {
                                        for (node_id, _) in self.senders.packet_send.clone().into_iter() {
                                            if node_id == from_id {
                                                continue;
                                            }

                                            if let Err(e) = Self::send_packet(&mut self.senders, node_id, PacketType::FloodRequest(req.clone()), None) {
                                                warn!("WARNING: Could not send flood request. {}", e);
                                            }
                                        }
                                    } else {
                                        let mut path = SourceRoutingHeader::with_first_hop(
                                            req.path_trace
                                                .iter()
                                                .cloned()
                                                .map(|(id, _)| id)
                                                .rev()
                                                .collect(),
                                        );
                                        if let Some(destination) = path.destination() {
                                            if destination != req.initiator_id {
                                                path.append_hop(req.initiator_id);
                                            }
                                        }

                                        self.senders.node_path.insert(req.initiator_id, path);
                                        if let Err(e) = Self::send_packet(&mut self.senders, req.initiator_id, PacketType::FloodResponse(FloodResponse { flood_id: req.flood_id, path_trace: req.path_trace }), None) {
                                            warn!("WARNING: Could not send flood response. {}", e);
                                        }
                                    }
                                }
                                None => {warn!("WARNING: Received flood request with empty path trace.");}
                            }
                        }
                        PacketType::Nack(nack) => {
                            let resend_packet = self.senders.history.get(&(packet.session_id, nack.fragment_index));
                            if let Some(resend_packet) = resend_packet {
                                if let Some(neighbor_id) = resend_packet.routing_header.current_hop() {
                                    match self.senders.packet_send.get(&neighbor_id) {
                                        Some(channel) => {
                                            let resend_packet = resend_packet.clone();
                                            Self::send_packet_raw(channel, &self.senders.controller_send, &mut self.senders.history, resend_packet);
                                        }
                                        None => {
                                            warn!("WARNING: Invalid routing neighbor in resend. {}", neighbor_id);
                                        },
                                    }
                                } else {
                                    warn!("WARNING: Invalid node_path route for node_id in resend. Current hop is None.");
                                }
                            } else {
                                warn!("WARNING: Nack received for packet {}:{}, but no such packet is recorded in our send history.", packet.session_id, nack.fragment_index);
                            }
                        }
                        _ => {}
                    }
                }
            },
        }
    }

    pub fn run(&mut self) {
        while self.running {
            self.update();
        }
    }

    fn prepare_node_send(
        senders: &mut ServerSenders,
        to: NodeId,
        increment_session: bool,
    ) -> Result<
        (
            &SourceRoutingHeader,
            u64,
            &Sender<Packet>,
            &Sender<LeafEvent>,
            &mut PacketHistory,
        ),
        Either<UnknownNodeIdError, UnknownNodeInfoError>,
    > {
        match senders.node_path.get_mut(&to) {
            Some(node_path) => {
                if let Some(neighbor_id) = node_path.current_hop() {
                    match senders.packet_send.get(&neighbor_id) {
                        Some(channel) => {
                            if increment_session {
                                senders.session_id += 1; // First session has id 1
                            }

                            Ok((
                                node_path,
                                senders.session_id,
                                channel,
                                &senders.controller_send,
                                &mut senders.history,
                            ))
                        }
                        None => Err(Left(UnknownNodeIdError { node_id: to })),
                    }
                } else {
                    warn!("WARNING: Invalid node_path route for node_id. Current hop is None.");
                    Err(Right(UnknownNodeInfoError { node_id: to }))
                }
            }
            None => Err(Right(UnknownNodeInfoError { node_id: to })),
        }
    }

    fn send_packet(
        senders: &mut ServerSenders,
        to: NodeId,
        packet: PacketType,
        fixed_session: Option<u64>,
    ) -> Result<Option<SendError<Packet>>, Either<UnknownNodeIdError, UnknownNodeInfoError>> {
        let (node_path, session_id, channel, controller, history) =
            Self::prepare_node_send(senders, to, fixed_session.is_none())?;
        Ok(Self::send_packet_raw(
            channel,
            controller,
            history,
            Packet {
                routing_header: node_path.clone(),
                session_id: fixed_session.unwrap_or(session_id),
                pack_type: packet,
            },
        ))
    }

    fn send_packet_raw(
        to: &Sender<Packet>,
        controller: &Sender<LeafEvent>,
        history: &mut PacketHistory,
        packet: Packet,
    ) -> Option<SendError<Packet>> {
        let record: bool = match packet.pack_type {
            PacketType::MsgFragment(_) => true,
            _ => false,
        };
        if record {
            history.insert(
                (packet.session_id, packet.get_fragment_index()),
                packet.clone(),
            );
        }

        let send_error = to.send(packet.clone()).err();

        let use_shortcut = match packet.pack_type {
            PacketType::Ack(_) | PacketType::Nack(_) | PacketType::FloodResponse(_) => true,
            _ => false,
        };
        if use_shortcut && send_error.is_some() {
            let shortcut_error = controller.send(LeafEvent::ControllerShortcut(packet)).err();

            return match shortcut_error {
                Some(e) => {
                    warn!("WARNING: Send error using shortcut: {}", e);
                    send_error // Return original send error
                }
                None => None, // Hide original error
            };
        }

        send_error
    }

    pub fn send_message(
        senders: &mut ServerSenders,
        to: NodeId,
        message: Message,
        fixed_session: Option<u64>,
    ) {
        let res = Self::send_message_raw(senders, to, message, fixed_session);

        match res {
            Ok(send_errors) => {
                for error in send_errors {
                    if let Some(e) = error {
                        warn!("WARNING: Send message error: {}", e)
                    }
                }
            }
            Err(e) => warn!("WARNING: Send message error: {}", e),
        };
    }

    fn send_message_raw(
        senders: &mut ServerSenders,
        to: NodeId,
        message: Message,
        fixed_session: Option<u64>,
    ) -> Result<Vec<Option<SendError<Packet>>>, Either<UnknownNodeIdError, UnknownNodeInfoError>>
    {
        let (node_path, session_id, channel, controller, history) =
            Self::prepare_node_send(senders, to, fixed_session.is_none())?;
        Ok(message
            .into_fragments()
            .into_iter()
            .map(|fragment| {
                Self::send_packet_raw(
                    channel,
                    controller,
                    history,
                    Packet::new_fragment(
                        node_path.clone(),
                        fixed_session.unwrap_or(session_id),
                        fragment,
                    ),
                )
            })
            .collect())
    }
}
