//! Pure-logic tests for the `RepeatMode` state machine.

use ferrosonic::config::RepeatMode;

#[test]
fn cycle_visits_all_three_modes() {
    assert_eq!(RepeatMode::Off.cycle(), RepeatMode::One);
    assert_eq!(RepeatMode::One.cycle(), RepeatMode::All);
    assert_eq!(RepeatMode::All.cycle(), RepeatMode::Off);
}

#[test]
fn labels_are_lowercase_words() {
    assert_eq!(RepeatMode::Off.label(), "off");
    assert_eq!(RepeatMode::One.label(), "one");
    assert_eq!(RepeatMode::All.label(), "all");
}

#[test]
fn next_manual_off_advances_then_stops_at_end() {
    let mode = RepeatMode::Off;
    assert_eq!(mode.next_manual(0, 3), Some(1));
    assert_eq!(mode.next_manual(1, 3), Some(2));
    assert_eq!(
        mode.next_manual(2, 3),
        None,
        "Off does not wrap on manual Next"
    );
}

#[test]
fn next_manual_all_wraps_at_end() {
    let mode = RepeatMode::All;
    assert_eq!(mode.next_manual(0, 3), Some(1));
    assert_eq!(mode.next_manual(2, 3), Some(0), "All wraps at end");
}

#[test]
fn next_manual_one_still_advances_on_manual_skip() {
    let mode = RepeatMode::One;
    assert_eq!(
        mode.next_manual(0, 3),
        Some(1),
        "manual Next under repeat-One should still move forward"
    );
    assert_eq!(
        mode.next_manual(2, 3),
        Some(0),
        "repeat-One wraps on manual Next"
    );
}

#[test]
fn next_auto_off_advances_then_stops_at_end() {
    let mode = RepeatMode::Off;
    assert_eq!(mode.next_auto(0, 3), Some(1));
    assert_eq!(mode.next_auto(1, 3), Some(2));
    assert_eq!(
        mode.next_auto(2, 3),
        None,
        "Off returns None at end so the caller can trigger auto-continue or stop"
    );
}

#[test]
fn next_auto_all_wraps_at_end() {
    let mode = RepeatMode::All;
    assert_eq!(mode.next_auto(2, 3), Some(0), "All wraps on auto-advance");
}

#[test]
fn next_auto_one_repeats_current_track() {
    let mode = RepeatMode::One;
    assert_eq!(
        mode.next_auto(0, 3),
        Some(0),
        "repeat-One repeats the same index on auto-advance"
    );
    assert_eq!(mode.next_auto(2, 3), Some(2));
}

#[test]
fn next_handlers_return_none_on_empty_queue() {
    for mode in [RepeatMode::Off, RepeatMode::One, RepeatMode::All] {
        assert_eq!(
            mode.next_manual(0, 0),
            None,
            "{:?} manual on empty queue",
            mode
        );
        assert_eq!(mode.next_auto(0, 0), None, "{:?} auto on empty queue", mode);
    }
}

#[test]
fn prev_wrap_off_returns_none_at_start() {
    assert_eq!(
        RepeatMode::Off.prev_wrap(3),
        None,
        "Off does not wrap on Previous from position 0"
    );
}

#[test]
fn prev_wrap_all_and_one_wrap_to_last_track() {
    assert_eq!(RepeatMode::All.prev_wrap(3), Some(2));
    assert_eq!(RepeatMode::One.prev_wrap(3), Some(2));
}

#[test]
fn prev_wrap_empty_queue_returns_none() {
    for mode in [RepeatMode::Off, RepeatMode::One, RepeatMode::All] {
        assert_eq!(mode.prev_wrap(0), None);
    }
}
