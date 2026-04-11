use std::{
    env, fs,
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
};

use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender, unbounded};

use message_protocol::protocol::{GuestEvent, HostCommand};
use message_protocol::vsock::VsockListener;

use crate::terminal::TerminalIo;

#[derive(Resource)]
pub struct VmSession {
    pub _child: Child,
}

pub struct VmPlugin;

impl Plugin for VmPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, start_vm_system);
    }
}

fn env_required(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("missing required env var: {name}"))
}

fn start_vm_system(mut commands: Commands) {
    let (to_terminal_tx, to_terminal_rx) = unbounded::<Vec<u8>>();
    let (to_vm_tx, to_vm_rx) = unbounded::<Vec<u8>>();

    commands.insert_resource(TerminalIo {
        rx: to_terminal_rx,
        tx: to_vm_tx,
    });

    let crosvm = env_required("ARGVM_CROSVM");
    let kernel = env_required("ARGVM_KERNEL");
    let initrd = env_required("ARGVM_INITRD");
    let rootfs = env_required("ARGVM_ROOTFS");
    let console = env_required("ARGVM_CONSOLE");

    let vsock_port: u32 = 7000;
    let listener = VsockListener::bind(vsock_port).expect("bind vsock");

    let mut child = Command::new(&crosvm)
        .arg("run")
        .arg("--disable-sandbox")
        .arg("--mem")
        .arg("size=1024")
        .arg("--cpus")
        .arg("num-cores=2")
        .arg("--initrd")
        .arg(&initrd)
        .arg("--block")
        .arg(format!("path={},root=true,ro=false", rootfs))
        .arg("--serial")
        .arg("type=stdout,hardware=serial,num=1,console=true,stdin=true")
        .arg("--vsock")
        .arg("cid=3")
        .arg("--params")
        .arg(format!("console={} loglevel=3", "ttyS0"))
        .arg(&kernel)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn crosvm");

    let stdout = child.stdout.take().expect("stdout");
    let stdin = child.stdin.take().expect("stdin");

    spawn_crosvm_stdout_thread(stdout, to_terminal_tx.clone());
    spawn_crosvm_stdin_thread(stdin, to_vm_rx);
    //TODO: FIX
    //spawn_vsock_thread(listener, to_terminal_tx);

    commands.insert_resource(VmSession { _child: child });
}

fn spawn_crosvm_stdout_thread(mut stdout: impl Read + Send + 'static, tx: Sender<Vec<u8>>) {
    thread::spawn(move || {
        let mut buf = vec![0u8; 16 * 1024];

        loop {
            match stdout.read(&mut buf) {
                Ok(0) => {
                    eprintln!("[vm] crosvm stdout EOF");
                    break;
                }
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        eprintln!("[vm] terminal receiver dropped; stdout thread exiting");
                        break;
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {
                    continue;
                }
                Err(err) => {
                    eprintln!("[vm] crosvm stdout read error: {err}");
                    break;
                }
            }
        }

        eprintln!("[vm] crosvm stdout thread exited");
    });
}

fn spawn_crosvm_stdin_thread(mut stdin: impl Write + Send + 'static, rx: Receiver<Vec<u8>>) {
    thread::spawn(move || {
        while let Ok(buf) = rx.recv() {
            if let Err(err) = stdin.write_all(&buf) {
                eprintln!("[vm] crosvm stdin write error: {err}");
                break;
            }

            if let Err(err) = stdin.flush() {
                eprintln!("[vm] crosvm stdin flush error: {err}");
                break;
            }
        }

        eprintln!("[vm] crosvm stdin thread exited");
    });
}

fn spawn_vsock_thread(listener: VsockListener, tx: Sender<Vec<u8>>) {
    thread::spawn(move || {
        let Ok(mut stream) = listener.accept() else {
            return;
        };

        let mut reader = BufReader::new(&mut stream);
        let mut line = String::new();

        loop {
            line.clear();
            let Ok(n) = reader.read_line(&mut line) else {
                break;
            };
            if n == 0 {
                break;
            }

            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                continue;
            }

            match serde_json::from_str::<GuestEvent>(trimmed) {
                Ok(event) => {
                    let pretty = format!("\r\n[event] {:?}\r\n", event);
                    let _ = tx.send(pretty.into_bytes());

                    let response = serde_json::to_string(&HostCommand::Ack).unwrap() + "\n";
                    let _ = reader.get_mut().write_all(response.as_bytes());
                    let _ = reader.get_mut().flush();
                }
                Err(err) => {
                    let msg = format!(
                        "\r\n[host] bad event JSON: {trimmed}\r\n[host] parse error: {err}\r\n"
                    );
                    let _ = tx.send(msg.into_bytes());
                }
            }
        }
    });
}
