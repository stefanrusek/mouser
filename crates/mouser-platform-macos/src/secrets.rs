//! Keychain-backed [`SecretStore`] (generic passwords under one service).

use mouser_platform::{Result, SecretStore};
use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
};

/// `errSecItemNotFound`.
const NOT_FOUND: i32 = -25300;

pub(crate) struct KeychainStore {
    service: String,
}

impl KeychainStore {
    pub(crate) fn new(service: &str) -> Self {
        KeychainStore {
            service: service.into(),
        }
    }
}

/// `None` for a missing item; any other Keychain failure crashes loudly.
fn found<T>(r: std::result::Result<T, security_framework::base::Error>) -> Option<T> {
    match r {
        Ok(v) => Some(v),
        Err(e) => {
            assert_eq!(e.code(), NOT_FOUND, "keychain: {e}");
            None
        }
    }
}

impl SecretStore for KeychainStore {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>> {
        Ok(found(get_generic_password(&self.service, name)))
    }

    fn set(&mut self, name: &str, value: &[u8]) -> Result<()> {
        set_generic_password(&self.service, name, value).expect("keychain set");
        Ok(())
    }

    fn delete(&mut self, name: &str) -> Result<()> {
        found(delete_generic_password(&self.service, name));
        Ok(())
    }
}
