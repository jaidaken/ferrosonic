//! OS keychain credential storage: Secret Service on Linux, Keychain on macOS.

use std::fmt;
use std::sync::{Arc, RwLock};

use crate::secret::Secret;

/// Service name under which all ferrosonic credentials are filed.
const SERVICE: &str = "ferrosonic";

/// Failure talking to the OS keychain.
#[derive(Debug)]
pub enum KeyStoreError {
    /// No keychain backend is reachable (headless box, locked or absent
    /// Secret Service). Callers fall back to the next credential source.
    Unavailable(String),
    /// The backend exists but the operation failed.
    Backend(String),
}

impl fmt::Display for KeyStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(m) => write!(f, "OS keychain unavailable: {m}"),
            Self::Backend(m) => write!(f, "OS keychain error: {m}"),
        }
    }
}

impl std::error::Error for KeyStoreError {}

/// Result alias for keystore operations.
pub type KeyStoreResult<T> = Result<T, KeyStoreError>;

/// A credential backend keyed by `(service, account)`.
pub trait KeyStore: Send + Sync {
    /// Read the secret, `Ok(None)` when the account has no entry.
    ///
    /// # Errors
    /// Returns [`KeyStoreError`] when no keychain backend is reachable.
    fn get(&self, account: &str) -> KeyStoreResult<Option<Secret>>;
    /// Write (or overwrite) the secret for the account.
    ///
    /// # Errors
    /// Returns [`KeyStoreError`] when no keychain backend is reachable.
    fn set(&self, account: &str, secret: &Secret) -> KeyStoreResult<()>;
    /// Remove the account's entry; `Ok(())` when already absent.
    ///
    /// # Errors
    /// Returns [`KeyStoreError`] when no keychain backend is reachable.
    fn delete(&self, account: &str) -> KeyStoreResult<()>;
}

/// The real OS keychain via the `keyring` crate.
struct SystemKeyStore;

impl SystemKeyStore {
    fn entry(account: &str) -> KeyStoreResult<keyring::Entry> {
        keyring::Entry::new(SERVICE, account).map_err(map_err)
    }
}

impl KeyStore for SystemKeyStore {
    fn get(&self, account: &str) -> KeyStoreResult<Option<Secret>> {
        match Self::entry(account)?.get_secret() {
            Ok(bytes) => Ok(Some(Secret::from_bytes(bytes))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(map_err(e)),
        }
    }

    fn set(&self, account: &str, secret: &Secret) -> KeyStoreResult<()> {
        Self::entry(account)?
            .set_secret(secret.reveal_bytes())
            .map_err(map_err)
    }

    fn delete(&self, account: &str) -> KeyStoreResult<()> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(map_err(e)),
        }
    }
}

/// Map a keyring error to availability vs operational failure. A missing
/// backend or unsupported store means fall through to the next source; any
/// other failure is a real backend error worth surfacing.
fn map_err(e: keyring::Error) -> KeyStoreError {
    match e {
        keyring::Error::NoDefaultStore | keyring::Error::NotSupportedByStore(_) => {
            KeyStoreError::Unavailable(e.to_string())
        }
        other => KeyStoreError::Backend(other.to_string()),
    }
}

/// Test-only override; `None` selects the real OS keychain.
static OVERRIDE: RwLock<Option<Arc<dyn KeyStore>>> = RwLock::new(None);

fn with_backend<T>(f: impl FnOnce(&dyn KeyStore) -> T) -> T {
    let guard = OVERRIDE
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match guard.as_ref() {
        Some(store) => f(store.as_ref()),
        None => f(&SystemKeyStore),
    }
}

/// Account key uniquely identifying one server login in the keychain.
fn account(base_url: &str, username: &str) -> String {
    format!("{username}@{base_url}")
}

/// Store the password for `(base_url, username)`. Err means no keychain was
/// available; the caller should fall back to a different storage.
///
/// # Errors
/// Returns [`KeyStoreError`] when no keychain backend is reachable.
pub fn store(base_url: &str, username: &str, password: &Secret) -> KeyStoreResult<()> {
    with_backend(|s| s.set(&account(base_url, username), password))
}

/// Read the stored password. `Ok(None)` = keychain works but nothing is
/// filed; `Err` = no keychain reachable.
///
/// # Errors
/// Returns [`KeyStoreError`] when no keychain backend is reachable.
pub fn retrieve(base_url: &str, username: &str) -> KeyStoreResult<Option<Secret>> {
    with_backend(|s| s.get(&account(base_url, username)))
}

