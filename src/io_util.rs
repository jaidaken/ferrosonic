//! Filesystem helpers shared across config and daemon layers.

use std::io;
use std::path::Path;
use std::time::{Duration, SystemTime};

/// Remove files in the system temp dir matching `<prefix>...<suffix>` older
/// than `max_age`. Backstop for temp files leaked when the owning process was
/// SIGKILLed before its Drop-based cleanup ran; the age gate avoids deleting a
/// live instance's files.
pub fn sweep_stale_tmp_files(prefix: &str, suffix: &str, max_age: Duration) {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with(prefix) || !name.ends_with(suffix) {
            continue;
        }
        let path = entry.path();
        let stale = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age > max_age);
        if stale {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Internal filesystem abstraction so tests can inject failures at each io step. Production code uses `RealFs`; tests use the inline `FailingFs` recorder.
pub(crate) trait FileSystem {
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
    fn path_exists(&self, path: &Path) -> bool;
    fn write_then_sync(&self, path: &Path, body: &[u8]) -> io::Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn remove_file_if_exists(&self, path: &Path) -> io::Result<()>;
    fn open_and_sync_dir(&self, path: &Path) -> io::Result<()>;
}

pub(crate) struct RealFs;

impl FileSystem for RealFs {
    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir_all(path)
    }
    fn path_exists(&self, path: &Path) -> bool {
        path.exists()
    }
    fn write_then_sync(&self, path: &Path, body: &[u8]) -> io::Result<()> {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?;
        f.write_all(body)?;
        f.sync_all()?;
        Ok(())
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
    }
    fn remove_file_if_exists(&self, path: &Path) -> io::Result<()> {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }
    fn open_and_sync_dir(&self, path: &Path) -> io::Result<()> {
        let dir = std::fs::File::open(path)?;
        dir.sync_all()
    }
}

/// Best-effort parent-dir fsync after atomic rename so directory entry survives power loss on writeback filesystems. Silent on error since the rename itself succeeded.
///
/// ```
/// use ferrosonic::io_util::{atomic_write_bytes, fsync_parent_dir};
/// let dir = tempfile::tempdir().unwrap();
/// let p = dir.path().join("a.txt");
/// atomic_write_bytes(&p, b"x").unwrap();
/// fsync_parent_dir(&p);
/// assert!(p.exists());
/// ```
pub fn fsync_parent_dir(path: &Path) {
    record_public_fsync_call();
    fsync_parent_dir_with_fs(&RealFs, path);
}

#[cfg(not(test))]
fn record_public_fsync_call() {}

#[cfg(test)]
fn record_public_fsync_call() {
    public_fsync_calls::CALL_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
}

#[cfg(test)]
pub(crate) mod public_fsync_calls {
    use std::sync::atomic::AtomicUsize;
    pub(crate) static CALL_COUNT: AtomicUsize = AtomicUsize::new(0);
}

pub(crate) fn fsync_parent_dir_with_fs<F: FileSystem>(fs: &F, path: &Path) {
    if let Some(parent) = path.parent() {
        let _ = fs.open_and_sync_dir(parent);
    }
}

/// Atomic bytes-to-file via temp + fsync + rename + parent-dir fsync. Single audited entry point for the temp+rename pattern; callers using this avoid the disallowed_methods lint by routing through here.
///
/// ```
/// use ferrosonic::io_util::atomic_write_bytes;
/// let dir = tempfile::tempdir().unwrap();
/// let p = dir.path().join("x.toml");
/// atomic_write_bytes(&p, b"hello").unwrap();
/// assert_eq!(std::fs::read(&p).unwrap(), b"hello");
/// ```
pub fn atomic_write_bytes(path: &Path, body: &[u8]) -> std::io::Result<()> {
    atomic_write_bytes_with_fs(&RealFs, path, body)
}

pub(crate) fn atomic_write_bytes_with_fs<F: FileSystem>(
    fs: &F,
    path: &Path,
    body: &[u8],
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !fs.path_exists(parent) {
            fs.create_dir_all(parent)?;
        }
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("dat");
    let tmp = path.with_extension(format!("{}.tmp", ext));
    if let Err(e) = fs.write_then_sync(&tmp, body) {
        let _ = fs.remove_file_if_exists(&tmp);
        return Err(e);
    }
    if let Err(e) = fs.rename(&tmp, path) {
        let _ = fs.remove_file_if_exists(&tmp);
        return Err(e);
    }
    fsync_parent_dir_with_fs(fs, path);
    Ok(())
}

#[cfg(test)]
mod atomic_write_bytes_smoke {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn temp_then_rename_lands_content() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("x.toml");
        atomic_write_bytes(&p, b"hello").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "hello");
    }
}

#[cfg(test)]
mod fault_injection_tests {
    use super::*;
    use std::cell::RefCell;
    use std::io::ErrorKind;
    use std::path::PathBuf;

    #[derive(Default)]
    struct Calls {
        create_dir_all: Vec<PathBuf>,
        path_exists: Vec<PathBuf>,
        write_then_sync: Vec<(PathBuf, Vec<u8>)>,
        rename: Vec<(PathBuf, PathBuf)>,
        remove_file_if_exists: Vec<PathBuf>,
        open_and_sync_dir: Vec<PathBuf>,
    }

