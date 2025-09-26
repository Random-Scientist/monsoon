// meow
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomIdentifier {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelloFeatures {
    pub(crate) shared_playlists: bool,
    pub(crate) chat: bool,
    pub(crate) feature_list: bool,
    pub(crate) readiness: bool,
    pub(crate) managed_rooms: bool,
    // pub(crate) persistent_rooms: bool,
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
    #[serde(with = "transparent")] // inline fields
    Hello(ServerHello),
    // no transparent, set is a subobject
    Set(ServerSet),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerHello {
    username: String,
    version: String,
    motd: String,
    room: RoomIdentifier,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ready {
    username: Option<String>,
    is_ready: Option<bool>,
    manually_initiated: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistIndex {
    user: Option<String>,
    index: Option<u32>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistChange {
    user: Option<String>,
    files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ServerSet {
    #[serde(with = "transparent")]
    Ready(Ready),
    #[serde(with = "transparent")]
    PlaylistChange(PlaylistChange),
    #[serde(with = "transparent")]
    PlaylistIndex(PlaylistIndex),
    #[serde(with = "transparent")]
    User(Value),
}

mod transparent {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub(crate) fn serialize<T, S>(field: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: Serialize,
        S: Serializer,
    {
        serializer.serialize_some(&field)
    }
    pub(crate) fn deserialize<'de, D, T: Deserialize<'de>>(deserializer: D) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
    {
        T::deserialize(deserializer)
    }
}
