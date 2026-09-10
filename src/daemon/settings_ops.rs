//! Settings + server-config setters; broadcast `ConfigChanged` on commit.

use std::sync::Arc;

use tracing::{error, warn};

use crate::daemon::core::DaemonCore;
use crate::error::Error;
use crate::ipc::protocol::{DaemonEvent, PasswordStorage};
use crate::subsonic::SubsonicClient;

impl DaemonCore {
    /// Persist new credentials, swap in a fresh Subsonic client, refresh the
    /// library. The password is stored in priority: an existing `PasswordEval`
    /// or `PasswordFile` is honored, otherwise the OS keychain, falling back to
    /// an inline owner-only config write when no keychain is reachable
    /// (`Unavailable`); a reachable-but-failing backend (`Backend`) is
    /// surfaced rather than silently downgraded to plaintext. Returns where
    /// the password landed so the caller can inform the user.
    ///
    /// Persistence happens before the live state is mutated, and all keychain
    /// and file I/O runs outside the `state` lock, so a slow Secret Service or
    /// fsync cannot stall playback or the UI mirror. A failed persist leaves
    /// the prior config and client intact.
    ///
    /// # Errors
    /// Returns an `Error` if persisting the config or a follow-up server request fails.
    pub async fn update_server_config(
        self: &Arc<Self>,
        base_url: &str,
        username: &str,
        password: &crate::secret::Secret,
    ) -> Result<PasswordStorage, Error> {
        // Snapshot under a short read lock; never hold `state` across the
        // keychain/file/persist work below.
        let (mut candidate, old_url, old_user, had_keyring, music_folder_id, eval_present, pf_opt) = {
            let state = self.state.read().await;
            (
                state.config.clone(),
                state.config.base_url.clone(),
                state.config.username.clone(),
                state.config.password_keyring,
                state.config.music_folder_id,
                state.config.password_eval.is_some(),
                state.config.password_file.clone().filter(|s| !s.is_empty()),
            )
        };
        candidate.base_url = base_url.to_string();
        candidate.username = username.to_string();
        // The user explicitly committed a credential, so it is no longer the
        // transient `FERROSONIC_PASSWORD` env override and may be persisted.
        candidate.password_from_env = false;

        // Resolve where the secret lands, performing the secret side effect
        // first. The candidate's inline `password` is cleared for external
        // storage so a failed save can never leak the plaintext to disk.
        let storage = if eval_present {
            candidate.password_keyring = false;
            candidate.password = crate::secret::Secret::new();
            PasswordStorage::PasswordEval
        } else if let Some(pf) = pf_opt.as_deref() {
            if let Err(e) = crate::config::write_password_file_atomic(pf, password) {
                error!("Failed to write password to {}: {}", pf, e);
                return Err(Error::Io(e));
            }
            candidate.password_keyring = false;
            candidate.password = crate::secret::Secret::new();
            PasswordStorage::PasswordFile
        } else {
            match crate::secret_store::store(base_url, username, password) {
                Ok(()) => {
                    candidate.password_keyring = true;
                    candidate.password = crate::secret::Secret::new();
                    PasswordStorage::Keyring
                }
                Err(crate::secret_store::KeyStoreError::Unavailable(e)) => {
                    warn!("OS keychain unavailable ({e}); writing the password inline to the owner-only config file");
                    candidate.password_keyring = false;
                    candidate.password = password.clone();
                    PasswordStorage::Inline
                }
                Err(e @ crate::secret_store::KeyStoreError::Backend(_)) => {
                    // A reachable-but-failing keychain must not silently
                    // downgrade to a plaintext write; keep the old config and
                    // report the failure so the user can retry.
                    error!("OS keychain error while storing credentials: {e}");
                    return Err(Error::KeyStore(e));
                }
            }
        };

        // Persist before touching live state.
        if let Err(e) = candidate.save_default() {
            if storage == PasswordStorage::Keyring && (old_url != base_url || old_user != username)
            {
                let _ = crate::secret_store::delete(base_url, username);
            }
            return Err(Error::Config(e));
        }

        // Validate the URL up front so a malformed base_url cannot leave the
        // persisted config and live client disagreeing.
        let mut new_client = match SubsonicClient::new(base_url, username, password) {
            Ok(client) => client,
            Err(e) => {
                if storage == PasswordStorage::Keyring
                    && (old_url != base_url || old_user != username)
                {
                    let _ = crate::secret_store::delete(base_url, username);
                }
                return Err(Error::Subsonic(e));
            }
        };
        new_client.set_music_folder(music_folder_id);

        // Commit the persisted config, keeping the typed password live for the
        // client (the file itself omits it for external storage).
        {
            let mut state = self.state.write().await;
            let mut committed = candidate;
            committed.password = password.clone();
            state.config = committed;
        }

        // Drop an orphaned old keychain entry now the new config is committed.
        let key_changed = old_url != base_url || old_user != username;
        if had_keyring && (storage != PasswordStorage::Keyring || key_changed) {
            if let Err(e) = crate::secret_store::delete(&old_url, &old_user) {
                warn!("Could not remove the previous keychain entry: {e}");
            }
        }

        {
            // R4: bump gen before installing client, both under subsonic write so refreshes serialize.
            let mut slot = self.subsonic.write().await;
            self.config_gen
                .fetch_add(1, std::sync::atomic::Ordering::Release);
            slot.replace(new_client);
        }

        self.refresh_starred().await;
        self.refresh_artists().await;
        self.refresh_playlists().await;
        self.refresh_music_folders().await;
        self.spawn_refresh_scrobble_capability();

        self.emit_config_changed().await;
        Ok(storage)
    }

