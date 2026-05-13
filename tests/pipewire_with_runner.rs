//! pipewire CommandRunner injection: set_rate / clear_forced_rate /
//! Drop restore exercised against a fake pw-metadata.

use std::os::unix::process::ExitStatusExt;
use std::process::Output;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use ferrosonic::audio::pipewire::{CommandRunner, PipeWireController};
use ferrosonic::error::AudioError;

#[derive(Default)]
struct FakeRunnerState {
    calls: Vec<Vec<String>>,
    initial_query_stdout: String,
    fail_next: Option<String>,
    fail_status: bool,
}

#[derive(Clone)]
struct FakeRunner {
    inner: Arc<Mutex<FakeRunnerState>>,
}

impl FakeRunner {
    fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(FakeRunnerState::default())),
        }
    }

    fn set_initial_query(&self, stdout: &str) {
        self.inner.lock().unwrap().initial_query_stdout = stdout.to_string();
    }

    fn fail_next_with(&self, msg: &str) {
        self.inner.lock().unwrap().fail_next = Some(msg.to_string());
    }

    fn fail_with_nonzero_status(&self) {
        self.inner.lock().unwrap().fail_status = true;
    }

    fn calls(&self) -> Vec<Vec<String>> {
        self.inner.lock().unwrap().calls.clone()
    }
}

fn capture(state: &mut FakeRunnerState, args: &[&str]) -> Result<Output, AudioError> {
    state
        .calls
        .push(args.iter().map(|s| s.to_string()).collect());
    if let Some(msg) = state.fail_next.take() {
        return Err(AudioError::PipeWire(msg));
    }
    let is_query = args.len() == 4
        && args[0] == "-n"
        && args[1] == "settings"
        && args[2] == "0"
        && args[3] == "clock.force-rate";
    let stdout = if is_query {
        state.initial_query_stdout.clone().into_bytes()
    } else {
        Vec::new()
    };
    let (code, stderr) = if state.fail_status {
        (1, b"bad".to_vec())
    } else {
        (0, Vec::new())
    };
    Ok(Output {
        status: std::process::ExitStatus::from_raw(code << 8),
        stdout,
        stderr,
    })
}

#[async_trait]
impl CommandRunner for FakeRunner {
    async fn run(&self, args: &[&str]) -> Result<Output, AudioError> {
        capture(&mut self.inner.lock().unwrap(), args)
    }

    fn run_blocking(&self, args: &[&str]) -> Result<Output, AudioError> {
        capture(&mut self.inner.lock().unwrap(), args)
    }
}

#[tokio::test]
async fn new_calls_pw_metadata_to_read_original_rate() {
    let runner = FakeRunner::new();
    runner.set_initial_query("update: id:0 key:'clock.force-rate' value:'48000' type:''\n");
    let ctrl = PipeWireController::with_runner(Arc::new(runner.clone()));
    assert_eq!(ctrl.get_original_rate(), Some(48000));
}

#[tokio::test]
async fn new_with_unknown_rate_records_zero() {
    let runner = FakeRunner::new();
    runner.set_initial_query("");
    let ctrl = PipeWireController::with_runner(Arc::new(runner));
    assert_eq!(ctrl.get_original_rate(), Some(0));
}

#[tokio::test]
async fn set_rate_invokes_runner_with_expected_args() {
    let runner = FakeRunner::new();
    runner.set_initial_query("");
    let mut ctrl = PipeWireController::with_runner(Arc::new(runner.clone()));
    ctrl.set_rate(96000).await.expect("set_rate ok");
    assert_eq!(ctrl.get_current_rate(), Some(96000));
    let calls = runner.calls();
    assert!(calls
        .iter()
        .any(|c| c.contains(&"96000".to_string()) && c.contains(&"clock.force-rate".to_string())));
}

#[tokio::test]
async fn set_rate_with_same_value_still_issues_pw_metadata() {
    // External pw-metadata changes would make a cache short-circuit
    // silently break bit-perfect, so set_rate always issues.
    let runner = FakeRunner::new();
    runner.set_initial_query("");
    let mut ctrl = PipeWireController::with_runner(Arc::new(runner.clone()));
    ctrl.set_rate(44100).await.unwrap();
    let calls_before = runner.calls().len();
    ctrl.set_rate(44100).await.unwrap();
    let calls_after = runner.calls().len();
    assert!(
        calls_after > calls_before,
        "second set_rate must still run pw-metadata"
    );
}

#[tokio::test]
async fn set_rate_propagates_runner_error() {
    let runner = FakeRunner::new();
    runner.set_initial_query("");
    let mut ctrl = PipeWireController::with_runner(Arc::new(runner.clone()));
    runner.fail_next_with("boom");
    let r = ctrl.set_rate(48000).await;
    assert!(r.is_err());
    assert_eq!(ctrl.get_current_rate(), None);
}

#[tokio::test]
async fn set_rate_propagates_nonzero_exit_as_error() {
    let runner = FakeRunner::new();
    runner.set_initial_query("");
    let mut ctrl = PipeWireController::with_runner(Arc::new(runner.clone()));
    runner.fail_with_nonzero_status();
    let r = ctrl.set_rate(48000).await;
    assert!(r.is_err(), "nonzero exit must map to AudioError");
}

#[tokio::test]
async fn clear_forced_rate_invokes_runner_with_zero() {
    let runner = FakeRunner::new();
    runner.set_initial_query("");
    let mut ctrl = PipeWireController::with_runner(Arc::new(runner.clone()));
    ctrl.set_rate(48000).await.unwrap();
    ctrl.clear_forced_rate().await.expect("clear ok");
    assert_eq!(ctrl.get_current_rate(), None);
    let calls = runner.calls();
    assert!(
        calls.iter().any(|c| c.contains(&"0".to_string())),
        "must call with value 0; got {:?}",
        calls
    );
}

#[tokio::test]
async fn clear_forced_rate_propagates_runner_error() {
    let runner = FakeRunner::new();
    runner.set_initial_query("");
    let mut ctrl = PipeWireController::with_runner(Arc::new(runner.clone()));
    runner.fail_next_with("dead");
    let r = ctrl.clear_forced_rate().await;
    assert!(r.is_err());
}

#[tokio::test]
async fn drop_restores_original_rate_when_set() {
    let runner = FakeRunner::new();
    runner.set_initial_query("update: id:0 key:'clock.force-rate' value:'44100' type:''");
    {
        let _ctrl = PipeWireController::with_runner(Arc::new(runner.clone()));
    }
    let calls = runner.calls();
    let restored = calls
        .iter()
        .rev()
        .find(|c| c.len() == 5 && c[3] == "clock.force-rate")
        .expect("restore call captured");
    assert_eq!(
        restored[4], "44100",
        "Drop must restore the original rate via run_blocking; got {:?}",
        restored
    );
}

#[tokio::test]
async fn drop_clears_rate_when_original_was_zero() {
    let runner = FakeRunner::new();
    runner.set_initial_query("update: id:0 key:'clock.force-rate' value:'0' type:''");
    {
        let _ctrl = PipeWireController::with_runner(Arc::new(runner.clone()));
    }
    let calls = runner.calls();
    let last_set = calls
        .iter()
        .rev()
        .find(|c| c.len() == 5 && c[3] == "clock.force-rate")
        .expect("restore call captured");
    assert_eq!(last_set[4], "0");
}
