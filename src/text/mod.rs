use std::collections::HashMap;

use common_structs::{
    leaf::{Leaf, LeafCommand, LeafEvent},
    message::{FileWithData, Link, Message, ServerType},
};
use crossbeam_channel::{Receiver, Sender};
use wg_2024::{network::NodeId, packet::Packet};

use crate::server::{Server, ServerLogic, ServerSenders};

pub struct TextServer {
    file_map: HashMap<Link, FileWithData>,
}

impl TextServer {
    pub fn new(file_map: HashMap<Link, FileWithData>) -> Self {
        TextServer { file_map: file_map }
    }
}

impl ServerLogic for TextServer {
    fn on_message(&mut self, senders: &mut ServerSenders, from: NodeId, message: Message) -> () {
        match message {
            Message::ReqServerType => {
                Server::<TextServer>::send_message(
                    senders,
                    from,
                    Message::RespServerType(ServerType::Text),
                );
            }
            Message::ReqFilesList => {
                Server::<TextServer>::send_message(
                    senders,
                    from,
                    Message::RespFilesList(self.file_map.keys().cloned().collect()),
                );
            }
            Message::ReqFile(id) => {
                match self.file_map.get(&id) {
                    Some(file) => Server::<TextServer>::send_message(
                        senders,
                        from,
                        Message::RespFile(file.clone()),
                    ),
                    None => Server::<TextServer>::send_message(senders, from, Message::ErrNotFound),
                };
            }
            _ => {
                Server::<TextServer>::send_message(
                    senders,
                    from,
                    Message::ErrUnsupportedRequestType,
                );
            }
        }
    }
}

impl Leaf for Server<TextServer> {
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
            TextServer::new(HashMap::new()),
        )
    }

    fn run(&mut self) {
        self.run();
    }
}
