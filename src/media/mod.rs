use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
};

use common_structs::{
    leaf::{Leaf, LeafCommand, LeafEvent},
    message::{Link, Media, Message, ServerType},
};
use crossbeam_channel::{Receiver, Sender};
use wg_2024::{network::NodeId, packet::Packet};

use crate::server::{Server, ServerProtocol, ServerSenders};

pub struct MediaServer {
    uuid: u64,
    media_map: HashMap<Link, Media>,
}

impl MediaServer {
    pub fn new(media_map: HashMap<Link, Media>) -> Self {
        let mut s = DefaultHasher::new();
        "SamuelMediaServer".hash(&mut s);
        let uuid = s.finish();
        Self { uuid, media_map }
    }
}

impl ServerProtocol for MediaServer {
    fn on_message(
        &mut self,
        senders: &mut ServerSenders,
        from: NodeId,
        message: Message,
        session_id: u64,
    ) -> () {
        match message {
            Message::ReqServerType => {
                Server::<MediaServer>::send_message(
                    senders,
                    from,
                    Message::RespServerType(ServerType::Media(self.uuid)),
                    Some(session_id),
                );
            }
            Message::ReqMedia(id) => {
                match self.media_map.get(&id) {
                    // Media is present in this server
                    Some(media) => Server::<MediaServer>::send_message(
                        senders,
                        from,
                        Message::RespMedia(media.clone()),
                        Some(session_id),
                    ),
                    // Media with that id is not known
                    None => Server::<MediaServer>::send_message(
                        senders,
                        from,
                        Message::ErrNotFound,
                        Some(session_id),
                    ),
                };
            }
            _ => {
                // Default response
                Server::<MediaServer>::send_message(
                    senders,
                    from,
                    Message::ErrUnsupportedRequestType,
                    Some(session_id),
                );
            }
        }
    }
}

impl Leaf for Server<MediaServer> {
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
        // Media available in the network
        let mut media_map = HashMap::new();
        media_map.insert(
            String::from("chicken.jpeg"),
            Vec::from(include_bytes!("chicken.jpeg")),
        );
        Server::create(
            id,
            controller_send,
            controller_recv,
            packet_recv,
            packet_send,
            MediaServer::new(media_map),
        )
    }

    fn run(&mut self) {
        self.run();
    }
}
