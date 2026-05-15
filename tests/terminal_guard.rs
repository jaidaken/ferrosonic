//! TerminalGuard cleanup closure fires on Drop.

use ferrosonic::app::TerminalGuard;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[test]
fn cleanup_closure_runs_when_guard_dropped() {
    let flag = Arc::new(AtomicBool::new(false));
    let flag_clone = flag.clone();
    {
        let _g = TerminalGuard::with_cleanup(move || {
            flag_clone.store(true, Ordering::SeqCst);
        });
    }
    assert!(flag.load(Ordering::SeqCst));
}

#[test]
fn cleanup_runs_exactly_once() {
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count_clone = count.clone();
    {
        let _g = TerminalGuard::with_cleanup(move || {
            count_clone.fetch_add(1, Ordering::SeqCst);
        });
    }
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

/// Smoke: TerminalGuard::new_crossterm constructor does not panic; Drop is suppressed via mem::forget to avoid mutating the test runner's terminal state.
#[test]
fn crossterm_constructor_does_not_panic() {
    let guard = TerminalGuard::new_crossterm();
    std::mem::forget(guard);
}
