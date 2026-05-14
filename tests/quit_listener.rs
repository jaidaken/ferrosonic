//! spawn_quit_listener: future-driven quit handling.

use std::time::Duration;

use ferrosonic::app::spawn_quit_listener;
use ferrosonic::app::state::new_shared_client_state;
use ferrosonic::config::Config;

#[tokio::test]
async fn listener_sets_should_quit_when_signal_future_resolves() {
    let cs = new_shared_client_state(&Config::new());
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    spawn_quit_listener(cs.clone(), async move {
        let _ = rx.await;
    });
    assert!(!cs.read().await.should_quit);
    tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if cs.read().await.should_quit {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("listener did not set should_quit after signal");
    assert!(cs.read().await.should_quit);
}

#[tokio::test]
async fn listener_with_immediate_future_quits_promptly() {
    let cs = new_shared_client_state(&Config::new());
    spawn_quit_listener(cs.clone(), async {});
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if cs.read().await.should_quit {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("listener with immediate future did not set should_quit");
    assert!(cs.read().await.should_quit);
}

#[tokio::test]
async fn listener_with_pending_future_does_not_quit() {
    let cs = new_shared_client_state(&Config::new());
    spawn_quit_listener(cs.clone(), std::future::pending::<()>());
    for _ in 0..100 {
        tokio::task::yield_now().await;
    }
    assert!(!cs.read().await.should_quit);
}

#[tokio::test]
async fn multiple_listeners_set_should_quit_independently() {
    let cs = new_shared_client_state(&Config::new());
    spawn_quit_listener(cs.clone(), async {});
    spawn_quit_listener(cs.clone(), async {});
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if cs.read().await.should_quit {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("multiple listeners did not set should_quit");
    assert!(cs.read().await.should_quit);
}

#[tokio::test]
async fn wait_for_unix_quit_signal_can_be_dropped_without_panic() {
    use ferrosonic::app::wait_for_unix_quit_signal;
    let fut = wait_for_unix_quit_signal();
    drop(fut);
}
