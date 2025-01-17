use std::{collections::HashMap, fmt::Display};

use common_structs::{
    leaf::{
        LeafCommand::{self, AddSender, Crash, RemoveSender},
        LeafEvent,
    },
    message::Message,
};
use crossbeam_channel::{select_biased, Receiver, SendError, Sender};
use either::Either::{self, Left, Right};
use wg_2024::{
    network::{NodeId, SourceRoutingHeader},
    packet::{
        Ack, Fragment, Nack, NackType, Packet,
        PacketType::{self, MsgFragment},
    },
};

pub struct UnknownNodeIdError {
    pub node_id: NodeId,
}

impl Display for UnknownNodeIdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Unknown Node Id {}", self.node_id)
    }
}

pub struct UnknownNodeInfoError {
    pub node_id: NodeId,
}

impl Display for UnknownNodeInfoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Unknown Node Info {}", self.node_id)
    }
}

#[derive(Clone)]
pub struct NodeInfo {
    route: SourceRoutingHeader,
    session_id: u64,
}

impl NodeInfo {
    pub fn new(route: SourceRoutingHeader, session_id: u64) -> Self {
        Self { route, session_id }
    }
}

#[allow(unused)]
pub struct ServerSenders {
    controller_send: Sender<LeafEvent>,
    packet_send: HashMap<NodeId, Sender<Packet>>,

    node_info: HashMap<NodeId, NodeInfo>,
}

