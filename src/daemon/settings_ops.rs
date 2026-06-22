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
    /// an inline owner-only config write when no keychain is reachable. Returns
    /// where the password landed so the caller can inform the user.
    pub async fn update_server_config(
        self: &Arc<Self>,
        base_url: &str,
        username: &str,
        password: &crate::secret::Secret,
    ) -> Result<PasswordStorage, Error> {
        let mut state = self.state.write().await;
        let old_url = std::mem::replace(&mut state.config.base_url, base_url.to_string());
        let old_user = std::mem::replace(&mut state.config.username, username.to_string());
        let had_keyring = state.config.password_keyring;
        let music_folder_id = state.config.music_folder_id;
        let pf_opt = state.config.password_file.clone().filter(|s| !s.is_empty());
        let storage = if state.config.password_eval.is_some() {
            // The command owns the secret; never persist the typed password.
            state.config.password_keyring = false;
            state.config.password = crate::secret::Secret::new();
            state.config.save_default().map_err(Error::Config)?;
            state.config.password = password.clone();
            PasswordStorage::PasswordEval
        } else if let Some(pf) = pf_opt.as_deref() {
            if let Err(e) = crate::config::write_password_file_atomic(pf, password) {
                error!("Failed to write password to {}: {}", pf, e);
                return Err(Error::Io(e));
            }
            state.config.password_keyring = false;
            state.config.password = crate::secret::Secret::new();
            state.config.save_default().map_err(Error::Config)?;
            state.config.password = password.clone();
            PasswordStorage::PasswordFile
        } else {
            match crate::secret_store::store(base_url, username, password) {
                Ok(()) => {
                    state.config.password_keyring = true;
                    state.config.password = crate::secret::Secret::new();
                    state.config.save_default().map_err(Error::Config)?;
                    state.config.password = password.clone();
                    PasswordStorage::Keyring
                }
                Err(e) => {
                    warn!("OS keychain unavailable ({e}); writing the password inline to the owner-only config file");
                    state.config.password_keyring = false;
                    state.config.password = password.clone();
                    state.config.save_default().map_err(Error::Config)?;
                    PasswordStorage::Inline
                }
            }
        };
        drop(state);

        // Outside the state lock so keychain IO never blocks readers: drop an
        // orphaned entry when the credential left the keychain or its key changed.
        let key_changed = old_url != base_url || old_user != username;
        if had_keyring && (storage != PasswordStorage::Keyring || key_changed) {
            if let Err(e) = crate::secret_store::delete(&old_url, &old_user) {
                warn!("Could not remove the previous keychain entry: {e}");
            }
        }

        let mut new_client =
            SubsonicClient::new(base_url, username, password).map_err(Error::Subsonic)?;
        new_client.set_music_folder(music_folder_id);
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

    /// Persist the scrobble toggle and broadcast the config change.
    pub async fn set_scrobble(self: &Arc<Self>, on: bool) -> Result<(), Error> {
        {
            let mut state = self.state.write().await;
            state.config.scrobble = on;
            state.config.save_default().map_err(Error::Config)?;
        }
        self.emit_config_changed().await;
        Ok(())
    }

    /// Persist the desktop-notifications toggle and broadcast the config change.
    pub async fn set_notifications(self: &Arc<Self>, on: bool) -> Result<(), Error> {
        {
            let mut state = self.state.write().await;
            state.config.notifications = on;
            state.config.save_default().map_err(Error::Config)?;
        }
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
    pub async fn set_theme(self: &Arc<Self>, name: &str) -> Result<(), Error> {
        {
            let mut state = self.state.write().await;
            state.config.theme = name.to_string();
            state.config.save_default().map_err(Error::Config)?;
        }
        self.emit_config_changed().await;
        Ok(())
    }

    /// Persist the cava on/off toggle and broadcast the config change.
    pub async fn set_cava_enabled(self: &Arc<Self>, on: bool) -> Result<(), Error> {
        {
            let mut state = self.state.write().await;
            state.config.cava = on;
            state.config.save_default().map_err(Error::Config)?;
        }
        self.emit_config_changed().await;
        Ok(())
    }

    /// Takes effect on the next TUI launch.
    pub async fn set_daemon_enabled(self: &Arc<Self>, on: bool) -> Result<(), Error> {
        {
            let mut state = self.state.write().await;
            state.config.daemon = on;
            state.config.save_default().map_err(Error::Config)?;
        }
        self.emit_config_changed().await;
        Ok(())
    }

    /// Persist the auto-continue toggle and broadcast the config change.
    pub async fn set_auto_continue(self: &Arc<Self>, on: bool) -> Result<(), Error> {
        {
            let mut state = self.state.write().await;
            state.config.auto_continue = on;
            state.config.save_default().map_err(Error::Config)?;
        }
        self.emit_config_changed().await;
        Ok(())
    }

    /// Re-preloads the new auto-advance target so gapless picks up the mode change at the next track boundary.
    pub async fn set_repeat_mode(
        self: &Arc<Self>,
        mode: crate::config::RepeatMode,
    ) -> Result<(), Error> {
        let cur_pos = {
            let mut state = self.state.write().await;
            state.config.repeat_mode = mode;
            state.config.save_default().map_err(Error::Config)?;
            state.queue_position
        };
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
    pub async fn set_cover_art_enabled(self: &Arc<Self>, on: bool) -> Result<(), Error> {
        {
            let mut state = self.state.write().await;
            state.config.cover_art = on;
            state.config.save_default().map_err(Error::Config)?;
        }
        self.emit_config_changed().await;
        Ok(())
    }

    /// Persist the cover art size, clamped to 8-24 rows.
    pub async fn set_cover_art_size(self: &Arc<Self>, size: u8) -> Result<(), Error> {
        let clamped = size.clamp(8, 24);
        {
            let mut state = self.state.write().await;
            state.config.cover_art_size = clamped;
            state.config.save_default().map_err(Error::Config)?;
        }
        self.emit_config_changed().await;
        Ok(())
    }

    /// Persist the cava size, clamped to 10-80 rows.
    pub async fn set_cava_size(self: &Arc<Self>, size: u8) -> Result<(), Error> {
        let clamped = size.clamp(10, 80);
        {
            let mut state = self.state.write().await;
            state.config.cava_size = clamped;
            state.config.save_default().map_err(Error::Config)?;
        }
        self.emit_config_changed().await;
        Ok(())
    }
}
