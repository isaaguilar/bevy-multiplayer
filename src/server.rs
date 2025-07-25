#[cfg(feature = "dev")]
use crate::dev_tools;
use crate::{
    BoxCollectable, ClientMessage, CollectibleInfo, MAX_ACCELERATION, PROTOCOL_ID, ServerChannel,
    ServerMessage, connection_config,
    protocol::{PositionData, SERVER_HOST},
};
use bevy::{color::palettes::css::YELLOW, platform::collections::HashMap, prelude::*};
use bevy_rapier2d::{
    plugin::{NoUserData, PhysicsSet, RapierPhysicsPlugin},
    prelude::*,
};
use bevy_renet2::{
    netcode::{
        NetcodeServerPlugin, NetcodeServerTransport, ServerAuthentication, ServerSetupConfig,
    },
    prelude::{RenetServer, RenetServerPlugin, ServerEvent},
};
use rand::Rng;
use renet2_netcode::NativeSocket;
use std::{
    net::UdpSocket,
    time::{SystemTime, UNIX_EPOCH},
};

// === Entry Point ===
pub fn run() {
    let (server, transport) = new_server();

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                visible: false,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(100.0))
        .add_plugins(
            #[cfg(feature = "dev")]
            dev_tools::plugin,
        )
        .add_plugins(NetcodeServerPlugin)
        .add_plugins(RenetServerPlugin)
        .insert_resource(server)
        .insert_resource(transport)
        .insert_resource(PlayerEntityMap::default())
        .insert_resource(CollectibleEntityMap::default())
        .insert_resource(LastPlayerPosition::default())
        .add_systems(Startup, setup_world)
        .add_systems(
            Update,
            (
                handle_client_connects,
                receive_from_clients,
                print_server_events,
            ),
        )
        .add_systems(
            PostUpdate,
            broadcast_player_positions.in_set(PhysicsSet::Writeback),
        )
        .run();
}

// === Server Initialization ===
fn new_server() -> (RenetServer, NetcodeServerTransport) {
    let socket = UdpSocket::bind(SERVER_HOST).unwrap();
    let native_socket = NativeSocket::new(socket).unwrap();

    let setup_config = ServerSetupConfig {
        current_time: SystemTime::now().duration_since(UNIX_EPOCH).unwrap(),
        socket_addresses: vec![vec![SERVER_HOST.parse().unwrap()]],
        authentication: ServerAuthentication::Unsecure,
        max_clients: 64,
        protocol_id: PROTOCOL_ID,
    };

    let transport = NetcodeServerTransport::new(setup_config, native_socket).unwrap();
    let server = RenetServer::new(connection_config());

    (server, transport)
}

// === World Setup ===
fn setup_world(
    mut commands: Commands,
    mut config: Query<&mut RapierConfiguration>,
    mut collectible_entities: ResMut<CollectibleEntityMap>,
) {
    commands.spawn(Camera2d::default());

    if let Ok(mut config) = config.single_mut() {
        config.gravity.y = 0.0;
    }

    for i in 1..4 {
        let id = random_u64();
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

        collectible_entities.0.insert(id, entity);
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
        match event {
            ServerEvent::ClientConnected { client_id } => {
                let entity = commands
                    .spawn((
                        Player {
                            client_id: *client_id,
                        },
                        RigidBody::Dynamic,
                        Collider::cuboid(15.0, 15.0),
                        Velocity::linear(Vec2::ZERO),
                        Damping {
                            linear_damping: 5.0,
                            angular_damping: 2.0,
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
                let bytes =
                    bincode::serde::encode_to_vec(&msg, bincode::config::standard()).unwrap();
                server.send_message(*client_id, ServerChannel::World, bytes);

                let msg = ServerMessage::AssignClientId {
                    client_id: *client_id,
                };
                let bytes =
                    bincode::serde::encode_to_vec(&msg, bincode::config::standard()).unwrap();
                server.send_message(*client_id, ServerChannel::World, bytes);

                for player_client_id in player_map.0.keys() {
                    let msg = ServerMessage::SpawnRemotePlayer {
                        client_id: *player_client_id,
                    };
                    let bytes =
                        bincode::serde::encode_to_vec(&msg, bincode::config::standard()).unwrap();

                    server.broadcast_message(ServerChannel::World, bytes);
                }
            }
            ServerEvent::ClientDisconnected { client_id, reason } => {
                if let Some(entity) = player_map.0.remove(client_id) {
                    commands.entity(entity).despawn();

                    let msg = ServerMessage::DespawnPlayer {
                        client_id: *client_id,
                    };
                    let bytes =
                        bincode::serde::encode_to_vec(&msg, bincode::config::standard()).unwrap();

                    server.broadcast_message(ServerChannel::World, bytes);
                    info!("Despawned disconnected player {client_id} ({reason:?})");
                }
            }
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
    mut transforms: Query<&mut Velocity, With<Player>>,
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
                    frame: _frame, //unused
                    delta,
                } => {
                    if let Some(entity) = player_map.0.get(&client_id) {
                        if let Ok(mut velocity) = transforms.get_mut(*entity) {
                            let dir = direction.clamp_length_max(1.0);
                            velocity.linvel += dir * MAX_ACCELERATION * delta;
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

                        // Spawn a new collectable now
                        let mut rng = rand::rng();

                        let id = random_u64();
                        let position = Vec3::new(
                            rng.random_range(-600.0..=600.0),
                            rng.random_range(-300.0..=300.0),
                            0.0,
                        );

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

                        collectible_entities.0.insert(id, entity);

                        let snapshot: Vec<CollectibleInfo> = vec![CollectibleInfo {
                            id: id,
                            position: position,
                        }];

                        let msg = ServerMessage::SpawnCollectibles(snapshot);
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

fn broadcast_player_positions(
    players: Query<(&Player, &Transform)>,
    mut server: ResMut<RenetServer>,
    mut last_player_position: ResMut<LastPlayerPosition>,
) {
    if std::time::Instant::now()
        .duration_since(last_player_position.time)
        .gt(&std::time::Duration::from_millis(33))
    {
        last_player_position.time = std::time::Instant::now();
        let player_positions = players
            .iter()
            .map(|(player, transform)| PositionData {
                client_id: player.client_id,
                position: transform.translation,
                rotation: transform.rotation,
            })
            .collect::<Vec<_>>();

        let msg = ServerMessage::PlayerPositions(player_positions);

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

#[derive(Resource)]
pub struct LastPlayerPosition {
    pub time: std::time::Instant,
}

impl Default for LastPlayerPosition {
    fn default() -> Self {
        Self {
            time: std::time::Instant::now(),
        }
    }
}

// ===== Utils ======
fn random_u64() -> u64 {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();

    let seed = now.as_secs() ^ now.subsec_nanos() as u64;

    const A: u64 = u64::MAX;
    const C: u64 = u64::MIN;

    A.wrapping_mul(seed).wrapping_add(C)
}