    #[derive(Default)]
    struct FailingFs {
        calls: RefCell<Calls>,
        fail_create_dir_all: bool,
        fail_write_then_sync: bool,
        fail_rename: bool,
        fail_remove_file: bool,
        fail_open_and_sync_dir: bool,
        path_exists_response: bool,
    }

    impl FileSystem for FailingFs {
        fn create_dir_all(&self, path: &Path) -> io::Result<()> {
            self.calls
                .borrow_mut()
                .create_dir_all
                .push(path.to_path_buf());
            if self.fail_create_dir_all {
                Err(io::Error::new(ErrorKind::Other, "synthetic create_dir_all"))
            } else {
                Ok(())
            }
        }
        fn path_exists(&self, path: &Path) -> bool {
            self.calls.borrow_mut().path_exists.push(path.to_path_buf());
            self.path_exists_response
        }
        fn write_then_sync(&self, path: &Path, body: &[u8]) -> io::Result<()> {
            self.calls
                .borrow_mut()
                .write_then_sync
                .push((path.to_path_buf(), body.to_vec()));
            if self.fail_write_then_sync {
                Err(io::Error::new(
                    ErrorKind::Other,
                    "synthetic write_then_sync",
                ))
            } else {
                Ok(())
            }
        }
        fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
            self.calls
                .borrow_mut()
                .rename
                .push((from.to_path_buf(), to.to_path_buf()));
            if self.fail_rename {
                Err(io::Error::new(ErrorKind::Other, "synthetic rename"))
            } else {
                Ok(())
            }
        }
        fn remove_file_if_exists(&self, path: &Path) -> io::Result<()> {
            self.calls
                .borrow_mut()
                .remove_file_if_exists
                .push(path.to_path_buf());
            if self.fail_remove_file {
                Err(io::Error::new(ErrorKind::Other, "synthetic remove"))
            } else {
                Ok(())
            }
        }
        fn open_and_sync_dir(&self, path: &Path) -> io::Result<()> {
            self.calls
                .borrow_mut()
                .open_and_sync_dir
                .push(path.to_path_buf());
            if self.fail_open_and_sync_dir {
                Err(io::Error::new(ErrorKind::Other, "synthetic fsync"))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn fsync_parent_dir_invokes_open_and_sync_on_parent() {
        let fs = FailingFs::default();
        fsync_parent_dir_with_fs(&fs, Path::new("/tmp/some/file.txt"));
        let calls = fs.calls.borrow();
        assert_eq!(calls.open_and_sync_dir.len(), 1);
        assert_eq!(calls.open_and_sync_dir[0], PathBuf::from("/tmp/some"));
    }

    #[test]
    fn fsync_parent_dir_still_invokes_fsync_even_when_open_fails() {
        let fs = FailingFs {
            fail_open_and_sync_dir: true,
            ..FailingFs::default()
        };
        fsync_parent_dir_with_fs(&fs, Path::new("/tmp/some/file.txt"));
        let calls = fs.calls.borrow();
        assert_eq!(calls.open_and_sync_dir.len(), 1);
    }

    #[test]
    fn fsync_parent_dir_skips_when_no_parent() {
        let fs = FailingFs::default();
        fsync_parent_dir_with_fs(&fs, Path::new("/"));
        let calls = fs.calls.borrow();
        assert_eq!(calls.open_and_sync_dir.len(), 0);
    }

    #[test]
    fn atomic_write_skips_create_dir_when_parent_empty() {
        let fs = FailingFs {
            path_exists_response: false,
            ..FailingFs::default()
        };
        atomic_write_bytes_with_fs(&fs, Path::new("rel.txt"), b"x").unwrap();
        let calls = fs.calls.borrow();
        assert!(
            calls.create_dir_all.is_empty(),
            "create_dir_all called for empty parent: {:?}",
            calls.create_dir_all
        );
    }

    #[test]
    fn atomic_write_skips_create_dir_when_parent_exists() {
        let fs = FailingFs {
            path_exists_response: true,
            ..FailingFs::default()
        };
        atomic_write_bytes_with_fs(&fs, Path::new("/etc/a.toml"), b"x").unwrap();
        let calls = fs.calls.borrow();
        assert!(
            calls.create_dir_all.is_empty(),
            "create_dir_all called when parent exists: {:?}",
            calls.create_dir_all
        );
        assert_eq!(calls.path_exists.len(), 1);
        assert_eq!(calls.path_exists[0], PathBuf::from("/etc"));
    }

    #[test]
    fn atomic_write_creates_parent_when_absent_and_nonempty() {
        let fs = FailingFs {
            path_exists_response: false,
            ..FailingFs::default()
        };
        atomic_write_bytes_with_fs(&fs, Path::new("/etc/a.toml"), b"x").unwrap();
        let calls = fs.calls.borrow();
        assert_eq!(calls.create_dir_all.len(), 1);
        assert_eq!(calls.create_dir_all[0], PathBuf::from("/etc"));
    }

    #[test]
    fn atomic_write_propagates_create_dir_failure() {
        let fs = FailingFs {
            path_exists_response: false,
            fail_create_dir_all: true,
            ..FailingFs::default()
        };
        let err = atomic_write_bytes_with_fs(&fs, Path::new("/etc/a.toml"), b"x").unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Other);
        let calls = fs.calls.borrow();
        assert!(calls.write_then_sync.is_empty());
        assert!(calls.rename.is_empty());
    }

    #[test]
    fn atomic_write_propagates_write_failure_and_cleans_tmp() {
        let fs = FailingFs {
            path_exists_response: true,
            fail_write_then_sync: true,
            ..FailingFs::default()
        };
        let err = atomic_write_bytes_with_fs(&fs, Path::new("/etc/a.toml"), b"x").unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Other);
        let calls = fs.calls.borrow();
        assert_eq!(calls.write_then_sync.len(), 1);
        assert!(
            calls.rename.is_empty(),
            "rename must not be attempted after write failure"
        );
        assert_eq!(calls.remove_file_if_exists.len(), 1);
        assert_eq!(
            calls.remove_file_if_exists[0],
            PathBuf::from("/etc/a.toml.tmp")
        );
    }

