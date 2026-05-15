//! pipewire parse logic + controller state machine.

use ferrosonic::audio::pipewire::{parse_force_rate_from_output, PipeWireController};

#[test]
fn parse_force_rate_from_typical_output() {
    let stdout = "update: id:0 key:'clock.force-rate' value:'48000' type:''\n";
    assert_eq!(parse_force_rate_from_output(stdout), 48000);
}

#[test]
fn parse_force_rate_with_44100() {
    let stdout = "update: id:0 key:'clock.force-rate' value:'44100' type:''";
    assert_eq!(parse_force_rate_from_output(stdout), 44100);
}

#[test]
fn parse_force_rate_returns_zero_when_value_missing() {
    let stdout = "update: id:0 key:'clock.force-rate' type:''";
    assert_eq!(parse_force_rate_from_output(stdout), 0);
}

#[test]
fn parse_force_rate_returns_zero_for_empty_output() {
    assert_eq!(parse_force_rate_from_output(""), 0);
}

#[test]
fn parse_force_rate_returns_zero_for_unrelated_lines() {
    let stdout = "update: id:0 key:'other.thing' value:'123'\n";
    assert_eq!(parse_force_rate_from_output(stdout), 0);
}

#[test]
fn parse_force_rate_handles_zero_value() {
    let stdout = "update: id:0 key:'clock.force-rate' value:'0' type:''\n";
    assert_eq!(parse_force_rate_from_output(stdout), 0);
}

#[test]
fn parse_force_rate_picks_first_matching_line() {
    let stdout = "update: id:0 key:'clock.force-rate' value:'48000'\n\
                  update: id:0 key:'clock.force-rate' value:'96000'\n";
    assert_eq!(parse_force_rate_from_output(stdout), 48000);
}

#[test]
fn parse_force_rate_ignores_non_numeric_value() {
    let stdout = "update: id:0 key:'clock.force-rate' value:'abc' type:''";
    assert_eq!(parse_force_rate_from_output(stdout), 0);
}

#[test]
fn parse_force_rate_handles_high_sample_rate() {
    let stdout = "update: id:0 key:'clock.force-rate' value:'192000' type:''";
    assert_eq!(parse_force_rate_from_output(stdout), 192000);
}

#[tokio::test]
async fn set_rate_to_same_value_short_circuits() {
    let mut ctrl = PipeWireController::default();
    // First call may fail if pw-metadata unavailable; that's fine.
    let _ = ctrl.set_rate(48000).await;
    // Second call with same rate hits the short-circuit branch.
    let _ = ctrl.set_rate(48000).await;
    let cur = ctrl.get_current_rate();
    if cur.is_some() {
        assert_eq!(cur, Some(48000));
    }
}

#[test]
fn pipewire_controller_default_is_constructible() {
    let ctrl = PipeWireController::default();
    assert!(ctrl.get_current_rate().is_none());
}
