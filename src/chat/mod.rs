use std::collections::{HashMap, HashSet};

use common_structs::{
    leaf::{Leaf, LeafCommand, LeafEvent},
    message::{Message, ServerType},
};
use crossbeam_channel::{Receiver, Sender};
use wg_2024::{network::NodeId, packet::Packet};

use crate::server::{Server, ServerLogic, ServerSenders};

pub struct ChatServer {
    connected_clients: HashSet<NodeId>,
}

impl ChatServer {
    pub fn new(connected_clients: HashSet<NodeId>) -> Self {
        Self { connected_clients }
    }
}

impl ServerLogic for ChatServer {
    fn on_message(
        &mut self,
        senders: &mut ServerSenders,
        from: NodeId,
        message: Message,
        session_id: u64,
    ) -> () {
        match message {
            Message::ReqServerType => {
                Server::<ChatServer>::send_message(
                    senders,
                    from,
                    Message::RespServerType(ServerType::Chat),
                    Some(session_id),
                );
            }
            Message::ReqChatRegistration => {
                self.connected_clients.insert(from);
            }
            Message::ReqChatClients => {
                Server::<ChatServer>::send_message(
                    senders,
                    from,
                    Message::RespClientList(self.connected_clients.clone().into_iter().collect()),
                    Some(session_id),
                );
            }
            Message::ReqChatSend { to, chat_msg } => {
                if !self.connected_clients.contains(&to) {
                    Server::<ChatServer>::send_message(
                        senders,
                        from,
                        Message::ErrNotExistentClient,
                        Some(session_id),
                    );
                    return;
                }

                Server::<ChatServer>::send_message(
                    senders,
                    to,
                    Message::RespChatFrom { from, chat_msg },
                    None,
                );
            }
            _ => {
                Server::<ChatServer>::send_message(
                    senders,
                    from,
                    Message::ErrUnsupportedRequestType,
                    Some(session_id),
                );
            }
        }
    }
}

impl Leaf for Server<ChatServer> {
    fn new(
        id: NodeId,
        controller_send: Sender<LeafEvent>,
        controller_recv: Receiver<LeafCommand>,
        packet_recv: Receiver<Packet>,
        packet_send: HashMap<NodeId, Sender<Packet>>,
    ) -> Self
    where
        Self: Sized,
    {
        Server::new(
            id,
            controller_send,
            controller_recv,
            packet_recv,
            packet_send,
            ChatServer::new(HashSet::new()),
        )
    }

    fn run(&mut self) {
        self.run();
    }
}