    #[test]
    fn atomic_write_propagates_rename_failure_and_cleans_tmp() {
        let fs = FailingFs {
            path_exists_response: true,
            fail_rename: true,
            ..FailingFs::default()
        };
        let err = atomic_write_bytes_with_fs(&fs, Path::new("/etc/a.toml"), b"x").unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Other);
        let calls = fs.calls.borrow();
        assert_eq!(calls.rename.len(), 1);
        assert_eq!(calls.remove_file_if_exists.len(), 1);
        assert_eq!(
            calls.remove_file_if_exists[0],
            PathBuf::from("/etc/a.toml.tmp")
        );
        assert!(
            calls.open_and_sync_dir.is_empty(),
            "fsync parent must not run after rename failure"
        );
    }

    #[test]
    fn atomic_write_does_not_cleanup_tmp_on_success() {
        let fs = FailingFs {
            path_exists_response: true,
            ..FailingFs::default()
        };
        atomic_write_bytes_with_fs(&fs, Path::new("/etc/a.toml"), b"x").unwrap();
        let calls = fs.calls.borrow();
        assert!(
            calls.remove_file_if_exists.is_empty(),
            "remove_file must not run on success: {:?}",
            calls.remove_file_if_exists
        );
        assert_eq!(calls.write_then_sync.len(), 1);
        assert_eq!(calls.rename.len(), 1);
        assert_eq!(calls.open_and_sync_dir.len(), 1);
    }

    #[test]
    fn atomic_write_uses_dat_extension_when_no_extension() {
        let fs = FailingFs {
            path_exists_response: true,
            ..FailingFs::default()
        };
        atomic_write_bytes_with_fs(&fs, Path::new("/etc/noext"), b"x").unwrap();
        let calls = fs.calls.borrow();
        assert_eq!(
            calls.write_then_sync[0].0,
            PathBuf::from("/etc/noext.dat.tmp")
        );
        assert_eq!(calls.rename[0].0, PathBuf::from("/etc/noext.dat.tmp"));
        assert_eq!(calls.rename[0].1, PathBuf::from("/etc/noext"));
    }

    #[test]
    fn atomic_write_calls_fsync_parent_on_success_path() {
        let fs = FailingFs {
            path_exists_response: true,
            ..FailingFs::default()
        };
        atomic_write_bytes_with_fs(&fs, Path::new("/etc/a.toml"), b"x").unwrap();
        let calls = fs.calls.borrow();
        assert_eq!(calls.open_and_sync_dir.len(), 1);
        assert_eq!(calls.open_and_sync_dir[0], PathBuf::from("/etc"));
    }

    #[test]
    fn atomic_write_writes_body_bytes_to_tmp_path() {
        let fs = FailingFs {
            path_exists_response: true,
            ..FailingFs::default()
        };
        atomic_write_bytes_with_fs(&fs, Path::new("/etc/a.toml"), b"hello-world").unwrap();
        let calls = fs.calls.borrow();
        assert_eq!(calls.write_then_sync[0].0, PathBuf::from("/etc/a.toml.tmp"));
        assert_eq!(calls.write_then_sync[0].1, b"hello-world".to_vec());
        assert_eq!(calls.rename[0].0, PathBuf::from("/etc/a.toml.tmp"));
        assert_eq!(calls.rename[0].1, PathBuf::from("/etc/a.toml"));
    }

    #[test]
    fn public_fsync_parent_dir_invokes_inner_implementation() {
        use std::sync::atomic::Ordering;
        let before = public_fsync_calls::CALL_COUNT.load(Ordering::SeqCst);
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("probe.txt");
        fsync_parent_dir(&p);
        let after = public_fsync_calls::CALL_COUNT.load(Ordering::SeqCst);
        assert!(
            after > before,
            "public fsync_parent_dir body must run the inner impl; before={before} after={after}"
        );
    }
}
