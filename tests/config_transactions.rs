//! Configuration persistence is one serialized disk/live/client transaction.

mod common;

use std::sync::Arc;

use common::TestDaemon;
use ferrosonic::config::{Config, PasswordEval};
use ferrosonic::secret::Secret;
use ferrosonic::secret_store;
use serial_test::serial;
use tokio::sync::Barrier;

fn load(td: &TestDaemon) -> Config {
    Config::load_from_file(&td.config_dir.path().join("config.toml")).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn concurrent_scalar_setters_commit_both_live_and_on_disk() {
    let td = TestDaemon::new().await;
    let barrier = Arc::new(Barrier::new(3));
    let theme = {
        let core = td.core.clone();
        let barrier = barrier.clone();
        tokio::spawn(async move {
            barrier.wait().await;
            core.set_theme("transaction-theme").await
        })
    };
    let scrobble = {
        let core = td.core.clone();
        let barrier = barrier.clone();
        tokio::spawn(async move {
            barrier.wait().await;
            core.set_scrobble(false).await
        })
    };
    barrier.wait().await;
    theme.await.unwrap().unwrap();
    scrobble.await.unwrap().unwrap();

    let live = td.state.read().await.config.clone();
    let disk = load(&td);
    assert_eq!(live.theme, "transaction-theme");
    assert!(!live.scrobble);
    assert_eq!(disk.theme, live.theme);
    assert_eq!(disk.scrobble, live.scrobble);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn scalar_setter_racing_server_update_survives_everywhere() {
    let td = TestDaemon::new().await;
    let url = td.fake_subsonic.url();
    let barrier = Arc::new(Barrier::new(3));
    let server = {
        let core = td.core.clone();
        let barrier = barrier.clone();
        tokio::spawn(async move {
            barrier.wait().await;
            core.update_server_config(&url, "new-user", &Secret::from("new-password"))
                .await
        })
    };
    let scalar = {
        let core = td.core.clone();
        let barrier = barrier.clone();
        tokio::spawn(async move {
            barrier.wait().await;
            core.set_theme("survivor").await
        })
    };
    barrier.wait().await;
    server.await.unwrap().unwrap();
    scalar.await.unwrap().unwrap();

    let live = td.state.read().await.config.clone();
    let disk = load(&td);
    assert_eq!(live.username, "new-user");
    assert_eq!(live.theme, "survivor");
    assert_eq!(disk.username, live.username);
    assert_eq!(disk.theme, live.theme);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn music_folder_change_is_in_the_same_config_transaction_domain() {
    let td = TestDaemon::new().await;
    let barrier = Arc::new(Barrier::new(3));
    let folder = {
        let core = td.core.clone();
        let barrier = barrier.clone();
        tokio::spawn(async move {
            barrier.wait().await;
            core.set_music_folder(Some(42)).await
        })
    };
    let scalar = {
        let core = td.core.clone();
        let barrier = barrier.clone();
        tokio::spawn(async move {
            barrier.wait().await;
            core.set_theme("folder-race").await
        })
    };
    barrier.wait().await;
    folder.await.unwrap().unwrap();
    scalar.await.unwrap().unwrap();
    let live = td.state.read().await.config.clone();
    let disk = load(&td);
    assert_eq!(live.music_folder_id, Some(42));
    assert_eq!(live.theme, "folder-race");
    assert_eq!(disk.music_folder_id, live.music_folder_id);
    assert_eq!(disk.theme, live.theme);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn racing_server_updates_leave_one_coherent_keyring_winner() {
    let td = TestDaemon::new().await;
    let url = td.fake_subsonic.url();
    let barrier = Arc::new(Barrier::new(3));
    let mut tasks = Vec::new();
    for (user, password) in [("alice", "alice-pass"), ("bob", "bob-pass")] {
        let core = td.core.clone();
        let barrier = barrier.clone();
        let url = url.clone();
        tasks.push(tokio::spawn(async move {
            barrier.wait().await;
            core.update_server_config(&url, user, &Secret::from(password))
                .await
        }));
    }
    barrier.wait().await;
    for task in tasks {
        task.await.unwrap().unwrap();
    }

    let live = td.state.read().await.config.clone();
    let disk = load(&td);
    assert_eq!(disk.username, live.username);
    assert!(live.password_keyring && disk.password_keyring);
    assert_eq!(disk.password.reveal(), live.password.reveal());
    let winner = secret_store::retrieve(&url, &live.username)
        .unwrap()
        .unwrap();
    assert_eq!(winner.reveal(), live.password.reveal());
    let loser = if live.username == "alice" {
        "bob"
    } else {
        "alice"
    };
    assert!(secret_store::retrieve(&url, loser).unwrap().is_none());
}

#[tokio::test]
#[serial]
async fn malformed_url_has_no_disk_live_client_or_keychain_side_effect() {
    let td = TestDaemon::new().await;
    let url = td.fake_subsonic.url();
    td.core
        .update_server_config(&url, "working", &Secret::from("working-password"))
        .await
        .unwrap();
    let before_live = td.state.read().await.config.clone();
    let before_disk = std::fs::read(td.config_dir.path().join("config.toml")).unwrap();
    let before_gen = td.core.config_gen_for_test();

    let result = td
        .core
        .update_server_config("not a url", "broken", &Secret::from("orphan"))
        .await;
    assert!(result.is_err());
    let after_live = td.state.read().await.config.clone();
    assert_eq!(after_live.base_url, before_live.base_url);
    assert_eq!(after_live.username, before_live.username);
    assert_eq!(after_live.password.reveal(), before_live.password.reveal());
    assert_eq!(td.core.config_gen_for_test(), before_gen);
    assert_eq!(
        std::fs::read(td.config_dir.path().join("config.toml")).unwrap(),
        before_disk
    );
    assert_eq!(
        secret_store::retrieve(&url, "working")
            .unwrap()
            .as_ref()
            .map(Secret::reveal),
        Some("working-password")
    );
    assert!(secret_store::retrieve("not a url", "broken")
        .unwrap()
        .is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn serialized_scalar_saves_preserve_external_credential_sources() {
    let td = TestDaemon::new().await;
    let password_file = td.config_dir.path().join("password");
    let cases = [
        (None, None, true, false),
        (
            Some(password_file.to_string_lossy().into_owned()),
            None,
            false,
            false,
        ),
        (
            None,
            Some(PasswordEval::Shell("printf eval-secret".into())),
            false,
            false,
        ),
        (None, None, false, true),
    ];
    for (password_file, password_eval, from_env, keyring) in cases {
        {
            let mut state = td.state.write().await;
            state.config.password_file = password_file.clone();
            state.config.password_eval = password_eval.clone();
            state.config.password_from_env = from_env;
            state.config.password_keyring = keyring;
            state.config.password = Secret::from("must-not-leak");
        }
        let barrier = Arc::new(Barrier::new(3));
        let a = {
            let core = td.core.clone();
            let barrier = barrier.clone();
            tokio::spawn(async move {
                barrier.wait().await;
                core.set_cava_enabled(true).await
            })
        };
        let b = {
            let core = td.core.clone();
            let barrier = barrier.clone();
            tokio::spawn(async move {
                barrier.wait().await;
                core.set_notifications(false).await
            })
        };
        barrier.wait().await;
        a.await.unwrap().unwrap();
        b.await.unwrap().unwrap();
        let written = std::fs::read_to_string(td.config_dir.path().join("config.toml")).unwrap();
        assert!(!written.contains("must-not-leak"));
        assert_eq!(written.contains("PasswordFile"), password_file.is_some());
        assert_eq!(written.contains("PasswordEval"), password_eval.is_some());
        assert_eq!(written.contains("PasswordKeyring = true"), keyring);
    }
}
