//! Exercises the pinned, unmodified Passless storage module and software key provider.
#[path = "credential.rs"]
#[allow(dead_code)]
mod credential;
use credential::{Credential, Extensions, RelyingParty, User};
use openssl::{
    bn::{BigNum, BigNumContext},
    ec::{EcGroup, EcKey, EcPoint},
    hash::MessageDigest,
    nid::Nid,
    pkey::{Id, PKey},
    sign::Verifier,
};
use soft_fido2_ctap::{CredentialKeyProvider, SecBytes, SoftwareCredentialKeyProvider};
use std::{borrow::Cow, env, fs, path::PathBuf};
const MESSAGE: &[u8] = b"Keycord Passless interoperability test";
fn verify(data: &[u8]) {
    let stored = Credential::from_bytes(data).expect("Passless decoder accepts record");
    let runtime = stored.to_soft_fido2();
    let signature = SoftwareCredentialKeyProvider
        .sign(&runtime.key, runtime.alg, MESSAGE)
        .expect("Passless signs with imported key");
    let private = stored.private_key.as_slice();
    if stored.alg == -7 {
        let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1).unwrap();
        let scalar = BigNum::from_slice(private).unwrap();
        let mut ctx = BigNumContext::new().unwrap();
        let mut point = EcPoint::new(&group).unwrap();
        point.mul_generator2(&group, &scalar, &mut ctx).unwrap();
        let key = PKey::from_ec_key(EcKey::from_public_key(&group, &point).unwrap()).unwrap();
        let mut verifier = Verifier::new(MessageDigest::sha256(), &key).unwrap();
        verifier.update(MESSAGE).unwrap();
        assert!(verifier.verify(&signature).unwrap());
    } else {
        let key = PKey::private_key_from_raw_bytes(private, Id::ED25519).unwrap();
        let public =
            PKey::public_key_from_raw_bytes(&key.raw_public_key().unwrap(), Id::ED25519).unwrap();
        assert!(
            Verifier::new_without_digest(&public)
                .unwrap()
                .verify_oneshot(&signature, MESSAGE)
                .unwrap()
        );
    }
}
fn main() {
    let args: Vec<_> = env::args().collect();
    let fixtures = PathBuf::from(&args[2]);
    for (name, alg) in [("es256", -7), ("ed25519", -8), ("ed25519-19", -19)] {
        let fixture = fixtures.join(format!("passless-{name}.cbor"));
        if args[1] == "generate" {
            let cred = Credential {
                id: Cow::Owned(vec![0x42; 32]),
                rp: RelyingParty {
                    id: "example.com".into(),
                    name: Some("Example".into()),
                },
                user: User {
                    id: Cow::Owned(vec![0x23; 16]),
                    name: if alg == -19 {
                        None
                    } else {
                        Some("alice".into())
                    },
                    display_name: Some("Alice".into()),
                },
                sign_count: 37,
                alg,
                private_key: SecBytes::from_slice(&[0x11; 32]),
                key_provider: None,
                key_format_version: None,
                created: 1700000000,
                discoverable: true,
                backup_state: soft_fido2::CredentialBackupState::BackedUp,
                extensions: Extensions {
                    cred_protect: Some(3),
                    hmac_secret: Some(true),
                    cred_random: Some(vec![0x33; 32]),
                },
            };
            let bytes = cred.to_bytes().unwrap();
            fs::write(&fixture, bytes).unwrap();
        } else {
            let out = PathBuf::from(&args[3]);
            let original = fs::read(&fixture).unwrap();
            let roundtrip = fs::read(out.join(format!("{name}.roundtrip.cbor"))).unwrap();
            assert_eq!(
                original, roundtrip,
                "Existing records preserved byte for byte"
            );
            let decoded = Credential::from_bytes(&roundtrip).unwrap();
            assert_eq!(decoded.sign_count, 37);
            assert_eq!(decoded.created, 1700000000);
            assert!(decoded.discoverable);
            assert_eq!(
                decoded.backup_state,
                soft_fido2::CredentialBackupState::BackedUp
            );
            assert_eq!(decoded.extensions.cred_protect, Some(3));
            assert_eq!(decoded.extensions.cred_random, Some(vec![0x33; 32]));
            verify(&roundtrip);
            if alg != -19 {
                let imported = fs::read(out.join(format!("{name}.import.cbor"))).unwrap();
                let decoded = Credential::from_bytes(&imported).unwrap();
                assert_eq!(decoded.sign_count, 0);
                assert_eq!(
                    decoded.backup_state,
                    soft_fido2::CredentialBackupState::NotEligible
                );
                assert!(decoded.discoverable);
                verify(&imported);
            }
        }
    }
    println!("Passless decoder and signing interoperability passed");
}
