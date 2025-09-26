use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomIdentifier {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloFeatures {
    #[serde(rename = "sharedPlaylists")]
    pub(crate) shared_playlists: bool,
    pub(crate) chat: bool,
    #[serde(rename = "featureList")]
    pub(crate) feature_list: bool,
    pub(crate) readiness: bool,
    #[serde(rename = "managedRooms")]
    pub(crate) managed_rooms: bool,
    // #[serde(rename = "persistentRooms")]
    // pub(crate) persistent_rooms: bool,
    // #[serde(rename = "uiMode")]
    // pub(crate) ui_mode: String,
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
        features: HelloFeatures,
    },
    Set,
    List,
    State,
    Chat,
    TLS,
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
