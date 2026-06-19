//! Random Config + RepeatMode values round-trip through TOML and JSON.

use ferrosonic::config::{Config, RepeatMode};
use proptest::prelude::*;

fn arb_repeat_mode() -> impl Strategy<Value = RepeatMode> {
    prop_oneof![
        Just(RepeatMode::Off),
        Just(RepeatMode::One),
        Just(RepeatMode::All),
    ]
}

fn arb_config() -> impl Strategy<Value = Config> {
    (
        ".*",
        ".*",
        ".*",
        ".*",
        any::<bool>(),
        any::<u8>(),
        any::<bool>(),
        any::<bool>(),
        arb_repeat_mode(),
        any::<bool>(),
        any::<u8>(),
        (any::<bool>(), any::<u32>()),
    )
        .prop_map(
            |(
                base_url,
                username,
                password,
                theme,
                cava,
                cava_size,
                daemon,
                auto_continue,
                repeat_mode,
                cover_art,
                cover_art_size,
                (scrobble, rate_switch_delay_ms),
            )| Config {
                base_url,
                username,
                password: password.into(),
                password_file: None,
                theme,
                cava,
                cava_size,
                daemon,
                auto_continue,
                repeat_mode,
                cover_art,
                cover_art_size,
                scrobble,
                rate_switch_delay_ms,
            },
        )
}

#[test]
fn config_round_trips_through_toml() {
    let mut runner = proptest::test_runner::TestRunner::default();
    runner
        .run(&arb_config(), |c| {
            let mut tmp = tempfile::NamedTempFile::new().unwrap();
            let toml = toml::to_string(&c).unwrap();
            use std::io::Write;
            tmp.write_all(toml.as_bytes()).unwrap();
            tmp.flush().unwrap();
            let parsed = Config::load_from_file(tmp.path()).unwrap();
            prop_assert_eq!(parsed.cava_size, c.cava_size);
            prop_assert_eq!(parsed.cover_art_size, c.cover_art_size);
            prop_assert_eq!(parsed.repeat_mode, c.repeat_mode);
            prop_assert_eq!(parsed.cava, c.cava);
            prop_assert_eq!(parsed.daemon, c.daemon);
            prop_assert_eq!(parsed.cover_art, c.cover_art);
            prop_assert_eq!(parsed.auto_continue, c.auto_continue);
            prop_assert_eq!(parsed.scrobble, c.scrobble);
            prop_assert_eq!(parsed.rate_switch_delay_ms, c.rate_switch_delay_ms);
            Ok(())
        })
        .unwrap();
}

#[test]
fn default_rate_switch_delay_is_a_non_trivial_settle() {
    // The default must give the audio device real time to re-clock in
    // silence; a near-zero value would defeat the gap-hiding pre-roll.
    assert!(
        Config::default().rate_switch_delay_ms >= 100,
        "default rate-switch settle must be a meaningful silence window"
    );
}

#[test]
fn repeat_mode_round_trips_through_json() {
    let mut runner = proptest::test_runner::TestRunner::default();
    runner
        .run(&arb_repeat_mode(), |m| {
            let s = serde_json::to_string(&m).unwrap();
            let back: RepeatMode = serde_json::from_str(&s).unwrap();
            prop_assert_eq!(back, m);
            Ok(())
        })
        .unwrap();
}

#[test]
fn repeat_mode_cycle_is_a_three_cycle() {
    let mut runner = proptest::test_runner::TestRunner::default();
    runner
        .run(&arb_repeat_mode(), |m| {
            prop_assert_eq!(m.cycle().cycle().cycle(), m);
            Ok(())
        })
        .unwrap();
}

#[test]
fn next_auto_always_in_bounds_when_queue_non_empty() {
    let mut runner = proptest::test_runner::TestRunner::default();
    runner
        .run(
            &(arb_repeat_mode(), 0usize..1000, 1usize..1000),
            |(mode, cur, queue_len)| {
                let cur = cur.min(queue_len.saturating_sub(1));
                if let Some(next) = mode.next_auto(cur, queue_len) {
                    prop_assert!(next < queue_len);
                }
                Ok(())
            },
        )
        .unwrap();
}

#[test]
fn next_manual_always_in_bounds_when_queue_non_empty() {
    let mut runner = proptest::test_runner::TestRunner::default();
    runner
        .run(
            &(arb_repeat_mode(), 0usize..1000, 1usize..1000),
            |(mode, cur, queue_len)| {
                let cur = cur.min(queue_len.saturating_sub(1));
                if let Some(next) = mode.next_manual(cur, queue_len) {
                    prop_assert!(next < queue_len);
                }
                Ok(())
            },
        )
        .unwrap();
}

#[test]
fn next_handlers_return_none_on_empty_queue_always() {
    let mut runner = proptest::test_runner::TestRunner::default();
    runner
        .run(&(arb_repeat_mode(), 0usize..1000), |(mode, cur)| {
            prop_assert_eq!(mode.next_auto(cur, 0), None);
            prop_assert_eq!(mode.next_manual(cur, 0), None);
            prop_assert_eq!(mode.prev_wrap(0), None);
            Ok(())
        })
        .unwrap();
}

#[test]
fn prev_wrap_matches_documented_contract() {
    let mut runner = proptest::test_runner::TestRunner::default();
    runner
        .run(&(arb_repeat_mode(), 1usize..1000), |(mode, queue_len)| {
            let got = mode.prev_wrap(queue_len);
            match mode {
                RepeatMode::Off => prop_assert_eq!(got, None),
                RepeatMode::One | RepeatMode::All => {
                    prop_assert_eq!(got, Some(queue_len - 1));
                    let idx = got.expect("wrap mode must return Some on non-empty queue");
                    prop_assert!(
                        idx < queue_len,
                        "prev_wrap index {} out of bounds for len {}",
                        idx,
                        queue_len
                    );
                }
            }
            Ok(())
        })
        .expect("prev_wrap contract: Off -> None, One/All -> last index");
}
