use std::collections::HashMap;

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
    packet::{Packet, PacketType::MsgFragment},
};

#[derive(Debug)]
pub struct UnknownNodeIdError {
    pub node_id: NodeId,
}

pub struct ServerSenders {
    controller_send: Sender<LeafEvent>,
    packet_send: HashMap<NodeId, Sender<Packet>>,
}

impl ServerSenders {
    pub fn new(
        controller_send: Sender<LeafEvent>,
        packet_send: HashMap<NodeId, Sender<Packet>>,
    ) -> Self {
        ServerSenders {
            controller_send,
            packet_send,
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
    fn on_message(&mut self, senders: &ServerSenders, from: &NodeId, message: Message) -> ();
}

pub struct Server<T: ServerLogic> {
    crashed: bool,
    id: NodeId,
    senders: ServerSenders,
    receivers: ServerReceivers,
    implementation: T,
    // fragments: HashMap<u64, Vec<Fragment>>,
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
                    if let MsgFragment(fragment) = packet.pack_type {
                        println!("Received fragments: {:?}", fragment);
                        match Message::from_fragments(vec![fragment]) {
                            Ok(message) => {
                                println!("Fragments parsed to message: {:?}", message);
                                // TODO: add to fragments hashmap and call on_message once all fragments are received
                                // TODO: send ACK back?
                                self.implementation.on_message(&self.senders, packet.routing_header.hops.get(0).unwrap(), message);
                            },
                            Err(e) => println!("WARNING: Fragments could not be parsed to message. {}", e),
                        }
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

    pub fn send_message(
        senders: &ServerSenders,
        to: &NodeId,
        message: Message,
    ) -> Either<UnknownNodeIdError, Option<Vec<SendError<Packet>>>> {
        match senders.packet_send.get(&to) {
            Some(channel) => Right(
                message
                    .into_fragments()
                    .iter()
                    .map(|fragment| {
                        channel
                            .send(Packet::new_fragment(
                                SourceRoutingHeader::empty_route(), // TODO
                                0,                                  // TODO
                                fragment.clone(),
                            ))
                            .err()
                    })
                    .collect(),
            ),
            None => Left(UnknownNodeIdError {
                node_id: to.clone(),
            }),
        }
    }
}
