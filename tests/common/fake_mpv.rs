//! Tokio Unix-socket server that pretends to be mpv's JSON-IPC.
//!
//! Models enough of the mpv protocol that `MpvController` and
//! `DaemonCore::play_queue_position` work end-to-end without spawning
//! a real mpv child. Captures every received command so tests can
//! assert on call order.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

pub struct FakeMpv {
    pub socket_path: PathBuf,
    _tempdir: TempDir,
    state: Arc<Mutex<FakeMpvState>>,
    _accept_task: JoinHandle<()>,
}

#[derive(Default)]
struct FakeMpvState {
    loaded_file: Option<String>,
    paused: bool,
    position: f64,
    duration: f64,
    volume: f64,
    properties: HashMap<String, Value>,
    playlist: Vec<String>,
    commands: Vec<Vec<Value>>,
}

impl FakeMpv {
    pub async fn start() -> Self {
        let tempdir = tempfile::tempdir().expect("create tempdir for fake mpv");
        let socket_path = tempdir.path().join("fake-mpv.sock");
        let listener = UnixListener::bind(&socket_path).expect("bind fake mpv socket");
        let state = Arc::new(Mutex::new(FakeMpvState {
            volume: 100.0,
            duration: 180.0,
            ..Default::default()
        }));
        let state_for_task = state.clone();
        let accept_task = tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let state = state_for_task.clone();
                        tokio::spawn(handle_connection(stream, state));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            socket_path,
            _tempdir: tempdir,
            state,
            _accept_task: accept_task,
        }
    }

    pub async fn loaded_file(&self) -> Option<String> {
        self.state.lock().await.loaded_file.clone()
    }

    pub async fn is_paused(&self) -> bool {
        self.state.lock().await.paused
    }

    pub async fn position(&self) -> f64 {
        self.state.lock().await.position
    }

    pub async fn playlist(&self) -> Vec<String> {
        self.state.lock().await.playlist.clone()
    }

    pub async fn commands(&self) -> Vec<Vec<Value>> {
        self.state.lock().await.commands.clone()
    }

    /// Wait until the test predicate fires on the captured commands,
    /// or `timeout_ms` elapses. Useful for polling for async effects.
    pub async fn wait_for<F>(&self, timeout_ms: u64, predicate: F) -> bool
    where
        F: Fn(&[Vec<Value>]) -> bool,
    {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        loop {
            {
                let s = self.state.lock().await;
                if predicate(&s.commands) {
                    return true;
                }
            }
            if std::time::Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    pub async fn set_duration(&self, secs: f64) {
        self.state.lock().await.duration = secs;
    }

    pub async fn set_property(&self, name: &str, value: Value) {
        self.state.lock().await.properties.insert(name.to_string(), value);
    }
}

async fn handle_connection(stream: UnixStream, state: Arc<Mutex<FakeMpvState>>) {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => break,
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let command = req
            .get("command")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let request_id = req.get("request_id").and_then(|v| v.as_u64()).unwrap_or(0);
        let (error, data) = process_command(&state, &command).await;
        let resp = match data {
            Some(d) => json!({ "request_id": request_id, "error": error, "data": d }),
            None => json!({ "request_id": request_id, "error": error }),
        };
        let mut bytes = serde_json::to_vec(&resp).expect("serialize fake mpv response");
        bytes.push(b'\n');
        if writer.write_all(&bytes).await.is_err() {
            break;
        }
    }
}

async fn process_command(
    state: &Arc<Mutex<FakeMpvState>>,
    cmd: &[Value],
) -> (String, Option<Value>) {
    let mut s = state.lock().await;
    s.commands.push(cmd.to_vec());
    let name = cmd.first().and_then(|v| v.as_str()).unwrap_or("");
    match name {
        "loadfile" => {
            let path = cmd
                .get(1)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let mode = cmd.get(2).and_then(|v| v.as_str()).unwrap_or("replace");
            if mode == "append" {
                s.playlist.push(path);
            } else {
                s.loaded_file = Some(path.clone());
                s.playlist.clear();
                s.playlist.push(path);
                s.position = 0.0;
                s.paused = false;
            }
            ("success".into(), None)
        }
        "stop" => {
            s.loaded_file = None;
            s.playlist.clear();
            s.position = 0.0;
            s.paused = false;
            ("success".into(), None)
        }
        "seek" => {
            let pos = cmd.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0);
            let mode = cmd.get(2).and_then(|v| v.as_str()).unwrap_or("relative");
            if mode == "absolute" {
                s.position = pos;
            } else {
                s.position = (s.position + pos).max(0.0);
            }
            ("success".into(), None)
        }
        "set_property" => {
            let prop = cmd.get(1).and_then(|v| v.as_str()).unwrap_or("");
            let value = cmd.get(2).cloned().unwrap_or(Value::Null);
            match prop {
                "pause" => s.paused = value.as_bool().unwrap_or(false),
                "volume" => s.volume = value.as_f64().unwrap_or(100.0),
                other => {
                    s.properties.insert(other.to_string(), value);
                }
            }
            ("success".into(), None)
        }
        "get_property" => {
            let prop = cmd.get(1).and_then(|v| v.as_str()).unwrap_or("");
            let value = match prop {
                "pause" => Value::Bool(s.paused),
                "time-pos" => json!(s.position),
                "duration" => json!(s.duration),
                "volume" => json!(s.volume),
                "playlist-count" => json!(s.playlist.len()),
                "idle-active" => Value::Bool(s.loaded_file.is_none()),
                "eof-reached" => Value::Bool(false),
                other => s.properties.get(other).cloned().unwrap_or(Value::Null),
            };
            ("success".into(), Some(value))
        }
        "playlist-remove" => {
            let idx = cmd.get(1).and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            if idx < s.playlist.len() {
                s.playlist.remove(idx);
            }
            ("success".into(), None)
        }
        "playlist-next" => {
            if !s.playlist.is_empty() {
                s.playlist.remove(0);
                s.loaded_file = s.playlist.first().cloned();
                s.position = 0.0;
            }
            ("success".into(), None)
        }
        _ => ("success".into(), None),
    }
}
