use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

#[cfg(not(test))]
const SERVICE_NAME: &str = "omni-cli";

static CACHE: OnceLock<RwLock<HashMap<String, Option<String>>>> = OnceLock::new();

fn get_cache() -> &'static RwLock<HashMap<String, Option<String>>> {
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Store an API key in the system keychain.
///
/// # Panics
///
/// Panics if the keychain cache lock is poisoned.
#[cfg(not(test))]
pub fn store_api_key(provider: &str, api_key: &str) -> anyhow::Result<()> {
    use keyring::Entry;
    let entry = Entry::new(SERVICE_NAME, provider)?;
    entry.set_password(api_key)?;
    {
        let mut cache = get_cache().write().unwrap();
        cache.insert(provider.to_string(), Some(api_key.to_string()));
    }
    Ok(())
}

/// Retrieve an API key from the system keychain.
///
/// # Panics
///
/// Panics if the keychain cache lock is poisoned.
#[must_use]
#[cfg(not(test))]
pub fn get_api_key(provider: &str) -> Option<String> {
    // Check cache first
    {
        let cache = get_cache().read().unwrap();
        if let Some(cached) = cache.get(provider) {
            return cached.clone();
        }
    }
    // Cache miss - fetch from keychain
    let result = {
        use keyring::Entry;
        let entry = Entry::new(SERVICE_NAME, provider).ok()?;
        entry.get_password().ok()
    };
    // Store in cache (including None)
    {
        let mut cache = get_cache().write().unwrap();
        cache.insert(provider.to_string(), result.clone());
    }
    result
}

/// Delete an API key from the system keychain.
///
/// # Panics
///
/// Panics if the keychain cache lock is poisoned.
#[cfg(not(test))]
pub fn delete_api_key(provider: &str) -> anyhow::Result<()> {
    use keyring::Entry;
    let entry = Entry::new(SERVICE_NAME, provider)?;
    entry.delete_credential()?;
    {
        let mut cache = get_cache().write().unwrap();
        cache.insert(provider.to_string(), None);
    }
    Ok(())
}

#[cfg(test)]
static TEST_KEYS: OnceLock<RwLock<HashMap<String, String>>> = OnceLock::new();

#[cfg(test)]
fn get_test_store() -> &'static RwLock<HashMap<String, String>> {
    TEST_KEYS.get_or_init(|| RwLock::new(HashMap::new()))
}

#[cfg(test)]
pub fn set_test_key(provider: &str, key: &str) {
    let mut store = get_test_store().write().unwrap();
    store.insert(provider.to_string(), key.to_string());
    let mut cache = get_cache().write().unwrap();
    cache.insert(provider.to_string(), Some(key.to_string()));
}

#[cfg(test)]
pub fn clear_test_keys() {
    let mut store = get_test_store().write().unwrap();
    store.clear();
    let mut cache = get_cache().write().unwrap();
    cache.clear();
}

#[cfg(test)]
pub fn store_api_key(provider: &str, api_key: &str) -> anyhow::Result<()> {
    let mut store = get_test_store().write().unwrap();
    store.insert(provider.to_string(), api_key.to_string());
    let mut cache = get_cache().write().unwrap();
    cache.insert(provider.to_string(), Some(api_key.to_string()));
    Ok(())
}

#[cfg(test)]
pub fn get_api_key(provider: &str) -> Option<String> {
    {
        let cache = get_cache().read().unwrap();
        if let Some(cached) = cache.get(provider) {
            return cached.clone();
        }
    }
    let result = {
        let store = get_test_store().read().unwrap();
        store.get(provider).cloned()
    };
    {
        let mut cache = get_cache().write().unwrap();
        cache.insert(provider.to_string(), result.clone());
    }
    result
}

#[cfg(test)]
pub fn delete_api_key(provider: &str) -> anyhow::Result<()> {
    let mut store = get_test_store().write().unwrap();
    store.remove(provider);
    let mut cache = get_cache().write().unwrap();
    cache.insert(provider.to_string(), None);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn get_test_lock() -> &'static Mutex<()> {
        TEST_LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn test_mock_store_get_roundtrip() {
        let _lock = get_test_lock().lock().unwrap();
        clear_test_keys();

        // Initially no key
        assert_eq!(get_api_key("test-provider"), None);

        // Store a key
        store_api_key("test-provider", "sk-test-123").unwrap();

        // Now get should return it
        assert_eq!(
            get_api_key("test-provider"),
            Some("sk-test-123".to_string())
        );
    }

    #[test]
    fn test_mock_delete_removes_key() {
        let _lock = get_test_lock().lock().unwrap();
        clear_test_keys();

        // Store then delete
        store_api_key("test-provider", "sk-test-456").unwrap();
        assert_eq!(
            get_api_key("test-provider"),
            Some("sk-test-456".to_string())
        );

        delete_api_key("test-provider").unwrap();
        assert_eq!(get_api_key("test-provider"), None);
    }

    #[test]
    fn test_set_test_key_helper() {
        let _lock = get_test_lock().lock().unwrap();
        clear_test_keys();

        set_test_key("anthropic", "sk-ant-test");
        assert_eq!(get_api_key("anthropic"), Some("sk-ant-test".to_string()));
    }

    #[test]
    fn test_clear_test_keys_resets_state() {
        let _lock = get_test_lock().lock().unwrap();
        clear_test_keys();

        set_test_key("provider1", "key1");
        set_test_key("provider2", "key2");

        // Both exist
        assert!(get_api_key("provider1").is_some());
        assert!(get_api_key("provider2").is_some());

        // Clear
        clear_test_keys();

        // Both gone
        assert_eq!(get_api_key("provider1"), None);
        assert_eq!(get_api_key("provider2"), None);
    }

    #[test]
    fn test_cache_returns_consistent_results() {
        let _lock = get_test_lock().lock().unwrap();
        clear_test_keys();

        set_test_key("cached-provider", "cached-key");

        // Multiple calls should return same value (cached)
        let first = get_api_key("cached-provider");
        let second = get_api_key("cached-provider");
        let third = get_api_key("cached-provider");

        assert_eq!(first, second);
        assert_eq!(second, third);
        assert_eq!(first, Some("cached-key".to_string()));
    }
}
