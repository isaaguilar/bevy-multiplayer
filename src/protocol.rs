use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Unique protocol ID to identify your game
pub const PROTOCOL_ID: u64 = 0x12345678;

/// Enum describing messages the client can send to the server
#[derive(Serialize, Deserialize, Debug)]
pub enum ClientMessage {
    MoveInput {
        direction: Vec2,
        frame: u32, // logical input frame
        delta: f32,
    },
    AttemptCollect {
        id: u64,
    },
}

/// Enum describing messages the server can send to clients
#[derive(Serialize, Deserialize, Debug)]
pub enum ServerMessage {
    /// Sends all collectibles at login
    SpawnCollectibles(Vec<CollectibleInfo>),

    /// Removes collectible with given ID
    DespawnCollectible { id: u64 },

    /// Informs all clients of player movement
    PlayerPosition { client_id: u64, position: Vec3 },

    /// Informs this client what their assigned client ID is
    AssignClientId { client_id: u64 },

    PlayerCorrection {
        client_id: u64,
        frame: u32,
        position: Vec3,
    },
}

/// Basic info for spawning collectibles client-side
#[derive(Serialize, Deserialize, Debug)]
pub struct CollectibleInfo {
    pub id: u64,
    pub position: Vec3,
}

/// Enum for identifying outbound server channels
#[repr(u8)]
pub enum ServerChannel {
    /// For world snapshots (spawn, despawn, position)
    World = 0,
}

impl From<ServerChannel> for u8 {
    fn from(channel: ServerChannel) -> Self {
        channel as u8
    }
}

/// Enum for identifying inbound client channels
#[repr(u8)]
pub enum ClientChannel {
    /// For input messages
    Input = 0,
}

impl From<ClientChannel> for u8 {
    fn from(channel: ClientChannel) -> Self {
        channel as u8
    }
}