    /// Apply `mutate` to a config clone, persist it on a blocking thread, then
    /// commit the same mutation to live state. The atomic fsync-backed write
    /// never runs on an async worker and never holds the `state` lock, so a
    /// slow filesystem cannot stall the playback tick or the UI mirror. On a
    /// persist failure the live config is left untouched.
    async fn persist_config<F>(self: &Arc<Self>, mutate: F) -> Result<(), Error>
    where
        F: Fn(&mut crate::config::Config),
    {
        let mut candidate = { self.state.read().await.config.clone() };
        mutate(&mut candidate);
        let to_write = candidate.clone();
        tokio::task::spawn_blocking(move || to_write.save_default())
            .await
            .map_err(|e| {
                Error::Io(std::io::Error::other(format!(
                    "config save task failed: {e}"
                )))
            })?
            .map_err(Error::Config)?;
        // Re-apply to live state (rather than replacing it) so a concurrent
        // setting change between the clone and here is preserved.
        {
            let mut state = self.state.write().await;
            mutate(&mut state.config);
        }
        Ok(())
    }

    /// Persist the scrobble toggle and broadcast the config change.
    ///
    /// # Errors
    /// Returns an `Error` if persisting the config or a follow-up server request fails.
    pub async fn set_scrobble(self: &Arc<Self>, on: bool) -> Result<(), Error> {
        self.persist_config(move |c| c.scrobble = on).await?;
        self.emit_config_changed().await;
        Ok(())
    }

    /// Persist the desktop-notifications toggle and broadcast the config change.
    ///
    /// # Errors
    /// Returns an `Error` if persisting the config or a follow-up server request fails.
    pub async fn set_notifications(self: &Arc<Self>, on: bool) -> Result<(), Error> {
        self.persist_config(move |c| c.notifications = on).await?;
        self.emit_config_changed().await;
        Ok(())
    }

    /// Probe credentials without persisting; returns (ok, message).
    pub async fn test_server_connection(
        self: &Arc<Self>,
        base_url: &str,
        username: &str,
        password: &crate::secret::Secret,
    ) -> (bool, String) {
        match SubsonicClient::new(base_url, username, password) {
            Ok(client) => match client.ping().await {
                Ok(()) => (true, "Connection OK".to_string()),
                Err(e) => (false, format!("Connection failed: {e}")),
            },
            Err(e) => (false, format!("Invalid URL: {e}")),
        }
    }

    /// Persist the theme choice and broadcast the config change.
    ///
    /// # Errors
    /// Returns an `Error` if persisting the config or a follow-up server request fails.
    pub async fn set_theme(self: &Arc<Self>, name: &str) -> Result<(), Error> {
        self.persist_config(|c| c.theme = name.to_string()).await?;
        self.emit_config_changed().await;
        Ok(())
    }

    /// Persist the cava on/off toggle and broadcast the config change.
    ///
    /// # Errors
    /// Returns an `Error` if persisting the config or a follow-up server request fails.
    pub async fn set_cava_enabled(self: &Arc<Self>, on: bool) -> Result<(), Error> {
        self.persist_config(move |c| c.cava = on).await?;
        self.emit_config_changed().await;
        Ok(())
    }

    /// Takes effect on the next TUI launch.
    ///
    /// # Errors
    /// Returns an `Error` if persisting the config or a follow-up server request fails.
    pub async fn set_daemon_enabled(self: &Arc<Self>, on: bool) -> Result<(), Error> {
        self.persist_config(move |c| c.daemon = on).await?;
        self.emit_config_changed().await;
        Ok(())
    }

    /// Persist the auto-continue toggle and broadcast the config change.
    ///
    /// # Errors
    /// Returns an `Error` if persisting the config or a follow-up server request fails.
    pub async fn set_auto_continue(self: &Arc<Self>, on: bool) -> Result<(), Error> {
        self.persist_config(move |c| c.auto_continue = on).await?;
        self.emit_config_changed().await;
        Ok(())
    }

    /// Re-preloads the new auto-advance target so gapless picks up the mode change at the next track boundary.
    ///
    /// # Errors
    /// Returns an `Error` if persisting the config or a follow-up server request fails.
    pub async fn set_repeat_mode(
        self: &Arc<Self>,
        mode: crate::config::RepeatMode,
    ) -> Result<(), Error> {
        self.persist_config(move |c| c.repeat_mode = mode).await?;
        let cur_pos = { self.state.read().await.queue_position };
        self.emit(DaemonEvent::RepeatModeChanged(mode));
        self.emit_config_changed().await;
        if let Some(pos) = cur_pos {
            let mut mpv = self.mpv.lock().await;
            if let Ok(count) = mpv.get_playlist_count().await {
                if count > 1 {
                    let _ = mpv.playlist_remove(1).await;
                }
            }
            drop(mpv);
            self.preload_next_track(pos).await;
        }
        Ok(())
    }

