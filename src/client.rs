#[cfg(feature = "dev")]
use crate::dev_tools;
use crate::{BoxCollectable, ClientMessage, PROTOCOL_ID, ServerMessage, connection_config};
use bevy::color::palettes::css::{BLUE, YELLOW};
use bevy::prelude::*;

use bevy_renet2::{
    netcode::{ClientAuthentication, NetcodeClientPlugin, NetcodeClientTransport},
    prelude::{RenetClient, RenetClientPlugin, client_connected},
};
use renet2_netcode::NativeSocket;
use std::{
    net::UdpSocket,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn run() {
    let (client, transport) = new_client();
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(
            #[cfg(feature = "dev")]
            dev_tools::plugin,
        )
        .add_plugins(NetcodeClientPlugin)
        .add_plugins(RenetClientPlugin)
        .insert_resource(client)
        .insert_resource(transport)
        .insert_resource(ClientInfo::default())
        .configure_sets(Update, Connected.run_if(client_connected))
        .add_systems(Startup, setup_player)
        .add_systems(Update, move_player)
        .add_systems(Update, (receive_messages, check_collectibles))
        // .add_systems(
        //     PostUpdate,
        //     player_physics_simulation.in_set(PhysicsSet::Writeback),
        // )
        .run();
}

fn new_client() -> (RenetClient, NetcodeClientTransport) {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let native_socket = NativeSocket::new(socket).unwrap();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let client_id = now.as_millis() as u64;

    let auth = ClientAuthentication::Unsecure {
        client_id,
        protocol_id: PROTOCOL_ID,
        server_addr: "127.0.0.1:5000".parse().unwrap(),
        socket_id: 0,
        user_data: None,
    };

    let transport = NetcodeClientTransport::new(now, auth, native_socket).unwrap();
    let client = RenetClient::new(connection_config(), false);
    (client, transport)
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Connected;

fn setup_player(mut commands: Commands) {
    commands.spawn(Camera2d::default());

    commands.spawn((
        Player,
        Transform::from_xyz(0.0, 0.0, 0.0),
        Sprite {
            color: BLUE.into(),
            custom_size: Some(Vec2::splat(30.0)),
            ..default()
        },
    ));
}

fn move_player(keys: Res<ButtonInput<KeyCode>>, time: Res<Time>, mut client: ResMut<RenetClient>) {
    let mut direction = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        direction.y += 1.0;
    }
    if keys.pressed(KeyCode::KeyS) {
        direction.y -= 1.0;
    }
    if keys.pressed(KeyCode::KeyA) {
        direction.x -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) {
        direction.x += 1.0;
    }

    if direction != Vec2::ZERO {
        let dir = direction.normalize_or_zero();
        let delta = time.delta_secs();

        let msg = ClientMessage::MoveInput {
            direction: dir,
            frame: 0,
            delta,
        };
        let bytes = bincode::serde::encode_to_vec(&msg, bincode::config::standard()).unwrap();
        client.send_message(0, bytes);
        // }
    }
}

fn receive_messages(
    mut commands: Commands,
    mut client: ResMut<RenetClient>,
    mut client_info: ResMut<ClientInfo>,
    mut players: Query<(Entity, &mut Transform, Option<&RemotePlayer>), With<Player>>,
    collectible_query: Query<(Entity, &RemoteCollectibleId)>,
) {
    while let Some(bytes) = client.receive_message(0) {
        let Ok((message, _)) = bincode::serde::decode_from_slice::<ServerMessage, _>(
            &bytes,
            bincode::config::standard(),
        ) else {
            error!("Failed to decode server message");
            continue;
        };

        match message {
            ServerMessage::AssignClientId { client_id } => {
                info!("Received client ID: {client_id}");
                client_info.id = Some(client_id);
            }

            ServerMessage::PlayerPositions(player_positions) => {
                for data in player_positions {
                    for (_ent, mut transform, remote) in players.iter_mut() {
                        if Some(data.client_id) == client_info.id {
                            // This is us
                            transform.translation = data.position;
                            transform.rotation = data.rotation;
                            break;
                        }

                        match remote {
                            Some(remote_player) => {
                                if remote_player.client_id == data.client_id {
                                    transform.translation = data.position;
                                    transform.rotation = data.rotation;

                                    break;
                                }
                            }
                            None => {}
                        }
                    }
                }
            }

            ServerMessage::SpawnCollectibles(collectibles) => {
                for info in collectibles {
                    commands.spawn((
                        BoxCollectable,
                        RemoteCollectibleId(info.id),
                        Transform::from_translation(info.position),
                        Sprite {
                            color: YELLOW.into(),
                            custom_size: Some(Vec2::splat(20.0)),
                            ..default()
                        },
                    ));
                }
            }

            ServerMessage::DespawnCollectible { id } => {
                for (entity, box_id) in collectible_query.iter() {
                    if box_id.0 == id {
                        commands.entity(entity).despawn();
                    }
                }
            }

            ServerMessage::SpawnRemotePlayer { client_id } => {
                if Some(client_id) == client_info.id {
                    return;
                }
                let mut found = false;
                for (_, _, remote_player) in players.iter() {
                    match remote_player {
                        Some(player) => {
                            if player.client_id == client_id {
                                found = true;
                            }
                        }
                        None => {}
                    }
                }
                if !found {
                    commands.spawn((
                        Transform::default(),
                        Sprite {
                            color: Color::srgb(0.8, 0.2, 1.0),
                            custom_size: Some(Vec2::splat(30.0)),
                            ..default()
                        },
                        Player,
                        RemotePlayer {
                            client_id: client_id,
                        },
                    ));
                }
            }

            ServerMessage::DespawnPlayer { client_id } => {
                for (entity, _, remote_player) in players.iter() {
                    if let Some(player) = remote_player {
                        if player.client_id == client_id {
                            commands.entity(entity).despawn();
                        }
                    }
                }
            }
        }
    }
}

fn check_collectibles(
    player_query: Query<&Transform, (With<Player>, Without<RemotePlayer>)>,
    boxes: Query<(&Transform, &RemoteCollectibleId)>,
    mut client: ResMut<RenetClient>,
) {
    let Ok(player_transform) = player_query.single() else {
        return;
    };

    for (box_transform, box_id) in &boxes {
        let distance = player_transform
            .translation
            .distance(box_transform.translation);
        if distance < 40.0 {
            let msg = ClientMessage::AttemptCollect { id: box_id.0 };
            let bytes = bincode::serde::encode_to_vec(&msg, bincode::config::standard()).unwrap();
            client.send_message(0, bytes);
        }
    }
}

// === Components and Resources ===

#[derive(Component)]
pub struct Player;

#[derive(Component)]
pub struct RemotePlayer {
    pub client_id: u64,
}

#[derive(Component)]
pub struct RemoteCollectibleId(pub u64);

#[derive(Resource, Default)]
pub struct ClientInfo {
    pub id: Option<u64>,
}
