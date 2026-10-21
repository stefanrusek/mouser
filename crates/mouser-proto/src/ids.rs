//! Identity types (data-model.md "Identity").

use std::fmt;
use std::str::FromStr;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

/// Error parsing a 32-lowercase-hex-character identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseIdError;

impl fmt::Display for ParseIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("expected 32 lowercase hex characters")
    }
}

impl std::error::Error for ParseIdError {}

fn to_hex(bytes: &[u8], f: &mut fmt::Formatter<'_>) -> fmt::Result {
    bytes.iter().try_for_each(|b| write!(f, "{b:02x}"))
}

fn nibble(c: u8) -> Result<u8, ParseIdError> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        _ => Err(ParseIdError),
    }
}

fn from_hex<const N: usize>(s: &str) -> Result<[u8; N], ParseIdError> {
    let bytes = s.as_bytes();
    if bytes.len() != N * 2 {
        return Err(ParseIdError);
    }
    let mut out = [0u8; N];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = nibble(bytes[2 * i])? << 4 | nibble(bytes[2 * i + 1])?;
    }
    Ok(out)
}

macro_rules! id_type {
    ($(#[$meta:meta])* $name:ident, $n:literal) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub [u8; $n]);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                to_hex(&self.0, f)
            }
        }

        impl FromStr for $name {
            type Err = ParseIdError;
            fn from_str(s: &str) -> Result<Self, ParseIdError> {
                from_hex::<$n>(s).map(Self)
            }
        }

        /// Serialized as lowercase hex in every format (JSON, TOML, postcard).
        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.collect_str(self)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                String::deserialize(d)?.parse().map_err(D::Error::custom)
            }
        }
    };
}

id_type!(
    /// 128-bit computer identity: first 16 bytes of SHA-256(device public key).
    ComputerId,
    16
);
id_type!(
    /// 128-bit random desk identity, immutable.
    DeskId,
    16
);
id_type!(
    /// Ed25519 device public key (32 bytes).
    DevicePublicKey,
    32
);

impl ComputerId {
    /// Derive the self-certifying id from a device public key.
    pub fn from_public_key(key: &DevicePublicKey) -> Self {
        let digest = Sha256::digest(key.0);
        let mut id = [0u8; 16];
        id.copy_from_slice(&digest[..16]);
        Self(id)
    }
}
