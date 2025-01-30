#![cfg(test)]
// Testing of the chat protocol implementation

use std::collections::HashSet;

use common_structs::message::{Message, ServerType};

use crate::{chat::ChatServer, server::ServerProtocol};

use super::{assert_eq_message, setup_node0, test_on_message, test_on_message_fn};

#[test]
fn server_type() {
    let mut server = ChatServer::new(HashSet::new());
    test_on_message(
        &mut server,
        Message::ReqServerType,
        Message::RespServerType(ServerType::Chat),
    );
}

#[test]
fn chat_registration() {
    let (mut senders, node0_recv) = setup_node0();

    let mut server = ChatServer::new(HashSet::new());

    server.on_message(&mut senders, 0, Message::ReqChatRegistration, 0);
    server.on_message(&mut senders, 0, Message::ReqChatClients, 1);

    assert_eq_message(node0_recv.recv(), Message::RespClientList(vec![0]));
}

#[test]
fn chat_clients() {
    let ids = [1, 42, 123];
    let connected_clients = HashSet::from(ids.clone());
    let mut server = ChatServer::new(connected_clients);
    test_on_message_fn(
        &mut server,
        Message::ReqChatClients,
        Box::new(move |message| match message {
            Message::RespClientList(mut resp_ids) => {
                resp_ids.sort();
                assert_eq!(resp_ids, ids)
            }
            _ => panic!("Message was not of type RespClientList"),
        }),
    );
}

#[test]
fn chat_send() {
    let connected_clients = HashSet::from([0]);
    let to = 0;
    let chat_msg = String::from("Hello, World!").into_bytes();
    let mut server = ChatServer::new(connected_clients);
    test_on_message(
        &mut server,
        Message::ReqChatSend {
            to,
            chat_msg: chat_msg.clone(),
        },
        Message::RespChatFrom { from: to, chat_msg },
    );
}

#[test]
fn chat_send_not_found() {
    let to = 0;
    let chat_msg = String::from("Hello, World!").into_bytes();
    let mut server = ChatServer::new(HashSet::new());
    test_on_message(
        &mut server,
        Message::ReqChatSend { to, chat_msg },
        Message::ErrNotExistentClient,
    );
}
