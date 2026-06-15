//! Fake mpv JSON-IPC server for integration tests.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Mutex, Notify};
use tokio::task::JoinHandle;

pub struct FakeMpv {
    pub socket_path: PathBuf,
    _tempdir: TempDir,
    state: Arc<Mutex<FakeMpvState>>,
    changed: Arc<Notify>,
    _accept_task: JoinHandle<()>,
}

#[derive(Default)]
struct FakeMpvState {
    loaded_file: Option<String>,
    paused: bool,
    position: f64,
    duration: f64,
    volume: f64,
    playlist_pos: i64,
    properties: HashMap<String, Value>,
    playlist: Vec<String>,
    commands: Vec<Vec<Value>>,
    fail_loadfile: bool,
}

impl FakeMpv {
    pub async fn start() -> Self {
        let tempdir = super::tempdir();
        let socket_path = tempdir.path().join("fake-mpv.sock");
        let listener = UnixListener::bind(&socket_path).expect("bind fake mpv socket");
        let state = Arc::new(Mutex::new(FakeMpvState {
            volume: 100.0,
            duration: 180.0,
            ..Default::default()
        }));
        let changed = Arc::new(Notify::new());
        let state_for_task = state.clone();
        let changed_for_task = changed.clone();
        let accept_task = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let state = state_for_task.clone();
                let changed = changed_for_task.clone();
                tokio::spawn(handle_connection(stream, state, changed));
            }
        });
        Self {
            socket_path,
            _tempdir: tempdir,
            state,
            changed,
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

    /// Wait until `predicate` over captured commands is true or `timeout_ms` elapses.
    pub async fn wait_for<F>(&self, timeout_ms: u64, predicate: F) -> bool
    where
        F: Fn(&[Vec<Value>]) -> bool,
    {
        tokio::time::timeout(
            std::time::Duration::from_millis(timeout_ms),
            async {
                loop {
                    let waiter = self.changed.notified();
                    {
                        let s = self.state.lock().await;
                        if predicate(&s.commands) {
                            return true;
                        }
                    }
                    waiter.await;
                }
            },
        )
        .await
        .unwrap_or(false)
    }

    pub async fn set_duration(&self, secs: f64) {
        {
            let mut s = self.state.lock().await;
            s.duration = secs;
        }
        self.changed.notify_one();
    }

    pub async fn set_property(&self, name: &str, value: Value) {
        {
            let mut s = self.state.lock().await;
            s.properties.insert(name.to_string(), value);
        }
        self.changed.notify_one();
    }

    pub async fn set_loaded_file(&self, path: &str) {
        {
            let mut s = self.state.lock().await;
            s.loaded_file = Some(path.to_string());
            s.playlist = vec![path.to_string()];
        }
        self.changed.notify_one();
    }

    pub async fn set_position(&self, secs: f64) {
        {
            let mut s = self.state.lock().await;
            s.position = secs;
        }
        self.changed.notify_one();
    }

    pub async fn set_playlist_pos(&self, pos: i64) {
        {
            let mut s = self.state.lock().await;
            s.playlist_pos = pos;
        }
        self.changed.notify_one();
    }

    pub async fn set_playlist(&self, items: Vec<String>) {
        {
            let mut s = self.state.lock().await;
            s.playlist = items;
        }
        self.changed.notify_one();
    }

    pub async fn set_fail_loadfile(&self, fail: bool) {
        {
            let mut s = self.state.lock().await;
            s.fail_loadfile = fail;
        }
        self.changed.notify_one();
    }
}

async fn handle_connection(
    stream: UnixStream,
    state: Arc<Mutex<FakeMpvState>>,
    changed: Arc<Notify>,
) {
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
        changed.notify_one();
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
            if s.fail_loadfile {
                return ("loadfile injected failure".into(), None);
            }
            let path = cmd
                .get(1)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let mode = cmd.get(2).and_then(|v| v.as_str()).unwrap_or("replace");
            if mode == "append" {
                s.playlist.push(path);
            } else {
                let start = cmd
                    .get(4)
                    .and_then(|v| v.as_str())
                    .and_then(|opts| opts.split(',').find_map(|kv| kv.strip_prefix("start=")))
                    .and_then(|v| v.parse::<f64>().ok())
                    .unwrap_or(0.0);
                s.loaded_file = Some(path.clone());
                s.playlist.clear();
                s.playlist.push(path);
                s.position = start;
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
                "playlist-pos" => json!(s.playlist_pos),
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
