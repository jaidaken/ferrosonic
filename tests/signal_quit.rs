//! Signal-quit handler effect (pure function extracted from spawn_signal_quit).

use ferrosonic::app::handle_signal_received;
use ferrosonic::app::state::new_shared_client_state;
use ferrosonic::config::Config;

#[tokio::test]
async fn handle_signal_received_flips_should_quit() {
    let client_state = new_shared_client_state(&Config::new());
    assert!(!client_state.read().await.should_quit);
    handle_signal_received(client_state.clone()).await;
    assert!(
        client_state.read().await.should_quit,
        "signal-handler effect must set should_quit=true"
    );
}
