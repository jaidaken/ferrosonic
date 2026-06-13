//! mpv process ownership and JSON IPC control.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::UnixStream;
use tokio::sync::{oneshot, Mutex as TokioMutex};
use tokio::time::{sleep, timeout};
use tracing::{debug, info, trace, warn};

use crate::config::paths::mpv_socket_path;
use crate::error::AudioError;

/// Overall deadline for a single `send_command`. Without this, a hung
/// mpv would freeze every audio operation since the controller mutex
/// serialises all IPC.
const COMMAND_DEADLINE: Duration = Duration::from_secs(5);

type PendingMap = Arc<TokioMutex<HashMap<u64, oneshot::Sender<Result<Option<Value>, AudioError>>>>>;
const EVENT_CHANNEL_CAP: usize = 64;

#[derive(Debug, Serialize)]
struct MpvCommand {
    command: Vec<Value>,
    request_id: u64,
}

#[derive(Debug, Deserialize)]
struct MpvResponse {
    #[serde(default)]
    request_id: Option<u64>,
    #[serde(default)]
    data: Option<Value>,
    #[serde(default)]
    error: String,
}

#[derive(Debug, Deserialize)]
struct MpvEvent {
    event: String,
    #[serde(default)]
    reason: Option<String>,
}

/// Typed mpv event surface for daemon consumers; raw event fields stay private.
#[derive(Debug, Clone)]
pub enum MpvEventKind {
    /// Playback of the current file ended.
    EndFile {
        /// mpv's end reason, e.g. `"eof"` or `"stop"`.
        reason: String,
    },
    /// mpv started loading a new file.
    StartFile,
    /// The new file finished loading and playback begins.
    FileLoaded,
    /// Any other mpv event, carrying its raw name.
    Other(String),
}

/// Owner of the mpv child process and its JSON IPC socket.
pub struct MpvController {
    socket_path: PathBuf,
    process: Option<Child>,
    request_id: AtomicU64,
    writer: Option<OwnedWriteHalf>,
    /// Outstanding requests keyed by request_id; reader task resolves.
    pending: PendingMap,
    /// Background reader task; aborted on disconnect/shutdown.
    reader_handle: Option<tokio::task::JoinHandle<()>>,
    /// Broadcast of typed mpv events to daemon consumers.
    event_tx: tokio::sync::broadcast::Sender<MpvEventKind>,
}

/// Make `cmd`'s child receive `SIGKILL` when this process dies, even on a
/// SIGKILL or crash where `Drop` never runs; without it an orphaned mpv keeps
/// holding the audio device. Linux-only; a no-op elsewhere.
pub fn set_die_with_parent(cmd: &mut Command) {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: the closure runs in the forked child before exec; prctl,
        // getppid and _exit are async-signal-safe.
        unsafe {
            cmd.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
                if libc::getppid() == 1 {
                    libc::_exit(1);
                }
                Ok(())
            });
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = cmd;
}

impl MpvController {
    /// Construct against the default runtime-dir socket path.
    pub fn new() -> Self {
        Self::with_socket_path(mpv_socket_path())
    }

    /// Test seam: point the controller at a specific socket path. Does not spawn mpv or connect; call [`start`](Self::start) or [`connect_to_existing`](Self::connect_to_existing) to begin IPC.
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use ferrosonic::audio::mpv::MpvController;
    /// let mut ctrl = MpvController::with_socket_path(PathBuf::from("/tmp/ferrosonic-doctest.sock"));
    /// assert!(!ctrl.is_running(), "fresh controller has no IPC yet");
    /// ```
    pub fn with_socket_path(socket_path: PathBuf) -> Self {
        let (event_tx, _) = tokio::sync::broadcast::channel(EVENT_CHANNEL_CAP);
        Self {
            socket_path,
            process: None,
            request_id: AtomicU64::new(1),
            writer: None,
            pending: Arc::new(TokioMutex::new(HashMap::new())),
            reader_handle: None,
            event_tx,
        }
    }

