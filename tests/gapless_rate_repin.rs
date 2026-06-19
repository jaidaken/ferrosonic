//! Regression: a gapless advance must clear the audio props so the tick
//! re-probes and re-pins the new track's sample rate (bit-perfect across rates).

mod common;

use common::TestDaemon;
use serial_test::serial;

fn song(id: &str) -> ferrosonic::subsonic::models::Child {
    common::song(id, id)
}

#[tokio::test]
#[serial]
async fn gapless_advance_clears_sample_rate_so_it_re_pins() {
    let td = TestDaemon::new().await;
    {
        let mut s = td.state.write().await;
        s.queue = vec![song("t0"), song("t1")];
        s.queue_position = Some(0);
        s.now_playing.song = Some(song("t0"));
        s.now_playing.sample_rate = Some(44_100);
        s.now_playing.bit_depth = Some(16);
    }

    td.core.try_gapless_advance_for_test().await;

    let s = td.state.read().await;
    assert_eq!(s.queue_position, Some(1), "advanced to the next track");
    assert!(
        s.now_playing.sample_rate.is_none(),
        "rate cleared so the next tick re-probes and re-pins the new track's rate"
    );
    assert!(
        s.now_playing.bit_depth.is_none(),
        "bit depth cleared for re-probe too"
    );
}
