use crate::{
    BoxCollectable, ClientMessage, CollectibleInfo, PROTOCOL_ID, ServerChannel, ServerMessage,
    connection_config,
};
use bevy::{color::palettes::css::YELLOW, platform::collections::HashMap, prelude::*};
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

// === Entry Point ===
pub fn run() {
    let (server, transport) = new_server();

    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(NetcodeServerPlugin)
        .add_plugins(RenetServerPlugin)
        .insert_resource(server)
        .insert_resource(transport)
        .insert_resource(PlayerEntityMap::default())
        .insert_resource(CollectibleEntityMap::default())
        .add_systems(Startup, setup_world)
        .add_systems(
            Update,
            (
                handle_client_connects,
                receive_from_clients,
                broadcast_player_positions,
                print_server_events,
            ),
        )
        .run();
}

// === Server Initialization ===
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

// === World Setup ===
fn setup_world(mut commands: Commands, mut entity_map: ResMut<CollectibleEntityMap>) {
    commands.spawn(Camera2d::default());

    for i in 1..4 {
        let id = i as u64;
        let position = Vec3::new(i as f32 * 100.0, 0.0, 0.0);

        let entity = commands
            .spawn((
                BoxCollectable,
                CollectibleId(id),
                Transform::from_translation(position),
                Sprite {
                    color: YELLOW.into(),
                    custom_size: Some(Vec2::splat(20.0)),
                    ..default()
                },
            ))
            .id();

        entity_map.0.insert(id, entity);
    }
}

// === Handle New Connections ===
fn handle_client_connects(
    mut events: EventReader<ServerEvent>,
    boxes: Query<(&CollectibleId, &Transform), With<BoxCollectable>>,
    mut server: ResMut<RenetServer>,
    mut player_map: ResMut<PlayerEntityMap>,
    mut commands: Commands,
) {
    for event in events.read() {
        if let ServerEvent::ClientConnected { client_id } = event {
            let entity = commands
                .spawn((
                    Player {
                        client_id: *client_id,
                    },
                    Transform::from_xyz(0.0, 0.0, 0.0),
                    GlobalTransform::default(),
                ))
                .id();

            player_map.0.insert(*client_id, entity);

            let snapshot: Vec<CollectibleInfo> = boxes
                .iter()
                .map(|(id, t)| CollectibleInfo {
                    id: id.0,
                    position: t.translation,
                })
                .collect();

            let msg = ServerMessage::SpawnCollectibles(snapshot);
            let bytes = bincode::serde::encode_to_vec(&msg, bincode::config::standard()).unwrap();
            server.send_message(*client_id, ServerChannel::World, bytes);

            let msg = ServerMessage::AssignClientId {
                client_id: *client_id,
            };
            let bytes = bincode::serde::encode_to_vec(&msg, bincode::config::standard()).unwrap();
            server.send_message(*client_id, ServerChannel::World, bytes);
        }
    }
}

// === Print Events (Optional) ===
fn print_server_events(mut events: EventReader<ServerEvent>) {
    for event in events.read() {
        info!("Server Event: {:?}", event);
    }
}

// === Main Receive Logic ===
fn receive_from_clients(
    mut commands: Commands,
    mut server: ResMut<RenetServer>,
    player_map: Res<PlayerEntityMap>,
    mut collectible_entities: ResMut<CollectibleEntityMap>,
    mut transforms: Query<&mut Transform, With<Player>>,
) {
    for client_id in server.clients_id() {
        while let Some(message) = server.receive_message(client_id, 0u8) {
            let Ok((msg, _)) = bincode::serde::decode_from_slice::<ClientMessage, _>(
                &message,
                bincode::config::standard(),
            ) else {
                continue;
            };

            match msg {
                ClientMessage::MoveInput {
                    direction,
                    frame,
                    delta,
                } => {
                    if let Some(entity) = player_map.0.get(&client_id) {
                        if let Ok(mut transform) = transforms.get_mut(*entity) {
                            let dir = direction.clamp_length_max(1.0);
                            transform.translation += Vec3::new(dir.x, dir.y, 0.0) * 200.0 * delta;

                            let correction = ServerMessage::PlayerCorrection {
                                client_id,
                                frame,
                                position: transform.translation,
                            };

                            let bytes = bincode::serde::encode_to_vec(
                                &correction,
                                bincode::config::standard(),
                            )
                            .unwrap();
                            server.send_message(client_id, ServerChannel::World, bytes);
                        }
                    }
                }

                ClientMessage::AttemptCollect { id } => {
                    if let Some(entity) = collectible_entities.0.get(&id) {
                        commands.entity(*entity).despawn();
                        collectible_entities.0.remove(&id);

                        let msg = ServerMessage::DespawnCollectible { id };
                        let bytes =
                            bincode::serde::encode_to_vec(&msg, bincode::config::standard())
                                .unwrap();
                        server.broadcast_message(ServerChannel::World, bytes);
                    }
                }
            }
        }
    }
}

// === Broadcast Authoritative Player Positions and used to register ===
fn broadcast_player_positions(
    players: Query<(&Transform, &Player)>,
    mut server: ResMut<RenetServer>,
) {
    for (transform, player) in &players {
        let msg = ServerMessage::PlayerPosition {
            client_id: player.client_id,
            position: transform.translation,
        };

        let bytes = bincode::serde::encode_to_vec(&msg, bincode::config::standard()).unwrap();
        server.broadcast_message(ServerChannel::World, bytes);
    }
}

// === Components and Resources ===
#[derive(Component)]
pub struct CollectibleId(pub u64);

#[derive(Resource, Default)]
pub struct CollectibleEntityMap(pub HashMap<u64, Entity>);

#[derive(Component)]
pub struct Player {
    pub client_id: u64,
}

#[derive(Resource, Default)]
pub struct PlayerEntityMap(pub HashMap<u64, Entity>);