    /// Persist the cover art toggle and broadcast the config change.
    ///
    /// # Errors
    /// Returns an `Error` if persisting the config or a follow-up server request fails.
    pub async fn set_cover_art_enabled(self: &Arc<Self>, on: bool) -> Result<(), Error> {
        self.persist_config(move |c| c.cover_art = on).await?;
        self.emit_config_changed().await;
        Ok(())
    }

    /// Persist the cover art size, clamped to 8-24 rows.
    ///
    /// # Errors
    /// Returns an `Error` if persisting the config or a follow-up server request fails.
    pub async fn set_cover_art_size(self: &Arc<Self>, size: u8) -> Result<(), Error> {
        let clamped = size.clamp(8, 24);
        self.persist_config(move |c| c.cover_art_size = clamped)
            .await?;
        self.emit_config_changed().await;
        Ok(())
    }

    /// Persist the cava size, clamped to 10-80 rows.
    ///
    /// # Errors
    /// Returns an `Error` if persisting the config or a follow-up server request fails.
    pub async fn set_cava_size(self: &Arc<Self>, size: u8) -> Result<(), Error> {
        let clamped = size.clamp(10, 80);
        self.persist_config(move |c| c.cava_size = clamped).await?;
        self.emit_config_changed().await;
        Ok(())
    }

    /// Persist the `ReplayGain` mode and push it live to mpv via `set_property`,
    /// so an already-playing track re-applies gain immediately.
    ///
    /// # Errors
    /// Returns an `Error` if persisting the config or a follow-up server request fails.
    pub async fn set_replay_gain_mode(
        self: &Arc<Self>,
        mode: crate::config::ReplayGainMode,
    ) -> Result<(), Error> {
        self.persist_config(move |c| c.replay_gain_mode = mode)
            .await?;
        // Best-effort like set_volume: mpv is always running (--idle) once
        // start_mpv has succeeded, but a not-yet-started mpv should not
        // block persisting the setting.
        let mut mpv = self.mpv.lock().await;
        let _ = mpv.set_replaygain_mode(mode).await;
        drop(mpv);
        self.emit_config_changed().await;
        Ok(())
    }

    /// Persist the `ReplayGain` preamp (dB, clamped to -15.0..=15.0) and push it
    /// live to mpv via `set_property`.
    ///
    /// # Errors
    /// Returns an `Error` if persisting the config or a follow-up server request fails.
    pub async fn set_replay_gain_preamp(self: &Arc<Self>, preamp: f64) -> Result<(), Error> {
        crate::config::validate_replay_gain_preamp(preamp)?;
        let clamped = preamp.clamp(
            crate::config::REPLAY_GAIN_PREAMP_MIN,
            crate::config::REPLAY_GAIN_PREAMP_MAX,
        );
        self.persist_config(move |c| c.replay_gain_preamp = clamped)
            .await?;
        let mut mpv = self.mpv.lock().await;
        let _ = mpv.set_replaygain_preamp(clamped).await;
        drop(mpv);
        self.emit_config_changed().await;
        Ok(())
    }

    /// Persist the `ReplayGain` clip-prevention toggle and push it live to mpv
    /// via `set_property`.
    ///
    /// # Errors
    /// Returns an `Error` if persisting the config or a follow-up server request fails.
    pub async fn set_replay_gain_clip(self: &Arc<Self>, on: bool) -> Result<(), Error> {
        self.persist_config(move |c| c.replay_gain_clip = on)
            .await?;
        let mut mpv = self.mpv.lock().await;
        let _ = mpv.set_replaygain_clip(on).await;
        drop(mpv);
        self.emit_config_changed().await;
        Ok(())
    }

    /// Persist the playback-filter exclusion rules. Takes effect the next
    /// time songs are added to the queue; does not retroactively filter an
    /// already-persisted queue.
    ///
    /// # Errors
    /// Returns an `Error` if persisting the config or a follow-up server request fails.
    pub async fn set_playback_filters(
        self: &Arc<Self>,
        filters: crate::config::PlaybackFilters,
    ) -> Result<(), Error> {
        self.persist_config(move |c| c.playback_filters.clone_from(&filters))
            .await?;
        self.emit_config_changed().await;
        Ok(())
    }

    /// Persist global keybinding overrides. The TUI applies the resolved map
    /// after this request succeeds; the daemon only owns durable config.
    ///
    /// # Errors
    /// Returns an `Error` when the config cannot be persisted.
    pub async fn set_keybindings(
        self: &Arc<Self>,
        bindings: std::collections::HashMap<
            crate::config::keybind::GlobalAction,
            crate::config::keybind::KeyChord,
        >,
    ) -> Result<(), Error> {
        self.persist_config(move |c| c.keybindings.clone_from(&bindings))
            .await?;
        self.emit_config_changed().await;
        Ok(())
    }
}
