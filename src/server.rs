use std::{
    collections::HashMap,
    fmt::{Display, Formatter},
};

use common_structs::{
    leaf::{LeafCommand, LeafEvent},
    message::Message,
    types::{FragmentIdx, Routing, Session},
};
use crossbeam_channel::{select_biased, Receiver, SendError, Sender};
use either::Either::{self, Left, Right};
use log::{info, warn};
use wg_2024::{
    network::NodeId,
    packet::{
        Ack, FloodRequest, FloodResponse, Fragment, Nack, NackType, NodeType, Packet, PacketType,
    },
};

/// NodeId present in request is not known
pub struct UnknownNodeIdError {
    pub node_id: NodeId,
}

impl Display for UnknownNodeIdError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Unknown Node Id {}", self.node_id)
    }
}

/// Required information to fulfill request is not known for the requested node
pub struct UnknownNodeInfoError {
    pub node_id: NodeId,
}

impl Display for UnknownNodeInfoError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Unknown Node Info for {}", self.node_id)
    }
}

/// All errors that can occur when fetching the required information to send packets to a node.
pub type PrepareNodeSendError = Either<UnknownNodeIdError, UnknownNodeInfoError>;

/// Information required to send a packet.
pub struct PreparedNodeSend<'a> {
    routing: &'a Routing,
    session: Session,
    neighbor: &'a Sender<Packet>,
    controller: &'a Sender<LeafEvent>,
    history: &'a mut PacketHistory,
}

/// Per node, the sender to send packets to this node
pub type PacketSendLookup = HashMap<NodeId, Sender<Packet>>;
/// Per node, the routing to use to send packets to it
pub type NodePathLookup = HashMap<NodeId, Routing>;
/// Per session id + fragment index, the packet that was sent
pub type PacketHistory = HashMap<(Session, FragmentIdx), Packet>;

/// Struct to store the information required to send packets
pub struct ServerSenders {
    /// Send information to the Simulation Controller
    controller_send: Sender<LeafEvent>,
    /// Send information to connected nodes (neighbors)
    packet_send: PacketSendLookup,

    /// Incremental session id
    session_id: Session,
    /// The path to use to reach a certain node
    node_path: NodePathLookup,
    /// History of packets we sent
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

