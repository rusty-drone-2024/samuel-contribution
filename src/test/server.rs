#![cfg(test)]
// Testing of the protocol-independent server implementation

use std::collections::HashMap;
use std::time::Duration;

use crate::server::{Server, ServerProtocol};
use crate::test::panic_to_message_multi;
use common_structs::leaf::{LeafCommand, LeafEvent};
use common_structs::message::Message;
use common_structs::types::Routing;
use crossbeam_channel::{unbounded, Sender};
use wg_2024::network::{NodeId, SourceRoutingHeader};
use wg_2024::packet::{Ack, FloodRequest, Nack, NackType, NodeType, Packet, PacketType};

struct EchoServer {}

impl EchoServer {
    pub fn new() -> Self {
        EchoServer {}
    }
}

/// Send back any messages that we receive
impl ServerProtocol for EchoServer {
    fn on_message(
        &mut self,
        server: NodeId,
        senders: &mut crate::server::ServerSenders,
        from: NodeId,
        message: Message,
        session_id: u64,
    ) {
        Server::<EchoServer>::send_message(server, senders, from, message, Some(session_id));
    }
}

#[test]
fn fragments() {
    let (controller_send, _test_controller_recv) = unbounded::<LeafEvent>();
    let (_test_controller_send, controller_recv) = unbounded::<LeafCommand>();
    let (test_packet_send, packet_recv) = unbounded::<Packet>();
    let mut packet_send = HashMap::<NodeId, Sender<Packet>>::new();

    let (node0_send, node0_recv) = unbounded::<Packet>();
    packet_send.insert(0, node0_send);

    let mut server = Server::create(
        0,
        controller_send,
        controller_recv,
        packet_recv,
        packet_send,
        EchoServer::new(),
    );

    let message = Message::ReqChatSend {
        to: 0,
        chat_msg: String::from(
"Lorem ipsum dolor sit amet, consectetur adipiscing elit. Nullam non ultrices leo, ac dictum erat. In laoreet dui id augue placerat tempor. Aliquam nulla sem, tempor et aliquet tincidunt, iaculis eget lectus. Nam sagittis sodales augue in rhoncus. Sed eget augue at justo tempus dapibus. Nullam sodales diam ut eros finibus accumsan. Donec euismod eget odio eget suscipit. Suspendisse non sapien nec libero commodo euismod vitae vel orci. In sed ex sagittis, fringilla dui sed, gravida orci. Aliquam imperdiet vestibulum est, eget sagittis nibh porta vel. Pellentesque ut rhoncus nisl, vel varius purus. Proin mi sapien, bibendum eget quam eget, convallis euismod ex. Praesent fermentum lorem vitae tincidunt elementum. Pellentesque nisi orci, faucibus vitae neque eget, consequat vulputate tellus. Sed lacus nisl, consectetur at luctus vel, molestie at justo.

Donec a porta dolor. Phasellus vulputate risus nec porttitor dictum. Curabitur augue nulla, pharetra vel metus bibendum, sollicitudin posuere nunc. Vestibulum nec orci in turpis varius cursus. Suspendisse in suscipit dolor, non fringilla justo. Mauris ac quam dictum lorem interdum semper. Pellentesque velit erat, sagittis ultrices sapien id, vulputate consectetur justo.

Nullam porta scelerisque tortor at feugiat. Donec at elit sit amet sem ullamcorper luctus eget ac ante. Mauris diam libero, vestibulum ac elit sit amet, facilisis pharetra nibh. Cras mattis nulla a nunc lobortis aliquam. Nulla quis consequat odio. Vivamus elementum mattis pulvinar. Nunc mattis sem vel velit feugiat ornare. Ut porta nibh vitae ligula ultrices tristique. Duis ac lectus non augue laoreet commodo. Duis ac aliquet lacus, id maximus felis. Mauris finibus quam vitae felis egestas, id sollicitudin ligula vestibulum. Quisque dignissim massa sed tellus ultricies, sed tempus purus lobortis. Duis feugiat ligula nec diam ullamcorper malesuada.

Integer et consequat est. Ut aliquam urna ut scelerisque dignissim. Sed porta varius viverra. Mauris fermentum dictum metus, eget efficitur est euismod eu. Maecenas ut vestibulum eros. Etiam non sagittis dui. Cras tempus odio vitae est dapibus fermentum sed vitae tellus. Aenean lacus libero, elementum at consequat sit amet, mattis non ante. Mauris quis pellentesque ligula. Integer tempus fermentum vestibulum.

Etiam varius tortor vitae tincidunt rutrum. In tortor mauris, imperdiet malesuada cursus gravida, vehicula ut eros. Aliquam consequat mattis tincidunt. Integer dapibus lobortis ante, vitae suscipit mi rhoncus sit amet. Nunc eleifend venenatis euismod. Integer blandit tempus dapibus. Praesent vitae libero id lacus porttitor aliquet. Nam hendrerit sollicitudin libero, eget ultrices quam suscipit quis. Duis lacinia, sapien ut aliquam malesuada, turpis sapien lobortis ipsum, ut malesuada ligula arcu bibendum neque. Nullam sed libero eget diam eleifend sollicitudin non et mi. Curabitur vel ante non lacus placerat elementum id eget orci. Maecenas a dapibus nibh. Nam sed nulla quis lorem tincidunt sodales. Proin sit amet est augue. Sed leo ex, laoreet quis nulla eget, lacinia feugiat ex.").into_bytes(),
    }; // Message can be fuzz tested for full coverage

    let fragments = message.clone().into_fragments();
    let fragment_count = fragments.len();
    let session_id = 777;
    for fragment in fragments {
        let res = test_packet_send.send(Packet::new_fragment(
            SourceRoutingHeader::with_first_hop(vec![0, 0]),
            session_id,
            fragment,
        ));

        assert!(res.is_ok())
    }

    for i in 0..fragment_count {
        server.update();
        match node0_recv.recv_timeout(Duration::from_millis(10)) {
            Ok(p) => {
                assert_eq!(p.session_id, session_id);
                assert_eq!(
                    p.pack_type,
                    PacketType::Ack(Ack {
                        fragment_index: i as u64
                    })
                );
                println!("Recieved {}", i);
            }
            Err(e) => panic!("Did not receive packet (expected ACK): {}", e),
        }
    }

    let mut received_packets = Vec::with_capacity(fragment_count);
    for _ in 0..fragment_count {
        let packet = node0_recv.recv_timeout(Duration::from_millis(10));
        received_packets.push(packet);
    }
    assert_eq!(panic_to_message_multi(received_packets), message);
}

#[test]
fn flood_request() {
    let (controller_send, _test_controller_recv) = unbounded::<LeafEvent>();
    let (_test_controller_send, controller_recv) = unbounded::<LeafCommand>();
    let (test_packet_send, packet_recv) = unbounded::<Packet>();
    let mut packet_send = HashMap::<NodeId, Sender<Packet>>::new();

    let (node0_send, node0_recv) = unbounded::<Packet>();
    packet_send.insert(0, node0_send);

    let mut server = Server::create(
        0,
        controller_send,
        controller_recv,
        packet_recv,
        packet_send,
        EchoServer::new(),
    );

    let flood_id = 123;
    let path_trace = vec![(0, NodeType::Client)];
    let session_id = 777;
    assert!(test_packet_send
        .send(Packet::new_flood_request(
            Routing::empty_route(),
            session_id,
            FloodRequest {
                flood_id,
                initiator_id: 0,
                path_trace: path_trace.clone(),
            },
        ))
        .is_ok());

    server.update();

    match node0_recv.recv_timeout(Duration::from_millis(10)) {
        Ok(packet) => match packet.pack_type {
            PacketType::FloodResponse(resp) => {
                assert_eq!(resp.flood_id, flood_id);
                assert_eq!(
                    resp.path_trace,
                    [path_trace, vec![(0, NodeType::Server)]].concat()
                )
            }
            _ => panic!("Packet is not a flood response."),
        },
        Err(e) => panic!("No packet could be received: {}", e),
    }
}

#[test]
fn nack() {
    let (controller_send, _test_controller_recv) = unbounded::<LeafEvent>();
    let (_test_controller_send, controller_recv) = unbounded::<LeafCommand>();
    let (test_packet_send, packet_recv) = unbounded::<Packet>();
    let mut packet_send = HashMap::<NodeId, Sender<Packet>>::new();

    let (node0_send, node0_recv) = unbounded::<Packet>();
    packet_send.insert(0, node0_send);

    let mut server = Server::create(
        0,
        controller_send,
        controller_recv,
        packet_recv,
        packet_send,
        EchoServer::new(),
    );

    let message = Message::ReqChatSend {
        to: 0,
        chat_msg: String::from(
"Lorem ipsum dolor sit amet, consectetur adipiscing elit. Nullam non ultrices leo, ac dictum erat. In laoreet dui id augue placerat tempor. Aliquam nulla sem, tempor et aliquet tincidunt, iaculis eget lectus. Nam sagittis sodales augue in rhoncus. Sed eget augue at justo tempus dapibus. Nullam sodales diam ut eros finibus accumsan. Donec euismod eget odio eget suscipit. Suspendisse non sapien nec libero commodo euismod vitae vel orci. In sed ex sagittis, fringilla dui sed, gravida orci. Aliquam imperdiet vestibulum est, eget sagittis nibh porta vel. Pellentesque ut rhoncus nisl, vel varius purus. Proin mi sapien, bibendum eget quam eget, convallis euismod ex. Praesent fermentum lorem vitae tincidunt elementum. Pellentesque nisi orci, faucibus vitae neque eget, consequat vulputate tellus. Sed lacus nisl, consectetur at luctus vel, molestie at justo.

Donec a porta dolor. Phasellus vulputate risus nec porttitor dictum. Curabitur augue nulla, pharetra vel metus bibendum, sollicitudin posuere nunc. Vestibulum nec orci in turpis varius cursus. Suspendisse in suscipit dolor, non fringilla justo. Mauris ac quam dictum lorem interdum semper. Pellentesque velit erat, sagittis ultrices sapien id, vulputate consectetur justo.

Nullam porta scelerisque tortor at feugiat. Donec at elit sit amet sem ullamcorper luctus eget ac ante. Mauris diam libero, vestibulum ac elit sit amet, facilisis pharetra nibh. Cras mattis nulla a nunc lobortis aliquam. Nulla quis consequat odio. Vivamus elementum mattis pulvinar. Nunc mattis sem vel velit feugiat ornare. Ut porta nibh vitae ligula ultrices tristique. Duis ac lectus non augue laoreet commodo. Duis ac aliquet lacus, id maximus felis. Mauris finibus quam vitae felis egestas, id sollicitudin ligula vestibulum. Quisque dignissim massa sed tellus ultricies, sed tempus purus lobortis. Duis feugiat ligula nec diam ullamcorper malesuada.

Integer et consequat est. Ut aliquam urna ut scelerisque dignissim. Sed porta varius viverra. Mauris fermentum dictum metus, eget efficitur est euismod eu. Maecenas ut vestibulum eros. Etiam non sagittis dui. Cras tempus odio vitae est dapibus fermentum sed vitae tellus. Aenean lacus libero, elementum at consequat sit amet, mattis non ante. Mauris quis pellentesque ligula. Integer tempus fermentum vestibulum.

Etiam varius tortor vitae tincidunt rutrum. In tortor mauris, imperdiet malesuada cursus gravida, vehicula ut eros. Aliquam consequat mattis tincidunt. Integer dapibus lobortis ante, vitae suscipit mi rhoncus sit amet. Nunc eleifend venenatis euismod. Integer blandit tempus dapibus. Praesent vitae libero id lacus porttitor aliquet. Nam hendrerit sollicitudin libero, eget ultrices quam suscipit quis. Duis lacinia, sapien ut aliquam malesuada, turpis sapien lobortis ipsum, ut malesuada ligula arcu bibendum neque. Nullam sed libero eget diam eleifend sollicitudin non et mi. Curabitur vel ante non lacus placerat elementum id eget orci. Maecenas a dapibus nibh. Nam sed nulla quis lorem tincidunt sodales. Proin sit amet est augue. Sed leo ex, laoreet quis nulla eget, lacinia feugiat ex.").into_bytes(),
    }; // Message can be fuzz tested for full coverage

    let fragments = message.clone().into_fragments();
    let fragment_count = fragments.len();
    let session_id = 777;
    for fragment in fragments.clone() {
        let res = test_packet_send.send(Packet::new_fragment(
            SourceRoutingHeader::with_first_hop(vec![0, 0]),
            session_id,
            fragment,
        ));

        assert!(res.is_ok())
    }

    for i in 0..fragment_count {
        server.update();
        match node0_recv.recv_timeout(Duration::from_millis(10)) {
            Ok(p) => {
                assert_eq!(p.session_id, session_id);
                assert_eq!(
                    p.pack_type,
                    PacketType::Ack(Ack {
                        fragment_index: i as u64
                    })
                );
            }
            Err(e) => panic!("Did not receive packet (expected ACK): {}", e),
        }
    }

    let mut received_packets = Vec::with_capacity(fragment_count);
    for _ in 0..fragment_count {
        let packet = node0_recv.recv_timeout(Duration::from_millis(10));
        received_packets.push(packet);
    }
    assert_eq!(panic_to_message_multi(received_packets), message);

    let nack_index = 2;
    assert!(test_packet_send
        .send(Packet::new_nack(
            Routing::empty_route(),
            session_id,
            Nack {
                fragment_index: nack_index,
                nack_type: NackType::Dropped
            },
        ))
        .is_ok());

    server.update();

    match node0_recv.recv_timeout(Duration::from_millis(10)) {
        Ok(packet) => match packet.pack_type {
            PacketType::MsgFragment(fragment) => {
                assert_eq!(fragment.fragment_index, nack_index);

                let index: usize = nack_index
                    .try_into()
                    .expect("Nack index does not fit in usize.");
                assert_eq!(fragment, fragments[index]);
            }
            _ => panic!("Packet is not a fragment."),
        },
        Err(e) => panic!("No packet could be received: {}", e),
    }
}