    /// Subscribe to the typed event stream. Multiple subscribers are supported; each gets every event from subscription onwards. Channel capacity is fixed at [`EVENT_CHANNEL_CAP`]; slow consumers see `RecvError::Lagged`.
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use ferrosonic::audio::mpv::MpvController;
    /// let ctrl = MpvController::with_socket_path(PathBuf::from("/tmp/ferrosonic-doctest-sub.sock"));
    /// let rx = ctrl.subscribe_events();
    /// assert_eq!(rx.len(), 0, "no events have been emitted yet");
    /// ```
    pub fn subscribe_events(&self) -> tokio::sync::broadcast::Receiver<MpvEventKind> {
        self.event_tx.subscribe()
    }

    /// Test seam: connect to an mpv socket that's already listening.
    pub async fn connect_to_existing(&mut self) -> Result<(), AudioError> {
        if !self.socket_path.exists() {
            return Err(AudioError::MpvIpc(format!(
                "Socket {} does not exist",
                self.socket_path.display()
            )));
        }
        self.connect().await
    }

    /// Spawn mpv (if not already alive) and connect to its IPC socket.
    pub async fn start(&mut self) -> Result<(), AudioError> {
        // Reap an exited child so a fresh mpv can be spawned. Without
        // this, an mpv crash leaves self.process = Some(<exited Child>)
        // and start_mpv() silently no-ops on every subsequent call.
        if let Some(child) = self.process.as_mut() {
            match child.try_wait() {
                Ok(None) => return Ok(()),
                Ok(Some(status)) => {
                    warn!("mpv exited ({:?}), respawning", status);
                    self.tear_down_connection().await;
                }
                Err(e) => {
                    // try_wait Err means the process state is unknown;
                    // treat as dead and respawn rather than silently
                    // returning Ok and leaving the daemon half-broken.
                    warn!("mpv try_wait failed ({}), forcing respawn", e);
                    self.tear_down_connection().await;
                }
            }
        }
        let _ = std::fs::remove_file(&self.socket_path);
        info!("Starting MPV with socket: {}", self.socket_path.display());

        let mut cmd = Command::new("mpv");
        cmd.arg("--idle")
            .arg("--no-video")
            .arg("--no-terminal")
            .arg("--gapless-audio=yes")
            .arg("--prefetch-playlist=yes")
            .arg("--cache=yes")
            .arg("--cache-secs=120")
            .arg("--demuxer-max-bytes=100MiB")
            // No --audio-stream-silence: let PipeWire suspend the device
            // when paused/idle so the system sample rate can change.
            // Don't pause while waiting for the initial cache to fill —
            // start playback as soon as the decoder has bytes, which is
            // what we want for a music TUI.
            .arg("--cache-pause-initial=no")
            // And don't pause on cache underrun later either.
            .arg("--cache-pause=no")
            .arg(format!("--input-ipc-server={}", self.socket_path.display()))
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        set_die_with_parent(&mut cmd);
        let child = cmd.spawn().map_err(AudioError::MpvSpawn)?;
        self.process = Some(child);

        for _ in 0..50 {
            if self.socket_path.exists() {
                sleep(Duration::from_millis(50)).await;
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }

        if !self.socket_path.exists() {
            return Err(AudioError::MpvIpc("Socket not created".to_string()));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(
                &self.socket_path,
                std::fs::Permissions::from_mode(0o600),
            );
        }

        self.connect().await?;
        info!("MPV started successfully");
        Ok(())
    }

    async fn connect(&mut self) -> Result<(), AudioError> {
        let stream = UnixStream::connect(&self.socket_path)
            .await
            .map_err(AudioError::MpvSocket)?;
        let (read_half, write_half) = stream.into_split();
        self.writer = Some(write_half);

        let pending = self.pending.clone();
        let events = self.event_tx.clone();
        let handle = tokio::spawn(reader_loop(BufReader::new(read_half), pending, events));
        self.reader_handle = Some(handle);

        debug!("Connected to MPV socket");
        Ok(())
    }

    async fn tear_down_connection(&mut self) {
        if let Some(h) = self.reader_handle.take() {
            h.abort();
        }
        self.writer = None;
        self.process = None;
        // Fail any in-flight requests so callers don't hang.
        let mut p = self.pending.lock().await;
        for (_, tx) in p.drain() {
            let _ = tx.send(Err(AudioError::MpvIpc("connection torn down".to_string())));
        }
    }

    /// Whether the IPC connection and mpv process are both alive; clears dead state as a side effect.
    pub fn is_running(&mut self) -> bool {
        if self.writer.is_none() {
            return false;
        }
        // Reader task may have ended after a socket close; if so, drop
        // the writer too so callers see a consistent dead state.
        if let Some(h) = self.reader_handle.as_ref() {
            if h.is_finished() {
                self.reader_handle = None;
                self.writer = None;
                self.process = None;
                return false;
            }
        }
        match self.process.as_mut() {
            None => self.writer.is_some(),
            Some(child) => match child.try_wait() {
                Ok(None) => true,
                Ok(Some(_)) => {
                    self.writer = None;
                    self.process = None;
                    if let Some(h) = self.reader_handle.take() {
                        h.abort();
                    }
                    false
                }
                Err(_) => {
                    self.writer = None;
                    self.process = None;
                    if let Some(h) = self.reader_handle.take() {
                        h.abort();
                    }
                    false
                }
            },
        }
    }

    async fn send_command(&mut self, args: Vec<Value>) -> Result<Option<Value>, AudioError> {
        let request_id = self.request_id.fetch_add(1, Ordering::SeqCst);
        let cmd = MpvCommand {
            command: args,
            request_id,
        };
        let mut json = serde_json::to_vec(&cmd)?;
        json.push(b'\n');
        debug!("Sending MPV command (req {})", request_id);

        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(request_id, tx);

        {
            let Some(writer) = self.writer.as_mut() else {
                self.pending.lock().await.remove(&request_id);
                return Err(AudioError::MpvNotRunning);
            };
            if let Err(e) = writer.write_all(&json).await {
                self.pending.lock().await.remove(&request_id);
                return Err(AudioError::MpvIpc(e.to_string()));
            }
            if let Err(e) = writer.flush().await {
                self.pending.lock().await.remove(&request_id);
                return Err(AudioError::MpvIpc(e.to_string()));
            }
        }

        match timeout(COMMAND_DEADLINE, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => {
                // Sender dropped without sending: reader task exited.
                Err(AudioError::MpvIpc("reader task ended".to_string()))
            }
            Err(_) => {
                self.pending.lock().await.remove(&request_id);
                Err(AudioError::MpvIpc(format!(
                    "mpv command timeout after {:?} (req {})",
                    COMMAND_DEADLINE, request_id
                )))
            }
        }
    }

    /// Replace the playlist with `path` and start playing it.
    pub async fn loadfile(&mut self, path: &str) -> Result<(), AudioError> {
        info!("Loading: {}", path.split('?').next().unwrap_or(path));
        self.send_command(vec![json!("loadfile"), json!(path), json!("replace")])
            .await?;
        Ok(())
    }

    /// Append `path` to the playlist without interrupting playback.
    pub async fn loadfile_append(&mut self, path: &str) -> Result<(), AudioError> {
        debug!(
            "Appending to playlist: {}",
            path.split('?').next().unwrap_or(path)
        );
        self.send_command(vec![json!("loadfile"), json!(path), json!("append")])
            .await?;
        Ok(())
    }

    /// Remove the playlist entry at `index`.
    pub async fn playlist_remove(&mut self, index: usize) -> Result<(), AudioError> {
        debug!("Removing playlist entry {}", index);
        self.send_command(vec![json!("playlist-remove"), json!(index)])
            .await?;
        Ok(())
    }

    /// Advance to the next playlist entry, forcing past the last one.
    pub async fn playlist_next(&mut self) -> Result<(), AudioError> {
        debug!("Advancing to next playlist entry");
        // `force` advances even at the last entry; we always control
        // the playlist so this is safe.
        self.send_command(vec![json!("playlist-next"), json!("force")])
            .await?;
        Ok(())
    }

    /// Current playlist position, or `None` when nothing is loaded.
    pub async fn get_playlist_pos(&mut self) -> Result<Option<i64>, AudioError> {
        let data = self
            .send_command(vec![json!("get_property"), json!("playlist-pos")])
            .await?;
        Ok(data.and_then(|v| v.as_i64()))
    }

    /// Number of playlist entries; 0 when unavailable.
    pub async fn get_playlist_count(&mut self) -> Result<usize, AudioError> {
        let data = self
            .send_command(vec![json!("get_property"), json!("playlist-count")])
            .await?;
        Ok(data.and_then(|v| v.as_u64()).unwrap_or(0) as usize)
    }

    /// Pause playback. Idempotent if already paused.
    pub async fn pause(&mut self) -> Result<(), AudioError> {
        debug!("Pausing playback");
        self.send_command(vec![json!("set_property"), json!("pause"), json!(true)])
            .await?;
        Ok(())
    }

    /// Resume playback. Idempotent if already playing.
    pub async fn resume(&mut self) -> Result<(), AudioError> {
        debug!("Resuming playback");
        self.send_command(vec![json!("set_property"), json!("pause"), json!(false)])
            .await?;
        Ok(())
    }

    /// Flip the pause state; returns `true` when playback is now paused.
    pub async fn toggle_pause(&mut self) -> Result<bool, AudioError> {
        let paused = self.is_paused().await?;
        if paused {
            self.resume().await?;
        } else {
            self.pause().await?;
        }
        Ok(!paused)
    }

    /// Whether playback is currently paused; `false` when unknown.
    pub async fn is_paused(&mut self) -> Result<bool, AudioError> {
        let data = self
            .send_command(vec![json!("get_property"), json!("pause")])
            .await?;
        Ok(data.and_then(|v| v.as_bool()).unwrap_or(false))
    }

    /// Stop playback and unload the current file.
    pub async fn stop(&mut self) -> Result<(), AudioError> {
        debug!("Stopping playback");
        self.send_command(vec![json!("stop")]).await?;
        Ok(())
    }

    /// Seek to an absolute position in seconds.
    pub async fn seek(&mut self, position: f64) -> Result<(), AudioError> {
        debug!("Seeking to {:.1}s", position);
        self.send_command(vec![json!("seek"), json!(position), json!("absolute")])
            .await?;
        Ok(())
    }

    /// Seek by a signed offset in seconds from the current position.
    pub async fn seek_relative(&mut self, offset: f64) -> Result<(), AudioError> {
        debug!("Seeking {:+.1}s", offset);
        self.send_command(vec![json!("seek"), json!(offset), json!("relative")])
            .await?;
        Ok(())
    }

    /// Playback position in seconds; 0.0 when unknown.
    pub async fn get_time_pos(&mut self) -> Result<f64, AudioError> {
        let data = self
            .send_command(vec![json!("get_property"), json!("time-pos")])
            .await?;
        Ok(data.and_then(|v| v.as_f64()).unwrap_or(0.0))
    }

    /// Track duration in seconds; 0.0 when unknown.
    pub async fn get_duration(&mut self) -> Result<f64, AudioError> {
        let data = self
            .send_command(vec![json!("get_property"), json!("duration")])
            .await?;
        Ok(data.and_then(|v| v.as_f64()).unwrap_or(0.0))
    }

    /// Set playback volume, clamped to 0-100.
    pub async fn set_volume(&mut self, volume: i32) -> Result<(), AudioError> {
        debug!("Setting volume to {}", volume);
        self.send_command(vec![
            json!("set_property"),
            json!("volume"),
            json!(volume.clamp(0, 100)),
        ])
        .await?;
        Ok(())
    }

    /// Decoded sample rate in Hz of the playing track.
    pub async fn get_sample_rate(&mut self) -> Result<Option<u32>, AudioError> {
        let data = self
            .send_command(vec![
                json!("get_property"),
                json!("audio-params/samplerate"),
            ])
            .await?;
        Ok(data.and_then(|v| v.as_u64()).map(|v| v as u32))
    }

    /// Bit depth inferred from mpv's audio format string.
    pub async fn get_bit_depth(&mut self) -> Result<Option<u32>, AudioError> {
        let data = self
            .send_command(vec![json!("get_property"), json!("audio-params/format")])
            .await?;
        let format = data.and_then(|v| v.as_str().map(String::from));
        Ok(format.and_then(|f| {
            if f.contains("32") || f.contains("float") {
                Some(32)
            } else if f.contains("24") {
                Some(24)
            } else if f.contains("16") {
                Some(16)
            } else if f.contains("8") {
                Some(8)
            } else {
                None
            }
        }))
    }

    /// Raw mpv audio format string, e.g. `"s32"` or `"floatp"`.
    pub async fn get_audio_format(&mut self) -> Result<Option<String>, AudioError> {
        let data = self
            .send_command(vec![json!("get_property"), json!("audio-params/format")])
            .await?;
        Ok(data.and_then(|v| v.as_str().map(String::from)))
    }

    /// Channel layout label, e.g. `"Stereo"` or `"5ch"`.
    pub async fn get_channels(&mut self) -> Result<Option<String>, AudioError> {
        let data = self
            .send_command(vec![
                json!("get_property"),
                json!("audio-params/channel-count"),
            ])
            .await?;
        let count = data.and_then(|v| v.as_u64()).map(|v| v as u32);
        Ok(count.map(|c| match c {
            1 => "Mono".to_string(),
            2 => "Stereo".to_string(),
            n => format!("{}ch", n),
        }))
    }

    /// Whether mpv reports idle (nothing loaded); `true` when unknown.
    pub async fn is_idle(&mut self) -> Result<bool, AudioError> {
        let data = self
            .send_command(vec![json!("get_property"), json!("idle-active")])
            .await?;
        Ok(data.and_then(|v| v.as_bool()).unwrap_or(true))
    }

    /// Sync teardown for Drop. No graceful quit IPC (would need async).
    fn shutdown_sync(&mut self) {
        if let Some(mut child) = self.process.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(h) = self.reader_handle.take() {
            h.abort();
        }
        self.writer = None;
        let _ = std::fs::remove_file(&self.socket_path);
        info!("MPV shut down");
    }

    /// Ask mpv to quit gracefully, then force-kill and clean up.
    pub async fn quit(&mut self) -> Result<(), AudioError> {
        if self.writer.is_some() {
            let _ = self.send_command(vec![json!("quit")]).await;
        }
        self.shutdown_sync();
        Ok(())
    }
}

impl Drop for MpvController {
    fn drop(&mut self) {
        self.shutdown_sync();
    }
}

impl Default for MpvController {
    fn default() -> Self {
        Self::new()
    }
}

/// Demuxes responses to oneshots; forwards typed events to subscribers.
async fn reader_loop(
    mut reader: BufReader<OwnedReadHalf>,
    pending: PendingMap,
    events: tokio::sync::broadcast::Sender<MpvEventKind>,
) {
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => {
                debug!("mpv reader: socket closed");
                break;
            }
            Ok(_) => {
                if let Ok(resp) = serde_json::from_str::<MpvResponse>(&line) {
                    if let Some(req_id) = resp.request_id {
                        if let Some(tx) = pending.lock().await.remove(&req_id) {
                            let payload = if resp.error == "success" {
                                Ok(resp.data)
                            } else {
                                Err(AudioError::MpvIpc(resp.error))
                            };
                            let _ = tx.send(payload);
                            continue;
                        }
                    }
                }
                if let Ok(event) = serde_json::from_str::<MpvEvent>(&line) {
                    trace!("MPV event: {:?}", event);
                    let kind = classify_event(&event);
                    let _ = events.send(kind);
                }
            }
            Err(e) => {
                debug!("mpv reader: read error: {}", e);
                break;
            }
        }
    }
}

fn classify_event(ev: &MpvEvent) -> MpvEventKind {
    match ev.event.as_str() {
        "end-file" => MpvEventKind::EndFile {
            reason: ev.reason.clone().unwrap_or_else(|| "unknown".into()),
        },
        "start-file" => MpvEventKind::StartFile,
        "file-loaded" => MpvEventKind::FileLoaded,
        other => MpvEventKind::Other(other.to_string()),
    }
}

#[cfg(test)]
mod fuzz {
    use super::*;

    /// Arbitrary bytes must never panic either reply parser.
    #[test]
    fn fuzz_mpv_reply_never_panics() {
        bolero::check!().with_type::<Vec<u8>>().for_each(|input| {
            let _ = serde_json::from_slice::<MpvResponse>(input);
            let _ = serde_json::from_slice::<MpvEvent>(input);
        });
    }
}
