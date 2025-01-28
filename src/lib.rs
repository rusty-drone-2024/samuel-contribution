mod chat;
mod media;
mod server;
mod test;
mod text;

pub type ChatServer = server::Server<chat::ChatServer>;
pub type MediaServer = server::Server<media::MediaServer>;
pub type TextServer = server::Server<text::TextServer>;
