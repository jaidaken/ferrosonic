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

#[test]
fn crossterm_constructor_returns_runnable_guard() {
    let _g = TerminalGuard::new_crossterm();
}