/// Delete the stored password; `Ok(())` when already absent.
///
/// # Errors
/// Returns [`KeyStoreError`] when no keychain backend is reachable.
pub fn delete(base_url: &str, username: &str) -> KeyStoreResult<()> {
    with_backend(|s| s.delete(&account(base_url, username)))
}

/// In-memory keystore for tests; never touches the OS keychain.
pub struct InMemoryKeyStore {
    map: std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>,
    /// When set, every operation reports the keychain as unavailable.
    unavailable: bool,
}

impl InMemoryKeyStore {
    /// A working in-memory store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            map: std::sync::Mutex::new(std::collections::HashMap::new()),
            unavailable: false,
        }
    }

    /// A store that fails every call as if no keychain were present.
    #[must_use]
    pub fn unavailable() -> Self {
        Self {
            map: std::sync::Mutex::new(std::collections::HashMap::new()),
            unavailable: true,
        }
    }
}

impl Default for InMemoryKeyStore {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyStore for InMemoryKeyStore {
    fn get(&self, account: &str) -> KeyStoreResult<Option<Secret>> {
        if self.unavailable {
            return Err(KeyStoreError::Unavailable("test store offline".into()));
        }
        let out = {
            let map = self
                .map
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            map.get(account).map(|b| Secret::from_bytes(b.clone()))
        };
        Ok(out)
    }

    fn set(&self, account: &str, secret: &Secret) -> KeyStoreResult<()> {
        if self.unavailable {
            return Err(KeyStoreError::Unavailable("test store offline".into()));
        }
        {
            let mut map = self
                .map
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            map.insert(account.to_string(), secret.reveal_bytes().to_vec());
        }
        Ok(())
    }

    fn delete(&self, account: &str) -> KeyStoreResult<()> {
        if self.unavailable {
            return Err(KeyStoreError::Unavailable("test store offline".into()));
        }
        {
            let mut map = self
                .map
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            map.remove(account);
        }
        Ok(())
    }
}

/// Install a test keystore as the process-global backend.
///
/// Hermetic tests use this so they never reach the OS keychain. Pair with
/// [`clear_test_store`]. Serialize callers (`#[serial]`); the override is
/// process-global.
pub fn install_test_store(store: Arc<dyn KeyStore>) {
    let mut guard = OVERRIDE
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *guard = Some(store);
}

/// Remove any installed test keystore, restoring the real OS keychain.
pub fn clear_test_store() {
    let mut guard = OVERRIDE
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *guard = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn account_key_is_username_at_base_url() {
        assert_eq!(
            account("https://nav.example", "alice"),
            "alice@https://nav.example"
        );
    }

    #[test]
    #[serial(secret_store)]
    fn store_then_retrieve_round_trips_through_backend() {
        install_test_store(Arc::new(InMemoryKeyStore::new()));
        store("https://x", "u", &Secret::from("hunter2")).unwrap();
        let got = retrieve("https://x", "u").unwrap();
        assert_eq!(got.as_ref().map(Secret::reveal), Some("hunter2"));
        clear_test_store();
    }

    #[test]
    #[serial(secret_store)]
    fn retrieve_unknown_account_is_ok_none() {
        install_test_store(Arc::new(InMemoryKeyStore::new()));
        assert!(retrieve("https://x", "nobody").unwrap().is_none());
        clear_test_store();
    }

    #[test]
    #[serial(secret_store)]
    fn delete_removes_the_entry() {
        install_test_store(Arc::new(InMemoryKeyStore::new()));
        store("https://x", "u", &Secret::from("p")).unwrap();
        delete("https://x", "u").unwrap();
        assert!(retrieve("https://x", "u").unwrap().is_none());
        clear_test_store();
    }

    #[test]
    #[serial(secret_store)]
    fn unavailable_backend_errors_on_store_and_retrieve() {
        install_test_store(Arc::new(InMemoryKeyStore::unavailable()));
        assert!(store("https://x", "u", &Secret::from("p")).is_err());
        assert!(retrieve("https://x", "u").is_err());
        clear_test_store();
    }

    #[test]
    #[serial(secret_store)]
    fn different_accounts_do_not_collide() {
        install_test_store(Arc::new(InMemoryKeyStore::new()));
        store("https://a", "u", &Secret::from("pa")).unwrap();
        store("https://b", "u", &Secret::from("pb")).unwrap();
        assert_eq!(retrieve("https://a", "u").unwrap().unwrap().reveal(), "pa");
        assert_eq!(retrieve("https://b", "u").unwrap().unwrap().reveal(), "pb");
        clear_test_store();
    }
}
