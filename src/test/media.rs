#![cfg(test)]
// Testing of the media protocol implementation

use std::collections::HashMap;

use common_structs::message::{Message, ServerType};

use crate::media::MediaServer;

use super::{test_on_message, test_on_message_fn};

#[test]
fn server_type() {
    let mut server = MediaServer::new(HashMap::new());
    test_on_message_fn(
        &mut server,
        Message::ReqServerType,
        Box::new(|message| match message {
            Message::RespServerType(ServerType::Media(_)) => {}
            m => {
                panic!("Response is not resp server type media. {}", m);
            }
        }),
    );
}

#[test]
fn media() {
    let mut media_map = HashMap::new();
    let id = String::from("test");
    let media = vec![1, 2, 3, 4, 5];
    media_map.insert(id.clone(), media.clone());
    let mut server = MediaServer::new(media_map);
    test_on_message(
        &mut server,
        Message::ReqMedia(id),
        Message::RespMedia(media),
    );
}

#[test]
fn media_not_found() {
    let id = String::from("test");
    let mut server = MediaServer::new(HashMap::new());
    test_on_message(&mut server, Message::ReqMedia(id), Message::ErrNotFound);
}
