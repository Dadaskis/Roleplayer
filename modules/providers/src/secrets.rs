//! OS-keyring access for provider API keys (§5.4 of AGENTS.md).
//!
//! Keys are stored in the platform keyring (Windows Credential Manager via the
//! `keyring` crate) under service `roleplayer`, username = provider id. If the
//! keyring is unavailable (e.g. headless CI), the calls degrade to `None`
//! rather than erroring — the env-var fallback in [`crate::openai`] still
//! supplies the key for development.

use roleplayer_core::errors::{AppError, Result};

const KEYRING_SERVICE: &str = "roleplayer";

/// The `Secrets` facade — the only place keys are read or written.
pub struct Secrets;

impl Secrets {
    /// Read a provider's key from the keyring; `None` when absent/unavailable.
    pub fn get(provider_id: &str) -> Result<Option<String>> {
        let entry = match keyring::Entry::new(KEYRING_SERVICE, provider_id) {
            Ok(entry) => entry,
            Err(_error) => return Ok(None),
        };
        match entry.get_password() {
            Ok(password) => Ok(Some(password)),
            Err(_error) => Ok(None),
        }
    }

    /// Store a provider's key in the keyring.
    pub fn set(provider_id: &str, value: &str) -> Result<()> {
        if value.trim().is_empty() {
            return Err(AppError::Config(
                "api key must not be empty".to_string(),
            ));
        }
        let entry = keyring::Entry::new(KEYRING_SERVICE, provider_id).map_err(
            |error| AppError::Config(format!("keyring init failed: {error}")),
        )?;
        entry.set_password(value).map_err(|error| {
            AppError::Config(format!("keyring write failed: {error}"))
        })
    }

    /// Remove a provider's key from the keyring.
    pub fn delete(provider_id: &str) -> Result<()> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, provider_id).map_err(
            |error| AppError::Config(format!("keyring init failed: {error}")),
        )?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
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
        let _ = Secrets::get("opencode-go");
    }
}
