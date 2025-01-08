#![cfg(test)]

use std::collections::HashMap;

use common_structs::{leaf::LeafEvent, message::Message};
use crossbeam_channel::{unbounded, Receiver, RecvError, Sender};
use wg_2024::{
    network::NodeId,
    packet::{Packet, PacketType},
};

use crate::server::{ServerLogic, ServerSenders};

mod media;
mod server;
mod text;

pub fn setup_node0() -> (ServerSenders, Receiver<Packet>) {
    let (controller_send, _test_controller_recv) = unbounded::<LeafEvent>();
    let mut packet_send = HashMap::<NodeId, Sender<Packet>>::new();

    let (node0_send, node0_recv) = unbounded::<Packet>();
    packet_send.insert(0, node0_send);

    (ServerSenders::new(controller_send, packet_send), node0_recv)
}

pub fn panic_to_message(packet: Result<Packet, RecvError>) -> Message {
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

pub fn assert_eq_message(packet: Result<Packet, RecvError>, expected_message: Message) {
    assert_eq!(panic_to_message(packet), expected_message)
}

pub fn test_on_message<T: ServerLogic>(server: &mut T, message: Message, response: Message) {
    let (senders, node0_recv) = setup_node0();

    server.on_message(&senders, &0, message);

    assert_eq_message(node0_recv.recv(), response);
}

pub fn test_on_message_fn<T: ServerLogic>(
    server: &mut T,
    message: Message,
    check: Box<dyn FnOnce(Message) -> ()>,
) {
    let (senders, node0_recv) = setup_node0();

    server.on_message(&senders, &0, message);

    check(panic_to_message(node0_recv.recv()));
}
