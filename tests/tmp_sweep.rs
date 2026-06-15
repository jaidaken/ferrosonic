//! io_util::sweep_stale_tmp_files removes only old, matching temp files.

use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ferrosonic::io_util::sweep_stale_tmp_files;

fn backdate(path: &Path, secs_ago: u64) {
    let t = (SystemTime::now() - Duration::from_secs(secs_ago))
        .duration_since(UNIX_EPOCH)
        .unwrap();
    let tv = libc::timeval {
        tv_sec: t.as_secs() as libc::time_t,
        tv_usec: 0,
    };
    let times = [tv, tv];
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    assert_eq!(
        unsafe { libc::utimes(c.as_ptr(), times.as_ptr()) },
        0,
        "utimes"
    );
}

#[test]
fn sweep_removes_only_old_matching_files() {
    let tmp = std::env::temp_dir();
    // Prefix unique to this test process so we never touch real ferrosonic files.
    let prefix = format!("ferrosonic-sweeptest-{}-", std::process::id());

    let old = tmp.join(format!("{prefix}stale.dat"));
    let fresh = tmp.join(format!("{prefix}live.dat"));
    let other = tmp.join(format!("{prefix}keep.txt")); // wrong suffix
    std::fs::write(&old, b"x").unwrap();
    std::fs::write(&fresh, b"x").unwrap();
    std::fs::write(&other, b"x").unwrap();
    backdate(&old, 7200); // 2h old
    backdate(&other, 7200);

    sweep_stale_tmp_files(&prefix, ".dat", Duration::from_secs(300));

    let results = (old.exists(), fresh.exists(), other.exists());
    let _ = std::fs::remove_file(&fresh);
    let _ = std::fs::remove_file(&other);
    let _ = std::fs::remove_file(&old);

    assert_eq!(
        results,
        (false, true, true),
        "expected (old removed, fresh kept, wrong-suffix kept)"
    );
}
