use crate::error::{AppError, AppResult};

/// Returns build-profile-separated OS keychain service name (docs/security.md Section 4.3).
/// Ensures debug/dev credentials never collide with release credentials.
pub fn keyring_service_name() -> &'static str {
    if cfg!(debug_assertions) {
        "dev.theorem.desktop.debug"
    } else {
        "dev.theorem.desktop"
    }
}

#[cfg(not(test))]
pub fn store_secret(credential_ref: &str, secret: &str) -> AppResult<()> {
    let clean_ref = validate_ref(credential_ref)?;
    if secret.trim().is_empty() {
        return Err(AppError::Validation("保存するシークレットが空です".into()));
    }
    let entry = keyring::Entry::new(keyring_service_name(), clean_ref)
        .map_err(|e| AppError::Keyring(e.to_string()))?;
    entry
        .set_password(secret.trim())
        .map_err(|e| AppError::Keyring(e.to_string()))?;
    Ok(())
}

#[cfg(not(test))]
pub fn read_secret(credential_ref: &str) -> AppResult<Option<String>> {
    let clean_ref = validate_ref(credential_ref)?;
    let entry = keyring::Entry::new(keyring_service_name(), clean_ref)
        .map_err(|e| AppError::Keyring(e.to_string()))?;
    match entry.get_password() {
        Ok(val) => Ok(Some(val)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(AppError::Keyring(err.to_string())),
    }
}

#[cfg(not(test))]
pub fn delete_secret(credential_ref: &str) -> AppResult<()> {
    let clean_ref = validate_ref(credential_ref)?;
    let entry = keyring::Entry::new(keyring_service_name(), clean_ref)
        .map_err(|e| AppError::Keyring(e.to_string()))?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(AppError::Keyring(err.to_string())),
    }
}

#[cfg(test)]
mod test_store {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    pub fn map() -> &'static Mutex<HashMap<String, String>> {
        static STORE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
        STORE.get_or_init(|| Mutex::new(HashMap::new()))
    }
}

#[cfg(test)]
pub fn store_secret(credential_ref: &str, secret: &str) -> AppResult<()> {
    let clean_ref = validate_ref(credential_ref)?;
    if secret.trim().is_empty() {
        return Err(AppError::Validation("保存するシークレットが空です".into()));
    }
    let key = format!("{}:{clean_ref}", keyring_service_name());
    let mut guard = test_store::map()
        .lock()
        .map_err(|_| AppError::Keyring("keyring test mutex poisoned".into()))?;
    guard.insert(key, secret.trim().to_string());
    Ok(())
}

#[cfg(test)]
pub fn read_secret(credential_ref: &str) -> AppResult<Option<String>> {
    let clean_ref = validate_ref(credential_ref)?;
    let key = format!("{}:{clean_ref}", keyring_service_name());
    let guard = test_store::map()
        .lock()
        .map_err(|_| AppError::Keyring("keyring test mutex poisoned".into()))?;
    Ok(guard.get(&key).cloned())
}

#[cfg(test)]
pub fn delete_secret(credential_ref: &str) -> AppResult<()> {
    let clean_ref = validate_ref(credential_ref)?;
    let key = format!("{}:{clean_ref}", keyring_service_name());
    let mut guard = test_store::map()
        .lock()
        .map_err(|_| AppError::Keyring("keyring test mutex poisoned".into()))?;
    guard.remove(&key);
    Ok(())
}

fn validate_ref(credential_ref: &str) -> AppResult<&str> {
    let trimmed = credential_ref.trim();
    if trimmed.is_empty()
        || trimmed.len() > 128
        || !trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
    {
        return Err(AppError::Validation(
            "不正なキーチェーン参照キー形式です".into(),
        ));
    }
    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyring_roundtrip_and_service_separation() {
        assert_eq!(keyring_service_name(), "dev.theorem.desktop.debug");
        let k = "agent:claude-test-key";
        store_secret(k, "sk-ant-test-secret-001").expect("store");
        assert_eq!(
            read_secret(k).expect("read").as_deref(),
            Some("sk-ant-test-secret-001")
        );
        delete_secret(k).expect("delete");
        assert_eq!(read_secret(k).expect("read after delete"), None);
    }

    #[test]
    fn test_keyring_rejects_invalid_ref() {
        assert!(store_secret("../bad/key", "secret").is_err());
        assert!(store_secret("valid_key", "   ").is_err());
    }
}
