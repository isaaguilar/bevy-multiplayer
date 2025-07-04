use std::time::Duration;

use bevy::{ecs::component::Component, math::Vec3};
use renet2::{ChannelConfig, ConnectionConfig, SendType};
use serde::{Deserialize, Serialize};

mod client;
mod server;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("server") => server::run(),
        Some("client") => client::run(),
        _ => {
            eprintln!("Usage: cargo run --bin main -- [server|client]");
        }
    }
}

/// This must match on both client and server
pub const PROTOCOL_ID: u64 = 7;

pub fn connection_config() -> ConnectionConfig {
    let channel = ChannelConfig {
        channel_id: 0,
        max_memory_usage_bytes: 1024 * 1024,
        send_type: SendType::ReliableOrdered {
            resend_time: Duration::from_millis(200),
        },
    };

    ConnectionConfig {
        available_bytes_per_tick: 1024 * 1024,
        client_channels_config: vec![channel.clone()],
        server_channels_config: vec![channel],
    }
}

#[derive(Serialize, Deserialize)]
pub enum ServerMessage {
    SpawnCollectibles(Vec<Vec3>),
}

pub enum ServerChannel {
    Collectibles = 0,
}

impl From<ServerChannel> for u8 {
    fn from(channel: ServerChannel) -> Self {
        channel as u8
    }
}

#[derive(Component)]
pub struct BoxCollectable;