    /// Constructor for unit testing
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

/// Struct to store the information required to receive packets
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

/// Protocol to use to process messages
pub trait ServerProtocol {
    fn on_message(
        &mut self,
        server: NodeId,
        senders: &mut ServerSenders,
        from: NodeId,
        message: Message,
        session_id: Session,
    );
}

/// Per session id + node, the fragments received under this session id (so far)
pub type PendingFragmentsLookup = HashMap<(Session, NodeId), Vec<Fragment>>;

/// Struct to store the information required to run a server
pub struct Server<T: ServerProtocol> {
    running: bool,
    id: NodeId,
    senders: ServerSenders,
    receivers: ServerReceivers,
    protocol: T,
    pending_fragments: PendingFragmentsLookup,
}

impl<T: ServerProtocol> Server<T> {
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
            protocol: implementation,
            pending_fragments: HashMap::new(),
        }
    }

    /// Process one packet
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

                            // Add known route to this node (direct)
                            let route = Routing::with_first_hop(vec![node_id]);
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
                            self.on_fragment(packet.routing_header, packet.session_id, fragment);
                        },
                        PacketType::FloodRequest(req) => {
                            self.on_flood_request(req);
                        }
                        PacketType::Nack(nack) => {
                            self.on_nack(packet.session_id, nack);
                        }
                        PacketType::Ack(_) => {} // We could mark the packet as Acked in the history (e.g. in case of a resend when no response after x seconds)
                        pack_type => {warn!("Received packet of type {}, which this server does not handle.", pack_type);}
                    }
                }
            },
        }
    }

    /// Process fragment received
    fn on_fragment(&mut self, routing: Routing, session_id: Session, fragment: Fragment) {
        info!("Received fragment: {:?}", fragment);

        let src_id = routing.source();
        match src_id {
            Some(node_id) => {
                let des_id = routing.destination();
                if des_id.is_none() || des_id.is_some_and(|id| id != self.id) {
                    // Packet is not meant for us
                    if let Err(e) = Self::send_packet(
                        &mut self.senders,
                        node_id,
                        PacketType::Nack(Nack {
                            fragment_index: fragment.fragment_index,
                            nack_type: NackType::UnexpectedRecipient(self.id),
                        }),
                        Some(session_id),
                    ) {
                        warn!("WARNING: Could not send nack. {}", e);
                    }
                    return;
                }

                // Update route to latest
                let mut node_path = routing.get_reversed();
                node_path.increase_hop_index();
                self.senders.node_path.insert(node_id, node_path);

                if let Err(e) = Self::send_packet(
                    &mut self.senders,
                    node_id,
                    PacketType::Ack(Ack {
                        fragment_index: fragment.fragment_index,
                    }),
                    Some(session_id),
                ) {
                    warn!("WARNING: Could not send ack. {}", e);
                }

                // Collect fragment parts until the full message is received
                let expected_fragment_count = fragment
                    .total_n_fragments
                    .try_into()
                    .expect("Total number of fragments count exceeds usize");
                let key = (session_id, node_id);
                let fragments = self
                    .pending_fragments
                    .entry(key)
                    .or_insert(Vec::with_capacity(expected_fragment_count));
                fragments.push(fragment);
                if fragments.len() == expected_fragment_count {
                    match self.pending_fragments.remove(&key) {
                        Some(fragments) => {
                            match Message::from_fragments(fragments) {
                                Ok(message) => {
                                    info!("Fragments parsed to message: {:?}", message);
                                    self.protocol.on_message(
                                        self.id,
                                        &mut self.senders,
                                        node_id,
                                        message,
                                        session_id,
                                    );
                                }
                                Err(e) => {
                                    warn!(
                                        "WARNING: Fragments could not be parsed to message. {}",
                                        e
                                    );
                                }
                            };
                        }
                        None => {
                            warn!("WARNING: Fragments are ready to be processed but could not be removed from HashMap.");
                        }
                    }
                }
            }
            None => {
                warn!(
                    "WARNING: Received packet with invalid routing_header. {}",
                    routing
                );
            }
        }
    }

    /// Process flood request received
    fn on_flood_request(&mut self, mut req: FloodRequest) {
        info!("Received flood request: {:?}", req);

        // Add self to path
        req.increment(self.id, NodeType::Server);

        // Path to initiator is reverse of path trace
        let mut path = Routing::with_first_hop(
            req.path_trace
                .iter()
                .cloned()
                .map(|(id, _)| id)
                .rev()
                .collect(),
        );

        // Handle flood request where the initiator did not add themselves to the path trace
        if let Some(destination) = path.destination() {
            if destination != req.initiator_id {
                path.append_hop(req.initiator_id);
            }
        }

        // Set path as most recent path to the initiator
        self.senders.node_path.insert(req.initiator_id, path);

        // Send flood response back
        if let Err(e) = Self::send_packet(
            &mut self.senders,
            req.initiator_id,
            PacketType::FloodResponse(FloodResponse {
                flood_id: req.flood_id,
                path_trace: req.path_trace,
            }),
            None,
        ) {
            warn!("WARNING: Could not send flood response. {}", e);
        }
    }

    /// Process nack received
    fn on_nack(&mut self, session_id: Session, nack: Nack) {
        match nack.nack_type {
            NackType::Dropped => {
                // Try resend the packet that was dropped
                let resend_packet = self.senders.history.get(&(session_id, nack.fragment_index));
                if let Some(resend_packet) = resend_packet {
                    if let Some(neighbor_id) = resend_packet.routing_header.current_hop() {
                        match self.senders.packet_send.get(&neighbor_id) {
                            Some(channel) => {
                                let resend_packet = resend_packet.clone();
                                Self::send_packet_raw(
                                    channel,
                                    &self.senders.controller_send,
                                    &mut self.senders.history,
                                    resend_packet,
                                );
                            }
                            None => {
                                warn!(
                                    "WARNING: Invalid routing neighbor in resend. {}",
                                    neighbor_id
                                );
                            }
                        }
                    } else {
                        warn!("WARNING: Invalid node_path route for node_id in resend. Current hop is None.");
                    }
                } else {
                    warn!("WARNING: Nack received for packet {}:{}, but no such packet is recorded in our send history.", session_id, nack.fragment_index);
                }
            }
            nack_type => {
                warn!("WARNING: Received nack of type {:?}", nack_type);
            }
        }
    }

    pub fn run(&mut self) {
        while self.running {
            self.update();
        }
    }

    /// Gather information required to send a packet to a node
    fn prepare_node_send(
        senders: &mut ServerSenders,
        to: NodeId,
        increment_session: bool,
    ) -> Result<PreparedNodeSend, PrepareNodeSendError> {
        match senders.node_path.get_mut(&to) {
            Some(node_path) => {
                // All node paths are stored with hop index 1 (ready to be send)
                if let Some(neighbor_id) = node_path.current_hop() {
                    match senders.packet_send.get(&neighbor_id) {
                        Some(channel) => {
                            if increment_session {
                                senders.session_id += 1; // First session has id 1
                            }

                            Ok(PreparedNodeSend {
                                routing: node_path,
                                session: senders.session_id,
                                neighbor: channel,
                                controller: &senders.controller_send,
                                history: &mut senders.history,
                            })
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

    /// Send a (sugared) packet to a node
    fn send_packet(
        senders: &mut ServerSenders,
        to: NodeId,
        packet: PacketType,
        fixed_session: Option<u64>, // Session id to use (in case of a response to received packet)
    ) -> Result<Option<SendError<Packet>>, PrepareNodeSendError> {
        let prepared_node_send = Self::prepare_node_send(senders, to, fixed_session.is_none())?;
        Ok(Self::send_packet_raw(
            prepared_node_send.neighbor,
            prepared_node_send.controller,
            prepared_node_send.history,
            Packet {
                routing_header: prepared_node_send.routing.clone(),
                session_id: fixed_session.unwrap_or(prepared_node_send.session),
                pack_type: packet,
            },
        ))
    }

    /// Send a packet (including session and routing information) to a node
    fn send_packet_raw(
        to: &Sender<Packet>,
        controller: &Sender<LeafEvent>,
        history: &mut PacketHistory,
        packet: Packet,
    ) -> Option<SendError<Packet>> {
        // Record any packet that can be required to resend
        // Only MsgFragments can be dropped
        let record: bool = matches!(packet.pack_type, PacketType::MsgFragment(_));
        if record {
            history.insert(
                (packet.session_id, packet.get_fragment_index()),
                packet.clone(),
            );
        }

        // Inform the controller we are sending a packet
        if let Err(e) = controller.send(LeafEvent::PacketSend(packet.clone())) {
            warn!("WARNING: Could not inform controller of packet send: {}", e)
        }

        // Try sending the packet to the closest neighbor
        let send_error = to.send(packet.clone()).err();

        // Are we allowed to use the Simulation Controller in case we cannot send it through our neighbor
        let use_shortcut = matches!(
            packet.pack_type,
            PacketType::Ack(_) | PacketType::Nack(_) | PacketType::FloodResponse(_)
        );
        if use_shortcut && send_error.is_some() {
            let shortcut_error = controller.send(LeafEvent::ControllerShortcut(packet)).err();

            return match shortcut_error {
                Some(e) => {
                    warn!("WARNING: Send error using shortcut: {}", e);
                    send_error // Return original send error
                }
                None => None, // Hide original error, sending through Simulation Controller succeeded
            };
        }

        send_error
    }

    /// Send a message to a node and process all errors
    pub fn send_message(
        from: NodeId,
        senders: &mut ServerSenders,
        to: NodeId,
        message: Message,
        fixed_session: Option<u64>, // Session id to use (in case of a response to received packet)
    ) {
        let res = Self::send_message_raw(from, senders, to, message, fixed_session);

        match res {
            Ok(send_errors) => {
                for error in send_errors.into_iter().flatten() {
                    warn!("WARNING: Send message error: {}", error)
                }
            }
            Err(e) => warn!("WARNING: Send message error: {}", e),
        };
    }

    /// Send a message to a node and receive all errors
    /// The message will be split in multiple fragments
    /// More optimized than using send_packet for each fragment
    fn send_message_raw(
        from: NodeId,
        senders: &mut ServerSenders,
        to: NodeId,
        message: Message,
        fixed_session: Option<u64>, // Session id to use (in case of a response to received packet)
    ) -> Result<Vec<Option<SendError<Packet>>>, Either<UnknownNodeIdError, UnknownNodeInfoError>>
    {
        let prepared_node_send = Self::prepare_node_send(senders, to, fixed_session.is_none())?;
        let session = fixed_session.unwrap_or(prepared_node_send.session);

        // Inform controller we are sending a message
        if let Err(e) = prepared_node_send
            .controller
            .send(LeafEvent::MessageStartSend {
                start: from,
                session,
                dest: to,
                message: message.clone(),
            })
        {
            warn!("WARNING: Could not send message start to controller: {}", e);
        }

        // Send message split into fragment packets
        let result = Ok(message
            .into_fragments()
            .into_iter()
            .map(|fragment| {
                Self::send_packet_raw(
                    prepared_node_send.neighbor,
                    prepared_node_send.controller,
                    prepared_node_send.history,
                    Packet::new_fragment(prepared_node_send.routing.clone(), session, fragment),
                )
            })
            .collect());

        // Inform controller finished sending a message
        if let Err(e) = prepared_node_send
            .controller
            .send(LeafEvent::MessageFullySent(from, session))
        {
            warn!(
                "WARNING: Could not send message fully sent to controller: {}",
                e
            );
        }

        result
    }
}
