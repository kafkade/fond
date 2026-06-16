use std::sync::Once;

use crate::ScrapeError;

/// Ensures the process-wide default credential store is registered exactly once.
static STORE_INIT: Once = Once::new();

/// Register the platform's native credential store as the `keyring-core` default.
///
/// `keyring-core` requires a default store to be set before any
/// [`keyring_core::Entry`] is created. We register the platform's native
/// backend. This is idempotent — only the first call has any effect.
///
/// On platforms without a supported native store, no store is registered and
/// subsequent entry operations surface a `NoDefaultStore` error.
fn init_store() {
    STORE_INIT.call_once(|| {
        #[cfg(target_os = "macos")]
        if let Ok(store) = apple_native_keyring_store::keychain::Store::new() {
            keyring_core::set_default_store(store);
        }
        #[cfg(target_os = "windows")]
        if let Ok(store) = windows_native_keyring_store::Store::new() {
            keyring_core::set_default_store(store);
        }
        #[cfg(target_os = "linux")]
        if let Ok(store) = linux_keyutils_keyring_store::Store::new() {
            keyring_core::set_default_store(store);
        }
    });
}

/// Credential storage using the OS keychain.
///
/// Uses the `keyring-core` crate to store and retrieve credentials securely
/// via the platform's native credential manager:
///
/// - **macOS**: Keychain
/// - **Windows**: Credential Manager
/// - **Linux**: kernel keyutils keyring
///
/// Credentials are stored per service + username, identified by a
/// service name (e.g., `"fond-my-service"`).
pub struct CredentialStore;

impl CredentialStore {
    /// Store a credential in the OS keychain.
    ///
    /// Overwrites any existing credential for the same service + username.
    pub fn store(service: &str, username: &str, password: &str) -> Result<(), ScrapeError> {
        init_store();
        let entry = keyring_core::Entry::new(service, username)
            .map_err(|e| ScrapeError::CredentialError(e.to_string()))?;
        entry
            .set_password(password)
            .map_err(|e| ScrapeError::CredentialError(e.to_string()))
    }

    /// Load a credential from the OS keychain.
    ///
    /// Returns `None` if no credential is stored for this service + username.
    pub fn load(service: &str, username: &str) -> Result<Option<String>, ScrapeError> {
        init_store();
        let entry = keyring_core::Entry::new(service, username)
            .map_err(|e| ScrapeError::CredentialError(e.to_string()))?;
        match entry.get_password() {
            Ok(password) => Ok(Some(password)),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(e) => Err(ScrapeError::CredentialError(e.to_string())),
        }
    }

    /// Delete a credential from the OS keychain.
    ///
    /// Returns `true` if a credential was deleted, `false` if none existed.
    pub fn delete(service: &str, username: &str) -> Result<bool, ScrapeError> {
        init_store();
        let entry = keyring_core::Entry::new(service, username)
            .map_err(|e| ScrapeError::CredentialError(e.to_string()))?;
        match entry.delete_credential() {
            Ok(()) => Ok(true),
            Err(keyring_core::Error::NoEntry) => Ok(false),
            Err(e) => Err(ScrapeError::CredentialError(e.to_string())),
        }
    }
}

/// Well-known service names for fond credential storage.
///
/// These constants define the keyring service identifiers. Currently no
/// services are supported for authenticated import (NYT Cooking and ATK
/// prohibit automated access — see docs/due-diligence/nyt-atk-scraping-review.md).
/// This infrastructure exists for future permitted auth sources.
pub mod service_names {
    /// Placeholder — reserved for future use if a service permits authenticated access.
    pub const _RESERVED: &str = "fond-reserved";
}

#[cfg(test)]
mod tests {
    use super::*;

    // NOTE: These tests interact with the real OS keychain.
    // They use a unique service name to avoid collisions.

    const TEST_SERVICE: &str = "fond-test-credential-store";
    const TEST_USER: &str = "test-user";

    #[test]
    fn load_missing_returns_none() {
        // Clean up first in case a previous test left it
        let _ = CredentialStore::delete(TEST_SERVICE, TEST_USER);
        let result = CredentialStore::load(TEST_SERVICE, TEST_USER).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    #[ignore = "requires a working OS keychain (macOS Keychain, Windows Credential Manager, or Linux keyutils)"]
    fn store_load_delete_roundtrip() {
        let _ = CredentialStore::delete(TEST_SERVICE, TEST_USER);

        CredentialStore::store(TEST_SERVICE, TEST_USER, "s3cret").unwrap();
        let loaded = CredentialStore::load(TEST_SERVICE, TEST_USER).unwrap();
        assert_eq!(loaded, Some("s3cret".to_string()));

        let deleted = CredentialStore::delete(TEST_SERVICE, TEST_USER).unwrap();
        assert!(deleted);

        let after = CredentialStore::load(TEST_SERVICE, TEST_USER).unwrap();
        assert_eq!(after, None);
    }

    #[test]
    fn delete_missing_returns_false() {
        let _ = CredentialStore::delete(TEST_SERVICE, "nonexistent-user");
        let result = CredentialStore::delete(TEST_SERVICE, "nonexistent-user").unwrap();
        assert!(!result);
    }
}
