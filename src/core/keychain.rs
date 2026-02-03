use keyring::Entry;

const SERVICE_NAME: &str = "omni-cli";

pub fn store_api_key(provider: &str, api_key: &str) -> anyhow::Result<()> {
    let entry = Entry::new(SERVICE_NAME, provider)?;
    entry.set_password(api_key)?;
    Ok(())
}

pub fn get_api_key(provider: &str) -> Option<String> {
    let entry = Entry::new(SERVICE_NAME, provider).ok()?;
    entry.get_password().ok()
}

pub fn delete_api_key(provider: &str) -> anyhow::Result<()> {
    let entry = Entry::new(SERVICE_NAME, provider)?;
    entry.delete_credential()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keychain_roundtrip() {
        let test_provider = "omni-cli-test-provider";
        let test_key = "test-api-key-12345";

        let _ = delete_api_key(test_provider);

        if store_api_key(test_provider, test_key).is_err() {
            eprintln!("Keychain not available in test environment, skipping");
            return;
        }

        let retrieved = get_api_key(test_provider);
        if retrieved.is_none() {
            eprintln!("Keychain read failed (mock backend?), skipping");
            return;
        }

        assert_eq!(retrieved, Some(test_key.to_string()));

        delete_api_key(test_provider).expect("should delete key");

        let after_delete = get_api_key(test_provider);
        assert_eq!(after_delete, None);
    }
}
