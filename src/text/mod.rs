use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
};

use common_structs::{
    leaf::{Leaf, LeafCommand, LeafEvent},
    message::{FileWithData, Link, Message, ServerType},
};
use crossbeam_channel::{Receiver, Sender};
use wg_2024::{network::NodeId, packet::Packet};

use crate::server::{Server, ServerProtocol, ServerSenders};

pub struct TextServer {
    uuid: u64,
    file_map: HashMap<Link, FileWithData>,
}

impl TextServer {
    pub fn new(file_map: HashMap<Link, FileWithData>) -> Self {
        let mut s = DefaultHasher::new();
        "SamuelTextServer".hash(&mut s);
        let uuid = s.finish();
        Self { uuid, file_map }
    }
}

impl ServerProtocol for TextServer {
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
                Server::<TextServer>::send_message(
                    server,
                    senders,
                    from,
                    Message::RespServerType(ServerType::Text(self.uuid)),
                    Some(session_id),
                );
            }
            Message::ReqFilesList => {
                // List files present in this server
                Server::<TextServer>::send_message(
                    server,
                    senders,
                    from,
                    Message::RespFilesList(self.file_map.keys().cloned().collect()),
                    Some(session_id),
                );
            }
            Message::ReqFile(id) => {
                match self.file_map.get(&id) {
                    // File is present in this server
                    Some(file) => Server::<TextServer>::send_message(
                        server,
                        senders,
                        from,
                        Message::RespFile(file.clone()),
                        Some(session_id),
                    ),
                    // File with that id is not known
                    None => Server::<TextServer>::send_message(
                        server,
                        senders,
                        from,
                        Message::ErrNotFound,
                        Some(session_id),
                    ),
                };
            }
            _ => {
                // Default response
                Server::<TextServer>::send_message(
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
        // Files available in the network
        let mut file_map = HashMap::new();
        file_map.insert(
            String::from("helloworld"),
            FileWithData {
                file: String::from("Hello, World!"),
                related_data: HashMap::new(),
            },
        );

        {
            let mut s = DefaultHasher::new();
            "SamuelMediaServer".hash(&mut s);
            let media_uuid = s.finish();
            let mut related_data = HashMap::new();
            related_data.insert(String::from("chicken.jpeg"), media_uuid);
            file_map.insert(
                String::from("plophub"),
                FileWithData {
                    file: String::from("# Plopmenz\n![Profile Picture](chicken.jpeg)"),
                    related_data,
                },
            );
        }

        Server::create(
            id,
            controller_send,
            controller_recv,
            packet_recv,
            packet_send,
            TextServer::new(file_map),
        )
    }

    fn run(&mut self) {
        self.run();
    }
}
