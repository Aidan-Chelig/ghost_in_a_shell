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

use bevy::prelude::*;
use terminal::TerminalPlugin;

use crate::terminal::{TerminalLine, spawn_terminal_backend};

fn env_required(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("missing required env var: {name}"))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    //initVM();

    let asset_dir = std::env::var("HOST_ASSET_DIR").unwrap_or_else(|_| "assets".to_string());

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Bevy + alacritty_terminal demo".into(),
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
        .add_plugins(TerminalPlugin)
        .add_systems(Startup, setup)
        .run();
    return Ok(());
}

fn initVM() -> Result<(), Box<dyn std::error::Error>> {
    let crosvm = env_required("ARGVM_CROSVM");
    let kernel = env_required("ARGVM_KERNEL");
    let initrd = env_required("ARGVM_INITRD");
    let rootfs = env_required("ARGVM_ROOTFS");
    let console = env_required("ARGVM_CONSOLE");

    let vsock_port: u32 = 7000;
    let listener = VsockListener::bind(vsock_port)?;
    println!("[host-time] vsock listening on port {vsock_port}");

    let vsock_thread = thread::spawn(
        move || -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            let mut stream = listener.accept()?;
            println!(
                "[host] vsock client connected: cid={} port={}",
                stream.peer_cid, stream.peer_port
            );

            let mut reader = BufReader::new(stream);
            let mut line = String::new();

            loop {
                line.clear();
                let n = reader.read_line(&mut line)?;
                if n == 0 {
                    println!("[host] vsock disconnected");
                    break;
                }

                let trimmed = line.trim_end();
                if trimmed.is_empty() {
                    continue;
                }

                match serde_json::from_str::<GuestEvent>(trimmed) {
                    Ok(event) => {
                        println!("[event] {:?}", event);
                        let response = serde_json::to_string(&HostCommand::Ack)? + "\n";
                        reader.get_mut().write_all(response.as_bytes())?;
                        reader.get_mut().flush()?;
                    }
                    Err(err) => {
                        eprintln!("[host] bad event JSON: {trimmed}");
                        eprintln!("[host] parse error: {err}");
                    }
                }
            }

            Ok(())
        },
    );

    let temp_dir = tempfile::tempdir()?;
    let disk_path: PathBuf = temp_dir.path().join("argvm-x86_64.img");
    fs::copy(&rootfs, &disk_path)?;

    let mut child = Command::new(&crosvm)
        .arg("run")
        .arg("--mem")
        .arg("size=1024")
        .arg("--cpus")
        .arg("num-cores=2")
        .arg("--initrd")
        .arg(&initrd)
        .arg("--block")
        .arg(format!("path={},root=true,ro=false", disk_path.display()))
        .arg("--serial")
        .arg("type=stdout,hardware=serial,num=1,console=true,stdin=true")
        .arg("--vsock")
        .arg("cid=3")
        .arg("--params")
        .arg(format!("console={} loglevel=7", console))
        .arg(&kernel)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;

    if let Some(stdout) = child.stdout.take() {
        let reader_thread = thread::spawn(
            move || -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
                let mut reader = BufReader::new(stdout);
                let mut buf = [0u8; 4096];
                let mut pending = String::new();

                loop {
                    let n = reader.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }

                    pending.push_str(&String::from_utf8_lossy(&buf[..n]));

                    while let Some(pos) = pending.find('\n') {
                        let line: String = pending.drain(..=pos).collect();
                        print!("[vm] {line}");
                    }
                }

                if !pending.is_empty() {
                    print!("[vm] {pending}");
                }

                Ok(())
            },
        );

        let status = child.wait()?;
        let _ = reader_thread.join();

        if !status.success() {
            return Err(format!("crosvm exited with status: {status}").into());
        }
    }

    let _ = vsock_thread.join();
    Ok(())
}

#[derive(Component)]
struct TerminalRoot;

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((Camera2d, NoIndirectDrawing));

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
            parent.spawn((
                Text::new(""),
                TextFont {
                    font: font.clone(),
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::srgb(0.85, 0.85, 0.85)),
                Node {
                    width: Val::Percent(100.0),
                    min_height: Val::Px(20.0),
                    ..default()
                },
                TerminalLine { row },
            ));
        }
    });

    spawn_terminal_backend(&mut commands);
}
