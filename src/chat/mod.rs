use std::collections::{HashMap, HashSet};

use common_structs::{
    leaf::{Leaf, LeafCommand, LeafEvent},
    message::{Message, ServerType},
};
use crossbeam_channel::{Receiver, Sender};
use wg_2024::{network::NodeId, packet::Packet};

use crate::server::{Server, ServerProtocol, ServerSenders};

pub struct ChatServer {
    connected_clients: HashSet<NodeId>,
}

impl ChatServer {
    pub fn new(connected_clients: HashSet<NodeId>) -> Self {
        Self { connected_clients }
    }
}

impl ServerProtocol for ChatServer {
    fn on_message(
        &mut self,
        server: NodeId,
        senders: &mut ServerSenders,
        from: NodeId,
        message: Message,
        session_id: u64,
    ) {
        match message {
            Message::ReqServerType => {
                Server::<ChatServer>::send_message(
                    server,
                    senders,
                    from,
                    Message::RespServerType(ServerType::Chat),
                    Some(session_id),
                );
            }
            Message::ReqChatRegistration => {
                // Add sender to known clients
                self.connected_clients.insert(from);

                for client in self.connected_clients.iter() {
                    Server::<ChatServer>::send_message(
                        server,
                        senders,
                        *client,
                        Message::RespClientList(
                            self.connected_clients.clone().into_iter().collect(),
                        ),
                        None,
                    );
                }
            }
            Message::ReqChatClients => {
                // List known clients
                Server::<ChatServer>::send_message(
                    server,
                    senders,
                    from,
                    Message::RespClientList(self.connected_clients.clone().into_iter().collect()),
                    Some(session_id),
                );
            }
            Message::ReqChatSend { to, chat_msg } => {
                if !self.connected_clients.contains(&to) {
                    // Receiver client has not registered themselves
                    Server::<ChatServer>::send_message(
                        server,
                        senders,
                        from,
                        Message::ErrNotExistentClient,
                        Some(session_id),
                    );
                    return;
                }

                // Forward message to known client
                Server::<ChatServer>::send_message(
                    server,
                    senders,
                    to,
                    Message::RespChatFrom { from, chat_msg },
                    None,
                );
            }
            _ => {
                // Default response
                Server::<ChatServer>::send_message(
                    server,
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
        Server::create(
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
