use crate::{BoxCollectable, PROTOCOL_ID, ServerChannel, ServerMessage, connection_config};
use bevy::{color::palettes::css::YELLOW, prelude::*};
use bevy_renet2::{
    netcode::{
        NetcodeServerPlugin, NetcodeServerTransport, ServerAuthentication, ServerSetupConfig,
    },
    prelude::{RenetServer, RenetServerPlugin, ServerEvent},
};
use renet2_netcode::NativeSocket;
use std::{
    net::UdpSocket,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn run() {
    let (server, transport) = new_server();

    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(NetcodeServerPlugin)
        .add_plugins(RenetServerPlugin)
        .insert_resource(server)
        .insert_resource(transport)
        .add_systems(Startup, setup_world)
        .add_systems(
            Update,
            (
                handle_client_connects,
                receive_from_clients,
                print_server_events,
            ),
        )
        .run();
}

fn new_server() -> (RenetServer, NetcodeServerTransport) {
    let socket = UdpSocket::bind("127.0.0.1:5000").unwrap();
    let native_socket = NativeSocket::new(socket).unwrap();

    let setup_config = ServerSetupConfig {
        current_time: SystemTime::now().duration_since(UNIX_EPOCH).unwrap(),
        socket_addresses: vec![vec!["127.0.0.1:5000".parse().unwrap()]],
        authentication: ServerAuthentication::Unsecure,
        max_clients: 64,
        protocol_id: PROTOCOL_ID,
    };

    let transport = NetcodeServerTransport::new(setup_config, native_socket).unwrap();
    let server = RenetServer::new(connection_config());

    (server, transport)
}

fn setup_world(mut commands: Commands) {
    commands.spawn(Camera2d::default());

    for i in 1..4 {
        let position = Vec3::new(i as f32 * 100.0, 0.0, 0.0);
        commands.spawn((
            BoxCollectable,
            Transform::from_translation(position),
            Sprite {
                color: YELLOW.into(),
                custom_size: Some(Vec2::splat(20.0)),
                ..default()
            },
        ));
    }
}

fn handle_client_connects(
    mut events: EventReader<ServerEvent>,
    boxes: Query<&Transform, With<BoxCollectable>>,
    mut server: ResMut<RenetServer>,
) {
    for event in events.read() {
        if let ServerEvent::ClientConnected { client_id } = event {
            let positions: Vec<Vec3> = boxes.iter().map(|t| t.translation).collect();
            let message = ServerMessage::SpawnCollectibles(positions);
            let bytes =
                bincode::serde::encode_to_vec(&message, bincode::config::standard()).unwrap();
            server.send_message(*client_id, ServerChannel::Collectibles, bytes);
        }
    }
}

fn print_server_events(mut events: EventReader<ServerEvent>) {
    for event in events.read() {
        info!("Server Event: {:?}", event);
    }
}

fn receive_from_clients(mut server: ResMut<RenetServer>) {
    for client_id in server.clients_id() {
        while let Some(message) = server.receive_message(client_id, 0) {
            println!(
                "Received from client {}: {:?}",
                client_id,
                String::from_utf8_lossy(&message),
            );
        }
    }
}
