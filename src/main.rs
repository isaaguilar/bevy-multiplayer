use std::time::Duration;

use bevy::ecs::component::Component;
use renet2::{ChannelConfig, ConnectionConfig, SendType};

mod client;
#[cfg(feature = "dev")]
mod dev_tools;
mod protocol;
mod server;

use protocol::*;

const MAX_ACCELERATION: f32 = 1000.0;
// const MAX_VELOCITY: f32 = 200.0;

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

#[derive(Component)]
pub struct BoxCollectable;
