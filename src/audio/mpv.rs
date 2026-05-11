use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::UnixStream;
use tokio::time::{sleep, timeout};
use tracing::{debug, info, trace};

use crate::config::paths::mpv_socket_path;
use crate::error::AudioError;

const READ_TIMEOUT: Duration = Duration::from_millis(100);

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
#[allow(dead_code)]
struct MpvEvent {
    event: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    data: Option<Value>,
}

pub struct MpvController {
    socket_path: PathBuf,
    process: Option<Child>,
    request_id: AtomicU64,
    reader: Option<BufReader<OwnedReadHalf>>,
    writer: Option<OwnedWriteHalf>,
}

impl MpvController {
    pub fn new() -> Self {
        Self::with_socket_path(mpv_socket_path())
    }

    /// Test seam: point the controller at a specific socket path.
    pub fn with_socket_path(socket_path: PathBuf) -> Self {
        Self {
            socket_path,
            process: None,
            request_id: AtomicU64::new(1),
            reader: None,
            writer: None,
        }
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

    pub async fn start(&mut self) -> Result<(), AudioError> {
        if self.process.is_some() {
            return Ok(());
        }
        let _ = std::fs::remove_file(&self.socket_path);
        info!("Starting MPV with socket: {}", self.socket_path.display());

        let child = Command::new("mpv")
            .arg("--idle")
            .arg("--no-video")
            .arg("--no-terminal")
            .arg("--gapless-audio=yes")
            .arg("--prefetch-playlist=yes")
            .arg("--cache=yes")
            .arg("--cache-secs=120")
            .arg("--demuxer-max-bytes=100MiB")
            // Keep the audio device open across track swaps so the
            // hardware doesn't re-initialise and produce a click/silence
            // window on every loadfile. mpv emits real PCM silence
            // during the swap rather than dropping the device.
            .arg("--audio-stream-silence=yes")
            // Don't pause while waiting for the initial cache to fill —
            // start playback as soon as the decoder has bytes, which is
            // what we want for a music TUI.
            .arg("--cache-pause-initial=no")
            // And don't pause on cache underrun later either.
            .arg("--cache-pause=no")
            .arg(format!("--input-ipc-server={}", self.socket_path.display()))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(AudioError::MpvSpawn)?;
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

        self.connect().await?;
        info!("MPV started successfully");
        Ok(())
    }

    async fn connect(&mut self) -> Result<(), AudioError> {
        let stream = UnixStream::connect(&self.socket_path)
            .await
            .map_err(AudioError::MpvSocket)?;
        let (read_half, write_half) = stream.into_split();
        self.reader = Some(BufReader::new(read_half));
        self.writer = Some(write_half);
        debug!("Connected to MPV socket");
        Ok(())
    }

    pub fn is_running(&mut self) -> bool {
        if self.reader.is_none() {
            return false;
        }
        match self.process.as_mut() {
            // Test seam: live IPC, no spawned child.
            None => self.writer.is_some(),
            Some(child) => match child.try_wait() {
                Ok(None) => true,
                Ok(Some(_)) => {
                    self.reader = None;
                    self.writer = None;
                    self.process = None;
                    false
                }
                Err(_) => true,
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

        {
            let writer = self.writer.as_mut().ok_or(AudioError::MpvNotRunning)?;
            writer
                .write_all(&json)
                .await
                .map_err(|e| AudioError::MpvIpc(e.to_string()))?;
            writer
                .flush()
                .await
                .map_err(|e| AudioError::MpvIpc(e.to_string()))?;
        }

        let reader = self.reader.as_mut().ok_or(AudioError::MpvNotRunning)?;
        let mut line = String::new();
        loop {
            line.clear();
            match timeout(READ_TIMEOUT, reader.read_line(&mut line)).await {
                Ok(Ok(0)) => return Err(AudioError::MpvIpc("Socket closed".to_string())),
                Ok(Ok(_)) => {
                    if let Ok(resp) = serde_json::from_str::<MpvResponse>(&line) {
                        if resp.request_id == Some(request_id) {
                            if resp.error != "success" {
                                return Err(AudioError::MpvIpc(resp.error));
                            }
                            return Ok(resp.data);
                        }
                    }
                    if let Ok(event) = serde_json::from_str::<MpvEvent>(&line) {
                        trace!("MPV event: {:?}", event);
                    }
                }
                Ok(Err(e)) => return Err(AudioError::MpvIpc(e.to_string())),
                Err(_) => continue,
            }
        }
    }

    pub async fn loadfile(&mut self, path: &str) -> Result<(), AudioError> {
        info!("Loading: {}", path.split('?').next().unwrap_or(path));
        self.send_command(vec![json!("loadfile"), json!(path), json!("replace")])
            .await?;
        Ok(())
    }

    pub async fn loadfile_append(&mut self, path: &str) -> Result<(), AudioError> {
        debug!(
            "Appending to playlist: {}",
            path.split('?').next().unwrap_or(path)
        );
        self.send_command(vec![json!("loadfile"), json!(path), json!("append")])
            .await?;
        Ok(())
    }

    pub async fn playlist_remove(&mut self, index: usize) -> Result<(), AudioError> {
        debug!("Removing playlist entry {}", index);
        self.send_command(vec![json!("playlist-remove"), json!(index)])
            .await?;
        Ok(())
    }

    pub async fn playlist_next(&mut self) -> Result<(), AudioError> {
        debug!("Advancing to next playlist entry");
        // `force` advances even at the last entry; we always control
        // the playlist so this is safe.
        self.send_command(vec![json!("playlist-next"), json!("force")])
            .await?;
        Ok(())
    }

    pub async fn get_playlist_pos(&mut self) -> Result<Option<i64>, AudioError> {
        let data = self
            .send_command(vec![json!("get_property"), json!("playlist-pos")])
            .await?;
        Ok(data.and_then(|v| v.as_i64()))
    }

    pub async fn get_playlist_count(&mut self) -> Result<usize, AudioError> {
        let data = self
            .send_command(vec![json!("get_property"), json!("playlist-count")])
            .await?;
        Ok(data.and_then(|v| v.as_u64()).unwrap_or(0) as usize)
    }

    pub async fn pause(&mut self) -> Result<(), AudioError> {
        debug!("Pausing playback");
        self.send_command(vec![json!("set_property"), json!("pause"), json!(true)])
            .await?;
        Ok(())
    }

    pub async fn resume(&mut self) -> Result<(), AudioError> {
        debug!("Resuming playback");
        self.send_command(vec![json!("set_property"), json!("pause"), json!(false)])
            .await?;
        Ok(())
    }

    pub async fn toggle_pause(&mut self) -> Result<bool, AudioError> {
        let paused = self.is_paused().await?;
        if paused {
            self.resume().await?;
        } else {
            self.pause().await?;
        }
        Ok(!paused)
    }

    pub async fn is_paused(&mut self) -> Result<bool, AudioError> {
        let data = self
            .send_command(vec![json!("get_property"), json!("pause")])
            .await?;
        Ok(data.and_then(|v| v.as_bool()).unwrap_or(false))
    }

    pub async fn stop(&mut self) -> Result<(), AudioError> {
        debug!("Stopping playback");
        self.send_command(vec![json!("stop")]).await?;
        Ok(())
    }

    pub async fn seek(&mut self, position: f64) -> Result<(), AudioError> {
        debug!("Seeking to {:.1}s", position);
        self.send_command(vec![json!("seek"), json!(position), json!("absolute")])
            .await?;
        Ok(())
    }

    pub async fn seek_relative(&mut self, offset: f64) -> Result<(), AudioError> {
        debug!("Seeking {:+.1}s", offset);
        self.send_command(vec![json!("seek"), json!(offset), json!("relative")])
            .await?;
        Ok(())
    }

    pub async fn get_time_pos(&mut self) -> Result<f64, AudioError> {
        let data = self
            .send_command(vec![json!("get_property"), json!("time-pos")])
            .await?;
        Ok(data.and_then(|v| v.as_f64()).unwrap_or(0.0))
    }

    pub async fn get_duration(&mut self) -> Result<f64, AudioError> {
        let data = self
            .send_command(vec![json!("get_property"), json!("duration")])
            .await?;
        Ok(data.and_then(|v| v.as_f64()).unwrap_or(0.0))
    }

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

    pub async fn get_sample_rate(&mut self) -> Result<Option<u32>, AudioError> {
        let data = self
            .send_command(vec![
                json!("get_property"),
                json!("audio-params/samplerate"),
            ])
            .await?;
        Ok(data.and_then(|v| v.as_u64()).map(|v| v as u32))
    }

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

    pub async fn get_audio_format(&mut self) -> Result<Option<String>, AudioError> {
        let data = self
            .send_command(vec![json!("get_property"), json!("audio-params/format")])
            .await?;
        Ok(data.and_then(|v| v.as_str().map(String::from)))
    }

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
        self.reader = None;
        self.writer = None;
        let _ = std::fs::remove_file(&self.socket_path);
        info!("MPV shut down");
    }

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
