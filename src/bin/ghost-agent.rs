#[path = "../protocol.rs"]
mod protocol;
#[path = "../vsock.rs"]
mod vsock;

use protocol::{GuestEvent, HostCommand, NodeKind, WorldNode};
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};
use vsock::{VMADDR_CID_HOST, VsockStream};

const HOST_PORT: u32 = 7000;
const WORLD_ROOT: &str = "/root/world";
const CWD_FILE: &str = "/run/ghost/current-cwd";

fn read_current_cwd() -> Option<String> {
    let s = fs::read_to_string(CWD_FILE).ok()?;
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn node_kind(path: &Path) -> NodeKind {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            let ft = meta.file_type();
            if ft.is_dir() {
                NodeKind::Directory
            } else if ft.is_file() {
                NodeKind::File
            } else if ft.is_symlink() {
                NodeKind::Symlink
            } else {
                NodeKind::Other
            }
        }
        Err(_) => NodeKind::Other,
    }
}

fn read_ghost_xattrs(path: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();

    let output = match Command::new("getfattr")
        .arg("-d")
        .arg("-m")
        .arg("^user\\.ghost\\.")
        .arg("--absolute-names")
        .arg(path)
        .output()
    {
        Ok(output) => output,
        Err(err) => {
            eprintln!(
                "[ghost-agent] getfattr failed for {}: {err}",
                path.display()
            );
            return out;
        }
    };

    if !output.status.success() {
        eprintln!(
            "[ghost-agent] getfattr non-zero for {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        return out;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    eprintln!("[ghost-agent] xattrs for {}:\n{}", path.display(), text);

    for line in text.lines() {
        let line = line.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some((k, v)) = line.split_once('=') {
            if k.starts_with("user.ghost.") {
                let value = v.trim().trim_matches('"').to_string();
                out.insert(k.to_string(), value);
            }
        }
    }

    out
}

fn scan_world_recursive(root: &Path, current: &Path, nodes: &mut Vec<WorldNode>) {
    let rel = current.strip_prefix(root).unwrap_or(current);
    let rel_path = if rel.as_os_str().is_empty() {
        "/".to_string()
    } else {
        format!("/{}", rel.display())
    };

    nodes.push(WorldNode {
        path: rel_path,
        kind: node_kind(current),
        attrs: read_ghost_xattrs(current),
    });

    let Ok(read_dir) = fs::read_dir(current) else {
        return;
    };

    let mut children: Vec<PathBuf> = read_dir
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .collect();

    children.sort();

    for child in children {
        match fs::symlink_metadata(&child) {
            Ok(meta) if meta.file_type().is_dir() => {
                scan_world_recursive(root, &child, nodes);
            }
            Ok(_) => {
                let rel = child.strip_prefix(root).unwrap_or(&child);
                nodes.push(WorldNode {
                    path: format!("/{}", rel.display()),
                    kind: node_kind(&child),
                    attrs: read_ghost_xattrs(&child),
                });
            }
            Err(_) => {}
        }
    }
}

fn snapshot_world(root: &Path) -> Vec<WorldNode> {
    let mut nodes = Vec::new();

    if root.exists() {
        scan_world_recursive(root, root, &mut nodes);
    }

    nodes
}

fn send_event(
    stream: &mut VsockStream,
    event: &GuestEvent,
) -> Result<(), Box<dyn std::error::Error>> {
    let msg = serde_json::to_string(event)? + "\n";
    stream.write_all(msg.as_bytes())?;
    stream.flush()?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut last_err = None;

    for _ in 0..30 {
        match VsockStream::connect(VMADDR_CID_HOST, HOST_PORT) {
            Ok(mut stream) => {
                stream.set_read_timeout(Some(Duration::from_millis(100)))?;
                send_event(&mut stream, &GuestEvent::Hello { proto: 1 })?;
                send_event(&mut stream, &GuestEvent::BootComplete)?;

                let reader_stream = stream;
                let mut reader = BufReader::new(reader_stream);

                let mut last_cwd: Option<String> = None;
                let mut last_snapshot_json = String::new();
                let mut last_heartbeat = Instant::now();

                loop {
                    if let Some(cwd) = read_current_cwd() {
                        if last_cwd.as_deref() != Some(cwd.as_str()) {
                            send_event(
                                reader.get_mut(),
                                &GuestEvent::CwdChanged { path: cwd.clone() },
                            )?;
                            last_cwd = Some(cwd);
                        }
                    }

                    let root = Path::new(WORLD_ROOT);
                    let nodes = snapshot_world(root);
                    let snapshot_event = GuestEvent::WorldSnapshot {
                        root: WORLD_ROOT.to_string(),
                        nodes,
                    };

                    let snapshot_json = serde_json::to_string(&snapshot_event)?;
                    if snapshot_json != last_snapshot_json {
                        reader.get_mut().write_all(snapshot_json.as_bytes())?;
                        reader.get_mut().write_all(b"\n")?;
                        reader.get_mut().flush()?;
                        last_snapshot_json = snapshot_json;
                    }

                    if last_heartbeat.elapsed() >= Duration::from_secs(5) {
                        send_event(reader.get_mut(), &GuestEvent::Heartbeat)?;
                        last_heartbeat = Instant::now();
                    }

                    let mut line = String::new();
                    match reader.read_line(&mut line) {
                        Ok(0) => break,
                        Ok(_) => {
                            let trimmed = line.trim_end();
                            if !trimmed.is_empty() {
                                match serde_json::from_str::<HostCommand>(trimmed) {
                                    Ok(cmd) => eprintln!("[ghost-agent] host command: {:?}", cmd),
                                    Err(err) => eprintln!("[ghost-agent] bad host command: {err}"),
                                }
                            }
                        }
                        Err(err)
                            if err.kind() == std::io::ErrorKind::WouldBlock
                                || err.kind() == std::io::ErrorKind::TimedOut =>
                        {
                            // no host command right now; continue polling
                        }
                        Err(err) => {
                            eprintln!("[ghost-agent] read error: {err}");
                            break;
                        }
                    }

                    thread::sleep(Duration::from_millis(750));
                }

                return Ok(());
            }
            Err(err) => {
                last_err = Some(err);
                thread::sleep(Duration::from_secs(1));
            }
        }
    }

    Err(format!("failed to connect to host vsock: {:?}", last_err).into())
}