impl ServerSenders {
    pub fn new(
        controller_send: Sender<LeafEvent>,
        packet_send: HashMap<NodeId, Sender<Packet>>,
    ) -> Self {
        ServerSenders {
            controller_send,
            packet_send,

            node_info: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    pub fn with_node_info(
        controller_send: Sender<LeafEvent>,
        packet_send: HashMap<NodeId, Sender<Packet>>,
        node_info: HashMap<NodeId, NodeInfo>,
    ) -> Self {
        ServerSenders {
            controller_send,
            packet_send,
            node_info,
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
    fn on_message(&mut self, senders: &mut ServerSenders, from: NodeId, message: Message) -> ();
}

pub struct Server<T: ServerLogic> {
    crashed: bool,
    id: NodeId,
    senders: ServerSenders,
    receivers: ServerReceivers,
    implementation: T,
    pending_fragments: HashMap<(u64, u8), Vec<Fragment>>,
}

impl<T: ServerLogic> Server<T> {
    pub fn new(
        id: NodeId,
        controller_send: Sender<LeafEvent>,
        controller_recv: Receiver<LeafCommand>,
        packet_recv: Receiver<Packet>,
        packet_send: HashMap<NodeId, Sender<Packet>>,
        implementation: T,
    ) -> Self {
        Server {
            crashed: false,
            id,
            senders: ServerSenders::new(controller_send, packet_send),
            receivers: ServerReceivers::new(controller_recv, packet_recv),
            implementation,
            pending_fragments: HashMap::new(),
        }
    }

    pub fn update(&mut self) {
        select_biased! {
            recv(self.receivers.controller_recv) -> res => {
                if let Ok(packet) = res {
                    match packet {
                        RemoveSender(node_id) => {self.senders.packet_send.remove(&node_id);},
                        AddSender(node_id, sender) => {self.senders.packet_send.insert(node_id, sender);},
                        Crash => self.crashed = true,
                    };
                }
            },
            recv(self.receivers.packet_recv) -> res => {
                if let Ok(packet) = res {
                    let src_id = packet.routing_header.source();
                    match src_id {
                        Some(node_id) => {
                            let des_id = packet.routing_header.destination();
                            if des_id.is_none() || des_id.is_some_and(|id| id != self.id) {
                                if let Err(e) = Self::send_packet(&mut self.senders, node_id, PacketType::Nack(Nack{ fragment_index: packet.get_fragment_index(), nack_type: NackType::UnexpectedRecipient(self.id)})) {
                                    println!("WARNING: Could not send nack. {}", e);
                                }
                                return;
                            }

                            if let MsgFragment(fragment) = packet.pack_type {
                                println!("Received fragment: {:?}", fragment);
                                let node_info = self.senders.node_info.entry(node_id).or_insert(NodeInfo::new(SourceRoutingHeader::empty_route(), 0));
                                node_info.route = packet.routing_header.get_reversed();

                                if let Err(e) = Self::send_packet(&mut self.senders, node_id, PacketType::Ack(Ack{fragment_index: fragment.fragment_index})) {
                                    println!("WARNING: Could not send ack. {}", e);
                                }

                                let expected_fragment_count = fragment.total_n_fragments.try_into().unwrap();
                                let key = (packet.session_id, node_id);
                                let fragments = self.pending_fragments.entry(key).or_insert(Vec::with_capacity(expected_fragment_count));
                                fragments.push(fragment);
                                if fragments.len() == expected_fragment_count {
                                    match self.pending_fragments.remove(&key) {
                                        Some(fragments) => {
                                            match Message::from_fragments(fragments) {
                                                Ok(message) => {
                                                    println!("Fragments parsed to message: {:?}", message);
                                                    self.implementation.on_message(&mut self.senders, node_id, message);

                                                },
                                                Err(e) => println!("WARNING: Fragments could not be parsed to message. {}", e),
                                            };
                                        }
                                        None => {println!("WARNING: Fragments are ready to be processed but could not be removed from HashMap.")}
                                    }
                                }
                            }
                        }
                        None => {println!("WARNING: Received packet with invalid routing_header. {}", packet.routing_header);}
                    }
                }
            },
        }
    }

    pub fn run(&mut self) {
        while !self.crashed {
            self.update();
        }
    }

    fn prepare_node_send(
        senders: &mut ServerSenders,
        to: NodeId,
    ) -> Result<
        (&NodeInfo, &Sender<Packet>, &Sender<LeafEvent>),
        Either<UnknownNodeIdError, UnknownNodeInfoError>,
    > {
        match senders.packet_send.get(&to) {
            Some(channel) => match senders.node_info.get_mut(&to) {
                Some(node_info) => {
                    node_info.session_id += 1; // First sessions has id 1
                    Ok((node_info, channel, &senders.controller_send))
                }
                None => Err(Right(UnknownNodeInfoError { node_id: to })),
            },
            None => Err(Left(UnknownNodeIdError { node_id: to })),
        }
    }

    fn send_packet(
        senders: &mut ServerSenders,
        to: NodeId,
        packet: PacketType,
    ) -> Result<Option<SendError<Packet>>, Either<UnknownNodeIdError, UnknownNodeInfoError>> {
        let (node_info, channel, controller) = Self::prepare_node_send(senders, to)?;
        let send_error = channel
            .send(Packet {
                routing_header: node_info.route.clone(),
                session_id: node_info.session_id,
                pack_type: packet.clone(),
            })
            .err();
        let use_shortcut = match packet {
            PacketType::Ack(_) | PacketType::Nack(_) | PacketType::FloodResponse(_) => true,
            _ => false,
        };
        if use_shortcut && send_error.is_some() {
            let shortcut_error = controller
                .send(LeafEvent::ControllerShortcut(Packet {
                    routing_header: node_info.route.clone(),
                    session_id: node_info.session_id,
                    pack_type: packet,
                }))
                .err();

            return match shortcut_error {
                Some(e) => {
                    println!("WARNING: Send error to shortcut: {}", e);
                    Ok(send_error) // Return original send error
                }
                None => Ok(None), // Hide original error
            };
        }

        Ok(send_error)
    }

    pub fn send_message(senders: &mut ServerSenders, to: NodeId, message: Message) {
        let res = Self::send_message_raw(senders, to, message);

        match res {
            Ok(send_errors) => {
                for error in send_errors {
                    if let Some(e) = error {
                        println!("WARNING: Send message error: {}", e)
                    }
                }
            }
            Err(e) => println!("WARNING: Send message error: {}", e),
        };
    }

    fn send_message_raw(
        senders: &mut ServerSenders,
        to: NodeId,
        message: Message,
    ) -> Result<Vec<Option<SendError<Packet>>>, Either<UnknownNodeIdError, UnknownNodeInfoError>>
    {
        let (node_info, channel, _) = Self::prepare_node_send(senders, to)?;
        Ok(message
            .into_fragments()
            .into_iter()
            .map(|fragment| {
                channel
                    .send(Packet::new_fragment(
                        node_info.route.clone(),
                        node_info.session_id,
                        fragment,
                    ))
                    .err()
            })
            .collect())
    }
}
