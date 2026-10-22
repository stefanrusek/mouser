use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

use super::minisign::verify;
use super::pubkey::UPDATER_PUBKEY;
use super::testing::Signer;

const DATA: &[u8] = b"the bundle";

fn edit(file_b64: &str, change: impl Fn(Vec<String>) -> Vec<String>) -> String {
    let text = String::from_utf8(STANDARD.decode(file_b64).unwrap()).unwrap();
    let lines = change(text.lines().map(str::to_string).collect());
    STANDARD.encode(lines.join("\n") + "\n")
}

#[test]
fn both_signature_flavours_verify() {
    let signer = Signer::new(1);
    for prehashed in [false, true] {
        let sig = signer.sign(DATA, prehashed);
        assert_eq!(verify(&signer.pubkey(), &sig, DATA), Ok(()));
    }
}

#[test]
fn changed_data_or_another_key_is_refused() {
    let (signer, other) = (Signer::new(1), Signer::new(2));
    let sig = signer.sign(DATA, true);
    assert_eq!(
        verify(&signer.pubkey(), &sig, b"other"),
        Err("signature does not match".into())
    );
    assert_eq!(
        verify(&other.pubkey(), &sig, DATA),
        Err("signed with a different key".into())
    );
    // Same key id, different key.
    let mut impostor = Signer::new(1);
    impostor.replace_key(9);
    assert_eq!(
        verify(&impostor.pubkey(), &sig, DATA),
        Err("signature does not match".into())
    );
}

#[test]
fn the_compiled_in_key_is_a_valid_minisign_key() {
    let sig = Signer::new(1).sign(DATA, true);
    // Valid key, but not the one that signed.
    assert_eq!(
        verify(UPDATER_PUBKEY, &sig, DATA),
        Err("signed with a different key".into())
    );
}

#[test]
fn a_changed_trusted_comment_is_refused() {
    let signer = Signer::new(1);
    let sig = edit(&signer.sign(DATA, false), |mut lines| {
        lines[2] = "trusted comment: forged".into();
        lines
    });
    assert_eq!(
        verify(&signer.pubkey(), &sig, DATA),
        Err("trusted comment does not match".into())
    );
}

#[test]
fn malformed_files_are_refused() {
    let signer = Signer::new(1);
    let (pubkey, sig) = (signer.pubkey(), signer.sign(DATA, false));
    assert!(
        verify("***", &sig, DATA)
            .unwrap_err()
            .contains("public key is not base64")
    );
    assert!(
        verify(&STANDARD.encode([0xff, 0xfe]), &sig, DATA)
            .unwrap_err()
            .contains("not text")
    );
    assert!(
        verify(&STANDARD.encode("one line"), &sig, DATA)
            .unwrap_err()
            .contains("incomplete")
    );
    assert!(
        verify(
            &pubkey,
            &edit(&sig, |mut l| {
                l[1] = "***".into();
                l
            }),
            DATA
        )
        .unwrap_err()
        .contains("signature is not base64")
    );
    assert!(
        verify(
            &pubkey,
            &edit(&sig, |mut l| {
                l[1] = "AAAA".into();
                l
            }),
            DATA
        )
        .unwrap_err()
        .contains("wrong length")
    );
    assert!(
        verify(&pubkey, &edit(&sig, |l| l[..2].to_vec()), DATA)
            .unwrap_err()
            .contains("incomplete")
    );
    assert!(
        verify(&pubkey, &edit(&sig, |l| l[..3].to_vec()), DATA)
            .unwrap_err()
            .contains("incomplete")
    );
    assert!(
        verify(
            &pubkey,
            &edit(&sig, |mut l| {
                l[2] = "comment".into();
                l
            }),
            DATA
        )
        .unwrap_err()
        .contains("no trusted comment")
    );
    assert!(
        verify(&pubkey, "***", DATA)
            .unwrap_err()
            .contains("signature is not base64")
    );
    assert!(
        verify(&pubkey, &STANDARD.encode([0xff, 0xfe]), DATA)
            .unwrap_err()
            .contains("not text")
    );
}

#[test]
fn unknown_algorithms_and_invalid_keys_are_refused() {
    let signer = Signer::new(1);
    let sig = signer.sign(DATA, false);
    let odd = edit(&sig, |mut l| {
        let mut raw = STANDARD.decode(&l[1]).unwrap();
        raw[0] = b'X';
        l[1] = STANDARD.encode(raw);
        l
    });
    assert_eq!(
        verify(&signer.pubkey(), &odd, DATA),
        Err("unknown signature algorithm".into())
    );
    // 32 bytes that are not a curve point.
    let off_curve = (0..=255u8)
        .find(|v| ed25519_dalek::VerifyingKey::from_bytes(&[*v; 32]).is_err())
        .expect("some repeated byte is not a point");
    let bad_key = edit(&signer.pubkey(), |mut l| {
        let mut raw = STANDARD.decode(&l[1]).unwrap();
        raw[10..].fill(off_curve);
        l[1] = STANDARD.encode(raw);
        l
    });
    assert_eq!(
        verify(&bad_key, &sig, DATA),
        Err("public key is not valid".into())
    );
}
