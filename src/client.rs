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
        .add_plugins(NetcodeClientPlugin)
        .add_plugins(RenetClientPlugin)
        .insert_resource(client)
        .insert_resource(transport)
        .insert_resource(ClientInfo::default())
        .insert_resource(InputHistory::default())
        .configure_sets(Update, Connected.run_if(client_connected))
        .add_systems(Startup, setup_player)
        .add_systems(
            Update,
            (move_player, receive_messages, check_collectibles).in_set(Connected),
        )
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

fn move_player(
    keys: Res<ButtonInput<KeyCode>>,
    mut query: Query<&mut Transform, With<Player>>,
    time: Res<Time>,
    mut client: ResMut<RenetClient>,
    mut history: ResMut<InputHistory>,
) {
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
        let predicted_step = (dir * 200.0 * delta).extend(0.0);

        if let Ok(mut transform) = query.single_mut() {
            transform.translation += predicted_step;

            let frame = history.frame_counter;
            history.history.push((frame, dir, transform.translation));
            history.frame_counter += 1;

            let msg = ClientMessage::MoveInput {
                direction: dir,
                frame,
                delta,
            };
            let bytes = bincode::serde::encode_to_vec(&msg, bincode::config::standard()).unwrap();
            client.send_message(0, bytes);
        }
    }
}

fn receive_messages(
    mut commands: Commands,
    mut client: ResMut<RenetClient>,
    mut client_info: ResMut<ClientInfo>,
    mut remote_players: Query<(Entity, &mut Transform, &RemotePlayer)>,
    mut local_player: Query<&mut Transform, (With<Player>, Without<RemotePlayer>)>,
    collectible_query: Query<(Entity, &RemoteCollectibleId)>,
    mut history: ResMut<InputHistory>,
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

            ServerMessage::PlayerCorrection {
                client_id,
                frame,
                position: server_pos,
            } => {
                if Some(client_id) != client_info.id {
                    return; // Not ours
                }

                if let Ok(mut transform) = local_player.single_mut() {
                    if history.history.iter().any(|(f, _, _)| *f == frame) {
                        let dist = transform.translation.distance(server_pos);
                        if dist > 0.1 {
                            transform.translation = transform.translation.lerp(server_pos, 0.5);
                        }
                        history.history.retain(|(f, _, _)| *f > frame);
                    } else {
                        transform.translation = server_pos;
                        history.history.clear();
                    }
                }
            }

            ServerMessage::PlayerPosition {
                client_id,
                position,
            } => {
                if Some(client_id) == client_info.id {
                    return;
                }

                let mut found = false;
                for (_ent, mut transform, remote) in remote_players.iter_mut() {
                    if remote.client_id == client_id {
                        transform.translation = position;
                        found = true;
                        break;
                    }
                }

                if !found {
                    commands.spawn((
                        Transform::from_translation(position),
                        Sprite {
                            color: Color::srgb(0.8, 0.2, 1.0),
                            custom_size: Some(Vec2::splat(24.0)),
                            ..default()
                        },
                        RemotePlayer { client_id },
                    ));
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
        }
    }
}

fn check_collectibles(
    player_query: Query<&Transform, With<Player>>,
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

#[derive(Resource, Default)]
struct InputHistory {
    frame_counter: u32,
    history: Vec<(u32, Vec2, Vec3)>,
}
