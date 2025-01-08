#![cfg(test)]

use std::collections::HashMap;

use common_structs::message::{Message, ServerType};

use crate::media::MediaServer;

use super::test_on_message;

#[test]
fn server_type() {
    let mut server = MediaServer::new(HashMap::new());
    test_on_message(
        &mut server,
        Message::ReqServerType,
        Message::RespServerType(ServerType::Media),
    );
}

#[test]
fn media() {
    let mut media_map = HashMap::new();
    let id = 0;
    let media = vec![1, 2, 3, 4, 5];
    media_map.insert(id, media.clone());
    let mut server = MediaServer::new(media_map);
    test_on_message(
        &mut server,
        Message::ReqMedia(id),
        Message::RespMedia(media),
    );
}

#[test]
fn media_not_found() {
    let id = 0;
    let mut server = MediaServer::new(HashMap::new());
    test_on_message(&mut server, Message::ReqMedia(id), Message::ErrNotFound);
}
