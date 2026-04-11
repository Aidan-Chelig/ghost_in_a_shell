use bevy::render::view::NoIndirectDrawing;
use message_protocol::protocol::{GuestEvent, HostCommand};
use message_protocol::vsock::VsockListener;
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;

mod terminal;
mod vm;

use terminal::TerminalPlugin;
use vm::VmPlugin;

use bevy::prelude::*;

use crate::terminal::{
    TerminalLine, TerminalLineBg, TerminalLineCursor, TerminalLineText, default_fg,
};

fn env_required(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("missing required env var: {name}"))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let asset_dir = std::env::var("HOST_ASSET_DIR").unwrap_or_else(|_| "assets".to_string());

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Bevy + crosvm terminal".into(),
                        resolution: (1200, 800).into(),
                        ..default()
                    }),
                    ..default()
                })
                .set(AssetPlugin {
                    file_path: asset_dir,
                    ..default()
                }),
        )
        .add_plugins((TerminalPlugin, VmPlugin))
        .add_systems(Startup, setup)
        .run();
    Ok(())
}

#[derive(Component)]
struct TerminalRoot;

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((Camera2d));

    let font = asset_server.load("fonts/JetBrainsMonoNerdFont-Regular.ttf");

    let root = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                padding: UiRect::all(Val::Px(12.0)),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(Color::srgb(0.06, 0.06, 0.08)),
            TerminalRoot,
        ))
        .id();

    commands.entity(root).with_children(|parent| {
        for row in 0..40 {
            parent
                .spawn((
                    Node {
                        width: Val::Percent(100.0),
                        min_height: Val::Px(20.0),
                        position_type: PositionType::Relative,
                        ..default()
                    },
                    TerminalLine { row },
                ))
                .with_children(|row_parent| {
                    row_parent.spawn((
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Px(20.0),
                            flex_direction: FlexDirection::Row,
                            position_type: PositionType::Absolute,
                            left: Val::Px(0.0),
                            top: Val::Px(0.0),
                            ..default()
                        },
                        TerminalLineBg { row },
                    ));

                    row_parent.spawn((
                        Text::new(""),
                        TextFont {
                            font: font.clone(),
                            font_size: 18.0,
                            ..default()
                        },
                        TextColor(default_fg()),
                        Node {
                            width: Val::Percent(100.0),
                            min_height: Val::Px(20.0),
                            position_type: PositionType::Absolute,
                            left: Val::Px(0.0),
                            top: Val::Px(0.0),
                            ..default()
                        },
                        TerminalLineText { row },
                    ));

                    row_parent.spawn((
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Px(20.0),
                            position_type: PositionType::Absolute,
                            left: Val::Px(0.0),
                            top: Val::Px(0.0),
                            ..default()
                        },
                        ZIndex(10),
                        TerminalLineCursor { row },
                    ));
                });
        }
    });
}
