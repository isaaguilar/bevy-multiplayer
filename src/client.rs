use crate::{BoxCollectable, PROTOCOL_ID, ServerMessage, connection_config};
use bevy::{
    color::palettes::css::{BLUE, YELLOW},
    prelude::*,
};
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
        .configure_sets(Update, Connected.run_if(client_connected))
        .add_systems(Startup, setup_player)
        .add_systems(Update, (move_player, receive_messages).in_set(Connected))
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

#[derive(Component)]
struct Player;

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
        if let Ok(mut transform) = query.single_mut() {
            transform.translation +=
                (direction.normalize() * 200.0 * time.delta_secs()).extend(0.0);

            if client.is_connected() {
                client.send_message(0, "moving");
            }
        }
    }
}

fn receive_messages(mut commands: Commands, mut client: ResMut<RenetClient>) {
    while let Some(bytes) = client.receive_message(0u8) {
        let Ok((message, _)) = bincode::serde::decode_from_slice::<ServerMessage, _>(
            &bytes,
            bincode::config::standard(),
        ) else {
            error!("Failed to decode message from server");
            continue;
        };

        match message {
            ServerMessage::SpawnCollectibles(positions) => {
                for position in positions {
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
        }
    }
}
