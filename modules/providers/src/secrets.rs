//! OS-keyring access for provider API keys (§5.4 of AGENTS.md).
//!
//! Keys are stored in the platform keyring (Windows Credential Manager via the
//! `keyring` crate) under service `roleplayer`, username = provider id. If the
//! keyring is unavailable (e.g. headless CI), the calls degrade to `None`
//! rather than erroring — the env-var fallback in [`crate::openai`] still
//! supplies the key for development.
//!
//! Degrade-to-None policy: reads never fail on keyring absence; writes and
//! deletes do fail (you cannot persist a key you cannot reach) but with typed
//! Config errors. The env-var fallback (`OPENCODE_API_KEY` and friends) is a
//! development convenience layered on top by the provider adapters.

use roleplayer_core::errors::{AppError, Result};

/// Keyring service name; keys are keyed by username = provider id.
///
/// The service name scopes every key under one namespace; the per-provider id
/// used as the username keeps entries from clobbering each other.
const KEYRING_SERVICE: &str = "roleplayer";

/// The `Secrets` facade — the only place keys are read or written.
///
/// A unit struct: no state of its own, so calls are static methods on the
/// type. All keyring paths go through this type so the keyring crate never
/// leaks past this module's boundary.
pub struct Secrets;

impl Secrets {
    /// Read a provider's key from the keyring; `None` when absent/unavailable.
    ///
    /// Returns Ok(None) for every soft failure (keyring absent, entry missing,
    /// read error) so callers never distinguish "no key" from "broken keyring"
    /// — both mean "fall back to the env var".
    pub fn get(provider_id: &str) -> Result<Option<String>> {
        // Any keyring failure degrades to None so headless runs never fail.
        // The Entry::new failure mode is "no keyring backend available", e.g.
        // headless CI on Linux; that is absence, not a config error.
        let entry = match keyring::Entry::new(KEYRING_SERVICE, provider_id) {
            Ok(entry) => entry,
            Err(_error) => return Ok(None),
        };
        // A missing or unreadable key reads as absent, not as an error.
        // NoEntry (never stored) and a backend error (unreadable) collapse to
        // the same Ok(None); callers then fall back to the environment.
        match entry.get_password() {
            Ok(password) => Ok(Some(password)),
            Err(_error) => Ok(None),
        }
    }

    /// Store a provider's key in the keyring.
    ///
    /// Unlike reads, a write failure IS surfaced: the caller asked to persist
    /// a key and must know it did not stick. The keyring entry (service,
    /// username = provider id) is the same one get/delete use.
    pub fn set(provider_id: &str, value: &str) -> Result<()> {
        // Refuse blanks: an empty write would overwrite a real key with junk.
        // Trimming ignores whitespace-only values so accidental empty writes
        // cannot silently destroy an existing stored key.
        if value.trim().is_empty() {
            return Err(AppError::Config(
                "api key must not be empty".to_string(),
            ));
        }
        // Resolve the backend entry; a missing backend is a hard config error
        // here (unlike get) because storing is the explicit user intent.
        let entry = keyring::Entry::new(KEYRING_SERVICE, provider_id).map_err(
            |error| AppError::Config(format!("keyring init failed: {error}")),
        )?;
        // Overwrites any existing value for this provider id in one call.
        entry.set_password(value).map_err(|error| {
            AppError::Config(format!("keyring write failed: {error}"))
        })
    }

    /// Remove a provider's key from the keyring.
    ///
    /// Idempotent: deleting a key that is already absent is a success, so the
    /// UI can call this unconditionally when a provider is removed.
    pub fn delete(provider_id: &str) -> Result<()> {
        // Resolve the entry just like set; a missing backend is a hard error
        // because "remove my key" must not silently no-op.
        let entry = keyring::Entry::new(KEYRING_SERVICE, provider_id).map_err(
            |error| AppError::Config(format!("keyring init failed: {error}")),
        )?;
        match entry.delete_credential() {
            // The credential was there and is now gone.
            Ok(()) => Ok(()),
            // NoEntry means the key is already gone; absence is success.
            // This branch is what makes delete idempotent across retries.
            Err(keyring::Error::NoEntry) => Ok(()),
            // Any other backend failure (permission, platform error) is a
            // real problem the caller should surface to the user.
            Err(error) => {
                Err(AppError::Config(format!("keyring delete failed: {error}")))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyring_is_optional_in_headless_environments() {
        // Must never panic or error hard when the keyring is unavailable; the
        // env-var fallback carries the day in dev.
        // Whatever the environment, `get` either returns the stored key or
        // Ok(None) — never a hard failure — so headless CI stays green.
        let _ = Secrets::get("opencode-go");
    }
}
