#![cfg(test)]
// Testing of the text protocol implementation

use std::collections::HashMap;

use common_structs::message::{FileWithData, Message, ServerType};

use crate::text::TextServer;

use super::{test_on_message, test_on_message_fn};

#[test]
fn server_type() {
    let mut server = TextServer::new(HashMap::new());
    test_on_message_fn(
        &mut server,
        Message::ReqServerType,
        Box::new(|message| match message {
            Message::RespServerType(ServerType::Text(_)) => {}
            m => {
                panic!("Response is not resp server type text. {}", m);
            }
        }),
    );
}

#[test]
fn file_list() {
    let mut file_map = HashMap::new();
    // NOTE: Should be sorted in order!
    let ids = vec![
        String::from("demo"),
        String::from("test"),
        String::from("🍌"),
    ];
    let file = FileWithData {
        file: String::from("Hello, World!"),
        related_data: HashMap::new(),
    };
    ids.iter().for_each(|id| {
        file_map.insert(id.clone(), file.clone());
    });
    let mut server = TextServer::new(file_map);
    test_on_message_fn(
        &mut server,
        Message::ReqFilesList,
        Box::new(move |message| match message {
            Message::RespFilesList(mut resp_ids) => {
                resp_ids.sort();
                assert_eq!(resp_ids, ids)
            }
            m => panic!("Message was not of type RespFilesList. {}", m),
        }),
    );
}

#[test]
fn file() {
    let mut file_map = HashMap::new();
    let id = String::from("test");
    let file = FileWithData {
        file: String::from("Hello World!"),
        related_data: HashMap::new(),
    };
    file_map.insert(id.clone(), file.clone());
    let mut server = TextServer::new(file_map);
    test_on_message(&mut server, Message::ReqFile(id), Message::RespFile(file));
}

#[test]
fn media_not_found() {
    let id = String::from("test");
    let mut server = TextServer::new(HashMap::new());
    test_on_message(&mut server, Message::ReqFile(id), Message::ErrNotFound);
}
