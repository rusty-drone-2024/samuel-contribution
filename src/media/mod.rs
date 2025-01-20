use std::collections::HashMap;

use common_structs::{
    leaf::{Leaf, LeafCommand, LeafEvent},
    message::{Link, Media, Message, ServerType},
};
use crossbeam_channel::{Receiver, Sender};
use wg_2024::{network::NodeId, packet::Packet};

use crate::server::{Server, ServerLogic, ServerSenders};

pub struct MediaServer {
    media_map: HashMap<Link, Media>,
}

impl MediaServer {
    pub fn new(media_map: HashMap<Link, Media>) -> Self {
        Self { media_map }
    }
}

impl ServerLogic for MediaServer {
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
                    Message::RespServerType(ServerType::Media),
                    Some(session_id),
                );
            }
            Message::ReqMedia(id) => {
                match self.media_map.get(&id) {
                    Some(media) => Server::<MediaServer>::send_message(
                        senders,
                        from,
                        Message::RespMedia(media.clone()),
                        Some(session_id),
                    ),
                    None => Server::<MediaServer>::send_message(
                        senders,
                        from,
                        Message::ErrNotFound,
                        Some(session_id),
                    ),
                };
            }
            _ => {
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
        Server::new(
            id,
            controller_send,
            controller_recv,
            packet_recv,
            packet_send,
            MediaServer::new(HashMap::new()),
        )
    }

    fn run(&mut self) {
        self.run();
    }
}
