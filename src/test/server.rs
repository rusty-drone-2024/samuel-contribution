use std::collections::HashMap;

use crate::server::{Server, ServerLogic};
use common_structs::leaf::{LeafCommand, LeafEvent};
use common_structs::message::Message;
use crossbeam_channel::{unbounded, Sender};
use wg_2024::network::{NodeId, SourceRoutingHeader};
use wg_2024::packet::Packet;

use super::assert_eq_message;

struct EchoServer {}

impl EchoServer {
    pub fn new() -> Self {
        EchoServer {}
    }
}

impl ServerLogic for EchoServer {
    fn on_message(
        &mut self,
        senders: &crate::server::ServerSenders,
        from: &NodeId,
        message: Message,
    ) -> () {
        Server::<EchoServer>::send_message(senders, from, message);
    }
}

#[test]
fn message_parsing() {
    let (controller_send, _test_controller_recv) = unbounded::<LeafEvent>();
    let (_test_controller_send, controller_recv) = unbounded::<LeafCommand>();
    let (test_packet_send, packet_recv) = unbounded::<Packet>();
    let mut packet_send = HashMap::<NodeId, Sender<Packet>>::new();

    let (node0_send, node0_recv) = unbounded::<Packet>();
    packet_send.insert(0, node0_send);

    let mut server = Server::new(
        0,
        controller_send,
        controller_recv,
        packet_recv,
        packet_send,
        EchoServer::new(),
    );

    let message = Message::ReqServerType; // fuzz testing

    message
        .clone()
        .into_fragments()
        .iter()
        .for_each(|fragment| {
            let res = test_packet_send.send(Packet::new_fragment(
                SourceRoutingHeader::new(vec![0], 0),
                0,
                fragment.clone(),
            ));

            assert!(res.is_ok())
        });

    server.update();

    assert_eq_message(node0_recv.recv(), message);
}
