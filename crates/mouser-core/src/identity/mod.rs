//! Device identity: an Ed25519 device key kept in the `SecretStore`; the
//! `ComputerId` is derived from its public key (data-model.md "Identity").

use ed25519_dalek::SigningKey;
use mouser_platform::SecretStore;
use mouser_proto::ids::{ComputerId, DevicePublicKey};

/// Secret-store name of the device key seed.
pub const DEVICE_KEY: &str = "device_key";

/// This computer's long-term identity.
#[derive(Clone)]
pub struct Identity {
    seed: [u8; 32],
    public: DevicePublicKey,
    id: ComputerId,
}

impl Identity {
    /// Reuse the stored device key, or generate and store one on first launch.
    pub fn load_or_create(secrets: &mut dyn SecretStore) -> Self {
        let stored = secrets.get(DEVICE_KEY).expect("secret store readable");
        let seed = match stored {
            Some(bytes) => bytes.try_into().expect("device key is 32 bytes"),
            None => {
                let mut seed = [0u8; 32];
                getrandom::fill(&mut seed).expect("operating system randomness");
                secrets
                    .set(DEVICE_KEY, &seed)
                    .expect("secret store writable");
                seed
            }
        };
        Self::from_seed(seed)
    }

    /// Identity of the Ed25519 key with this seed.
    pub fn from_seed(seed: [u8; 32]) -> Self {
        let public = DevicePublicKey(SigningKey::from_bytes(&seed).verifying_key().to_bytes());
        Self {
            seed,
            public,
            id: ComputerId::from_public_key(&public),
        }
    }

    pub fn computer_id(&self) -> ComputerId {
        self.id
    }

    pub fn public_key(&self) -> DevicePublicKey {
        self.public
    }

    /// The Ed25519 private seed (used to build the TLS certificate).
    pub fn seed(&self) -> [u8; 32] {
        self.seed
    }
}

#[cfg(test)]
mod tests;
