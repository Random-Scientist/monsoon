use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomIdentifier {
    pub name: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    Hello {
        username: String,
        #[serde(rename = "password")]
        #[serde(skip_serializing_if = "Option::is_none")]
        password_md5: Option<[u8; 16]>,
        room: RoomIdentifier,
        version: String,
        realversion: String,
    },
    Error {
        message: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    Hello {
        username: String,
        version: String,
        motd: String,
        room: RoomIdentifier,
    },
}
