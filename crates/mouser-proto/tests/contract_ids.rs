use ed25519_dalek::SigningKey;
use mouser_proto::ids::{ComputerId, DeskId, DevicePublicKey};
use sha2::{Digest, Sha256};

#[test]
fn computer_id_is_first_16_bytes_of_sha256_of_device_key() {
    let key = SigningKey::from_bytes(&[7u8; 32])
        .verifying_key()
        .to_bytes();
    let id = ComputerId::from_public_key(&DevicePublicKey(key));
    assert_eq!(id.0[..], Sha256::digest(key)[..16]);
}

#[test]
fn ids_render_as_32_lowercase_hex_and_parse_back() {
    let id = ComputerId([0xab, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff]);
    let s = id.to_string();
    assert_eq!(s, format!("ab01{}ff", "00".repeat(13)));
    assert_eq!(s.len(), 32);
    assert_eq!(s.parse::<ComputerId>().unwrap(), id);
    let desk = DeskId([9; 16]);
    assert_eq!(desk.to_string(), "09".repeat(16));
    assert_eq!(desk.to_string().parse::<DeskId>().unwrap(), desk);
    let key = DevicePublicKey([0xee; 32]);
    assert_eq!(key.to_string().parse::<DevicePublicKey>().unwrap(), key);
}

#[test]
fn parsing_rejects_wrong_length_uppercase_and_non_hex() {
    assert!("abcd".parse::<DeskId>().is_err());
    assert!("AB".repeat(16).parse::<DeskId>().is_err());
    assert!("zz".repeat(16).parse::<DeskId>().is_err());
    assert!("0g".repeat(16).parse::<DeskId>().is_err());
    assert!("é".repeat(16).parse::<DeskId>().is_err());
    assert!("+1".repeat(16).parse::<DeskId>().is_err());
    assert_eq!(
        "x".parse::<DeskId>().unwrap_err().to_string(),
        "expected 32 lowercase hex characters"
    );
}

#[test]
fn serde_uses_hex_in_json_and_postcard() {
    let id = DeskId([0x5a; 16]);
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, format!("\"{}\"", "5a".repeat(16)));
    assert_eq!(serde_json::from_str::<DeskId>(&json).unwrap(), id);
    assert!(serde_json::from_str::<DeskId>("\"nothex\"").is_err());
    let bytes = postcard::to_stdvec(&id).unwrap();
    assert_eq!(bytes[0], 32);
    assert_eq!(&bytes[1..], "5a".repeat(16).as_bytes());
    assert_eq!(postcard::from_bytes::<DeskId>(&bytes).unwrap(), id);
}

fn serde_roundtrip<T>(id: T, raw_len: usize)
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug + ToString,
{
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, format!("\"{}\"", id.to_string()));
    assert_eq!(serde_json::from_str::<T>(&json).unwrap(), id);
    let value = serde_json::to_value(&id).unwrap();
    assert_eq!(value, serde_json::Value::String(id.to_string()));
    assert_eq!(serde_json::from_value::<T>(value).unwrap(), id);
    assert!(serde_json::from_str::<T>("\"nothex\"").is_err());
    let bytes = postcard::to_stdvec(&id).unwrap();
    assert_eq!(bytes.len(), raw_len);
    assert_eq!(postcard::from_bytes::<T>(&bytes).unwrap(), id);
}

#[test]
fn every_id_type_serializes_as_hex_in_json_and_postcard() {
    serde_roundtrip(ComputerId([0x11; 16]), 33);
    serde_roundtrip(DeskId([0x22; 16]), 33);
    serde_roundtrip(DevicePublicKey([0x33; 32]), 65);
}
