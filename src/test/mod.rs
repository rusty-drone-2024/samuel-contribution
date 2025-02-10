#![cfg(test)]
// Testing helper functions

use std::{collections::HashMap, fmt::Display};

use common_structs::{leaf::LeafEvent, message::Message};
use crossbeam_channel::{unbounded, Receiver, Sender};
use wg_2024::{
    network::{NodeId, SourceRoutingHeader},
    packet::{Packet, PacketType},
};

use crate::server::{ServerProtocol, ServerSenders};

mod chat;
mod media;
mod server;
mod text;

pub fn setup_node0() -> (ServerSenders, Receiver<Packet>) {
    let (controller_send, _test_controller_recv) = unbounded::<LeafEvent>();
    let mut packet_send = HashMap::<NodeId, Sender<Packet>>::new();

    let (node0_send, node0_recv) = unbounded::<Packet>();
    packet_send.insert(0, node0_send);

    let mut node_path = HashMap::new();
    node_path.insert(0, SourceRoutingHeader::with_first_hop(vec![0, 0]));

    (
        ServerSenders::with_node_path(controller_send, packet_send, node_path),
        node0_recv,
    )
}

pub fn panic_to_message<T: Display>(packet: Result<Packet, T>) -> Message {
    match packet {
        Ok(packet) => match packet.pack_type {
            PacketType::MsgFragment(fragment) => match Message::from_fragments(vec![fragment]) {
                Ok(message) => message,
                Err(e) => panic!("Fragment is not a message: {}", e),
            },
            _ => panic!("Packet is not a fragment."),
        },
        Err(e) => panic!("No packet could be received: {}", e),
    }
}

pub fn panic_to_message_multi<T: Display>(packets: Vec<Result<Packet, T>>) -> Message {
    let mut fragments = Vec::with_capacity(packets.len());
    for packet in packets {
        match packet {
            Ok(packet) => match packet.pack_type {
                PacketType::MsgFragment(fragment) => {
                    fragments.push(fragment);
                }
                _ => panic!("Packet is not a fragment."),
            },
            Err(e) => panic!("No packet could be received: {}", e),
        }
    }

    match Message::from_fragments(fragments) {
        Ok(message) => message,
        Err(e) => panic!("Fragments are not a message: {}", e),
    }
}

pub fn assert_eq_message<T: Display>(packet: Result<Packet, T>, expected_message: Message) {
    assert_eq!(panic_to_message(packet), expected_message)
}

pub fn test_on_message<T: ServerProtocol>(server: &mut T, message: Message, response: Message) {
    let (mut senders, node0_recv) = setup_node0();

    server.on_message(0, &mut senders, 0, message, 0);

    assert_eq_message(node0_recv.recv(), response);
}

pub fn test_on_message_fn<T: ServerProtocol>(
    server: &mut T,
    message: Message,
    check: Box<dyn FnOnce(Message)>,
) {
    let (mut senders, node0_recv) = setup_node0();

    server.on_message(0, &mut senders, 0, message, 0);

    check(panic_to_message(node0_recv.recv()));
}
