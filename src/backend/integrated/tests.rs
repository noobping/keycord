use super::crypto::IntegratedCryptoContext;
use super::entries::{
    delete_password_entry, password_entry_is_readable, read_password_entry, rename_password_entry,
    save_password_entry,
};
use super::git::{
    git_commit_private_key_requiring_unlock_for_entry,
    git_commit_private_key_requiring_unlock_for_store_recipients as git_commit_private_key_requiring_unlock_for_split_store_recipients,
};
#[cfg(feature = "fidokey")]
use super::keys::generate_fido2_private_key;
#[cfg(all(feature = "fidokey", feature = "fidopin", target_os = "linux"))]
use super::keys::set_fido2_security_key_pin;
#[cfg(feature = "hardwarekey")]
use super::keys::store_ripasso_hardware_key_bytes;
use super::keys::{
    armored_ripasso_private_key, clear_cached_unlocked_ripasso_private_keys,
    ensure_ripasso_private_key_is_ready, generate_ripasso_private_key,
    import_ripasso_private_key_bytes, is_ripasso_private_key_unlocked,
    list_connected_smartcard_keys, list_ripasso_private_keys, load_available_standard_key_ring,
    parse_managed_private_key_bytes, prepare_managed_private_key_bytes, remove_ripasso_private_key,
    reset_hardware_transport_for_tests, resolved_ripasso_own_fingerprint, ripasso_keys_dir,
    ripasso_private_key_requires_passphrase, ripasso_private_key_requires_session_unlock,
    set_hardware_transport_for_tests, unlock_ripasso_private_key_for_session,
    DiscoveredHardwareToken, HardwareSessionPolicy, HardwareTransport, HardwareTransportError,
    ManagedRipassoPrivateKeyProtection, PrivateKeyUnlockRequest,
};
#[cfg(any(feature = "hardwarekey", feature = "smartcard"))]
use super::keys::{
    discover_ripasso_hardware_keys, generate_ripasso_hardware_key,
    import_ripasso_hardware_key_bytes, ManagedRipassoHardwareKey,
};
#[cfg(feature = "fidokey")]
use super::keys::{
    reset_fido2_transport_for_tests, set_fido2_transport_for_tests, Fido2AssertionOutput,
    Fido2DeviceLabel, Fido2Enrollment, Fido2Transport, Fido2TransportError,
};
use super::paths::{recipients_file_for_label, secret_entry_relative_path};
use super::store::{
    save_store_recipients as save_split_store_recipients,
    save_store_recipients_for_relative_dir as save_split_store_recipients_for_relative_dir,
    store_recipients_private_key_requiring_unlock,
};
use crate::backend::{
    preferred_ripasso_private_key_fingerprint_for_entry,
    required_private_key_fingerprints_for_entry, test_support::SystemBackendTestEnv,
    PasswordEntryError, PasswordEntryWriteError, PrivateKeyError, StoreRecipientsError,
    StoreRecipientsPrivateKeyRequirement,
};
use crate::preferences::Preferences;
use crate::store::recipients::split_store_recipients;
use crate::support::git::has_git_repository;
#[cfg(feature = "hardwarekey")]
use secrecy::ExposeSecret;
#[cfg(any(feature = "hardwarekey", feature = "smartcard"))]
use secrecy::SecretString;
use sequoia_openpgp::{cert::CertBuilder, crypto::Password, parse::Parse, serialize::Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

fn cert_bytes(email: &str) -> Vec<u8> {
    let (cert, _) = CertBuilder::general_purpose(Some(email))
        .generate()
        .expect("failed to generate test certificate");
    let mut bytes = Vec::new();
    cert.as_tsk()
        .serialize(&mut bytes)
        .expect("failed to serialize test certificate");
    bytes
}

fn protected_cert(email: &str) -> (sequoia_openpgp::Cert, Vec<u8>) {
    let password: Password = "hunter2".into();
    let (cert, _) = CertBuilder::general_purpose(Some(email))
        .set_password(Some(password))
        .generate()
        .expect("failed to generate password-protected certificate");
    let mut bytes = Vec::new();
    cert.as_tsk()
        .serialize(&mut bytes)
        .expect("failed to serialize protected test certificate");
    (cert, bytes)
}

fn protected_cert_bytes(email: &str) -> Vec<u8> {
    protected_cert(email).1
}

fn public_cert_bytes(email: &str) -> Vec<u8> {
    let (cert, _) = CertBuilder::general_purpose(Some(email))
        .generate()
        .expect("failed to generate public test certificate");
    let public_only = cert.strip_secret_key_material();
    let mut bytes = Vec::new();
    public_only
        .serialize(&mut bytes)
        .expect("failed to serialize public test certificate");
    bytes
}

fn save_store_recipients(
    store_root: &str,
    recipients: &[String],
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> Result<(), StoreRecipientsError> {
    let recipients = split_store_recipients(recipients);
    save_split_store_recipients(store_root, &recipients, private_key_requirement)
}

fn save_store_recipients_for_relative_dir(
    store_root: &str,
    relative_dir: &str,
    recipients: &[String],
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> Result<(), StoreRecipientsError> {
    let recipients = split_store_recipients(recipients);
    save_split_store_recipients_for_relative_dir(
        store_root,
        relative_dir,
        &recipients,
        private_key_requirement,
    )
}

fn git_commit_private_key_requiring_unlock_for_store_recipients(
    store_root: &str,
    recipients: &[String],
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> Result<Option<String>, String> {
    let recipients = split_store_recipients(recipients);
    git_commit_private_key_requiring_unlock_for_split_store_recipients(
        store_root,
        &recipients,
        private_key_requirement,
    )
}

#[cfg(feature = "hardwarekey")]
type MockHardwareGenerationResult =
    Result<(DiscoveredHardwareToken, Vec<u8>), HardwareTransportError>;

#[derive(Default)]
struct MockHardwareTransport {
    tokens: Mutex<Vec<DiscoveredHardwareToken>>,
    #[cfg(feature = "hardwarekey")]
    generation_result: Mutex<Option<MockHardwareGenerationResult>>,
    #[cfg(feature = "hardwarekey")]
    generation_requests: Mutex<Vec<super::keys::HardwareKeyGenerationRequest>>,
    decrypt_response: Mutex<Option<String>>,
    sign_response: Mutex<Option<String>>,
}

impl MockHardwareTransport {
    fn with_tokens(tokens: Vec<DiscoveredHardwareToken>) -> Self {
        Self {
            tokens: Mutex::new(tokens),
            #[cfg(feature = "hardwarekey")]
            generation_result: Mutex::new(None),
            #[cfg(feature = "hardwarekey")]
            generation_requests: Mutex::new(Vec::new()),
            decrypt_response: Mutex::new(None),
            sign_response: Mutex::new(None),
        }
    }

    #[cfg(feature = "hardwarekey")]
    fn with_generation_result(mut self, result: MockHardwareGenerationResult) -> Self {
        self.generation_result
            .get_mut()
            .expect("generation mutex poisoned")
            .replace(result);
        self
    }

    fn with_decrypt_response(mut self, plaintext: &str) -> Self {
        self.decrypt_response
            .get_mut()
            .expect("decrypt mutex poisoned")
            .replace(plaintext.to_string());
        self
    }
}

impl HardwareTransport for MockHardwareTransport {
    fn list_tokens(&self) -> Result<Vec<DiscoveredHardwareToken>, HardwareTransportError> {
        Ok(self.tokens.lock().expect("tokens mutex poisoned").clone())
    }

    #[cfg(feature = "hardwarekey")]
    fn generate_key_material(
        &self,
        request: &super::keys::HardwareKeyGenerationRequest,
    ) -> Result<(DiscoveredHardwareToken, Vec<u8>), HardwareTransportError> {
        self.generation_requests
            .lock()
            .expect("generation request mutex poisoned")
            .push(request.clone());
        self.generation_result
            .lock()
            .expect("generation mutex poisoned")
            .clone()
            .ok_or_else(|| {
                HardwareTransportError::Other(
                    "No mock hardware generation result configured.".to_string(),
                )
            })?
    }

    fn verify_session(
        &self,
        _session: &HardwareSessionPolicy,
    ) -> Result<(), HardwareTransportError> {
        Ok(())
    }

    fn decrypt_ciphertext(
        &self,
        _session: &HardwareSessionPolicy,
        _ciphertext: &[u8],
    ) -> Result<String, HardwareTransportError> {
        self.decrypt_response
            .lock()
            .expect("decrypt mutex poisoned")
            .clone()
            .ok_or_else(|| {
                HardwareTransportError::Other("No mock decrypt response configured.".to_string())
            })
    }

    fn sign_cleartext(
        &self,
        _session: &HardwareSessionPolicy,
        _data: &str,
    ) -> Result<String, HardwareTransportError> {
        self.sign_response
            .lock()
            .expect("sign mutex poisoned")
            .clone()
            .ok_or_else(|| {
                HardwareTransportError::Other("No mock sign response configured.".to_string())
            })
    }
}

struct HardwareTransportGuard;

impl HardwareTransportGuard {
    fn install(transport: Arc<dyn HardwareTransport>) -> Self {
        set_hardware_transport_for_tests(transport);
        Self
    }
}

impl Drop for HardwareTransportGuard {
    fn drop(&mut self) {
        reset_hardware_transport_for_tests();
    }
}

struct FailingHardwareTransport;

impl HardwareTransport for FailingHardwareTransport {
    fn list_tokens(&self) -> Result<Vec<DiscoveredHardwareToken>, HardwareTransportError> {
        Err(HardwareTransportError::Other(
            "Mock smartcard enumeration failure.".to_string(),
        ))
    }

    #[cfg(feature = "hardwarekey")]
    fn generate_key_material(
        &self,
        _request: &super::keys::HardwareKeyGenerationRequest,
    ) -> Result<(DiscoveredHardwareToken, Vec<u8>), HardwareTransportError> {
        Err(HardwareTransportError::Other(
            "Mock hardware generation failure.".to_string(),
        ))
    }

    fn verify_session(
        &self,
        _session: &HardwareSessionPolicy,
    ) -> Result<(), HardwareTransportError> {
        Err(HardwareTransportError::Other(
            "Mock smartcard verification failure.".to_string(),
        ))
    }

    fn decrypt_ciphertext(
        &self,
        _session: &HardwareSessionPolicy,
        _ciphertext: &[u8],
    ) -> Result<String, HardwareTransportError> {
        Err(HardwareTransportError::Other(
            "Mock smartcard decrypt failure.".to_string(),
        ))
    }

    fn sign_cleartext(
        &self,
        _session: &HardwareSessionPolicy,
        _data: &str,
    ) -> Result<String, HardwareTransportError> {
        Err(HardwareTransportError::Other(
            "Mock smartcard signing failure.".to_string(),
        ))
    }
}

#[cfg(feature = "fidokey")]
#[derive(Default)]
struct MockFido2Transport {
    enrollments: Mutex<Vec<Result<Fido2Enrollment, Fido2TransportError>>>,
    assertions: Mutex<Vec<Result<Fido2AssertionOutput, Fido2TransportError>>>,
    #[cfg(feature = "fidopin")]
    pin_setups: Mutex<Vec<Result<(), Fido2TransportError>>>,
    #[cfg(feature = "fidopin")]
    observed_pin_setups: Mutex<Vec<String>>,
}

#[cfg(feature = "fidokey")]
impl MockFido2Transport {
    fn with_enrollment_result(
        mut self,
        result: Result<Fido2Enrollment, Fido2TransportError>,
    ) -> Self {
        self.enrollments
            .get_mut()
            .expect("enrollment mutex poisoned")
            .push(result);
        self
    }

    fn with_assertion_results(
        mut self,
        results: Vec<Result<Fido2AssertionOutput, Fido2TransportError>>,
    ) -> Self {
        self.assertions
            .get_mut()
            .expect("assertion mutex poisoned")
            .extend(results);
        self
    }

    #[cfg(feature = "fidopin")]
    fn with_pin_setup_result(mut self, result: Result<(), Fido2TransportError>) -> Self {
        self.pin_setups
            .get_mut()
            .expect("pin setup mutex poisoned")
            .push(result);
        self
    }

    fn next_enrollment(&self) -> Result<Fido2Enrollment, Fido2TransportError> {
        self.enrollments
            .lock()
            .expect("enrollment mutex poisoned")
            .remove(0)
    }

    fn next_assertion(&self) -> Result<Fido2AssertionOutput, Fido2TransportError> {
        self.assertions
            .lock()
            .expect("assertion mutex poisoned")
            .remove(0)
    }

    #[cfg(feature = "fidopin")]
    fn next_pin_setup(&self) -> Result<(), Fido2TransportError> {
        self.pin_setups
            .lock()
            .expect("pin setup mutex poisoned")
            .remove(0)
    }

    #[cfg(feature = "fidopin")]
    fn observed_pin_setups(&self) -> Vec<String> {
        self.observed_pin_setups
            .lock()
            .expect("observed pin setup mutex poisoned")
            .clone()
    }
}

#[cfg(feature = "fidokey")]
impl Fido2Transport for MockFido2Transport {
    fn enroll_hmac_secret(
        &self,
        _rp_id: &str,
        _user_name: &str,
        _user_display_name: &str,
        _pin: Option<&str>,
        _salt: &[u8],
    ) -> Result<Fido2Enrollment, Fido2TransportError> {
        self.next_enrollment()
    }

    fn derive_hmac_secret(
        &self,
        _rp_id: &str,
        _credential_id: &[u8],
        _pin: Option<&str>,
        _salt: &[u8],
        _excluded_devices: &[Fido2DeviceLabel],
    ) -> Result<Fido2AssertionOutput, Fido2TransportError> {
        self.next_assertion()
    }

    #[cfg(feature = "fidopin")]
    fn set_new_pin(&self, new_pin: &str) -> Result<(), Fido2TransportError> {
        self.observed_pin_setups
            .lock()
            .expect("observed pin setup mutex poisoned")
            .push(new_pin.to_string());
        self.next_pin_setup()
    }
}

#[cfg(feature = "fidokey")]
struct Fido2TransportGuard;

#[cfg(feature = "fidokey")]
impl Fido2TransportGuard {
    fn install(transport: Arc<dyn Fido2Transport>) -> Self {
        set_fido2_transport_for_tests(transport);
        Self
    }
}

#[cfg(feature = "fidokey")]
impl Drop for Fido2TransportGuard {
    fn drop(&mut self) {
        reset_fido2_transport_for_tests();
    }
}

#[cfg(feature = "fidokey")]
fn mock_fido2_enrollment(secret: &[u8]) -> Fido2Enrollment {
    Fido2Enrollment {
        credential_id: b"mock-credential-id".to_vec(),
        device: Fido2DeviceLabel {
            manufacturer: Some("Mock".to_string()),
            product: Some("Security Key".to_string()),
            vendor_id: Some(1),
            product_id: Some(2),
        },
        hmac_secret: secret.to_vec(),
    }
}
#[cfg(feature = "fidokey")]
fn mock_fido2_assertion(secret: &[u8]) -> Fido2AssertionOutput {
    Fido2AssertionOutput {
        hmac_secret: secret.to_vec(),
        device: Some(Fido2DeviceLabel {
            manufacturer: Some("Mock".to_string()),
            product: Some("Security Key".to_string()),
            vendor_id: Some(1),
            product_id: Some(2),
        }),
    }
}

#[test]
fn ripasso_private_key_parser_reads_secret_keys() {
    let bytes = cert_bytes("Alice Example <alice@example.com>");

    let (_, key) = parse_managed_private_key_bytes(&bytes)
        .expect("expected secret key to parse as a managed private key");

    assert_eq!(key.fingerprint.len(), 40);
    assert!(key
        .user_ids
        .iter()
        .any(|user_id| user_id.contains("alice@example.com")));
}

#[test]
fn ripasso_private_key_parser_rejects_public_only_keys() {
    let (cert, _) = CertBuilder::general_purpose(Some("Bob Example <bob@example.com>"))
        .generate()
        .expect("failed to generate test certificate");
    let public_only = cert.strip_secret_key_material();
    let mut bytes = Vec::new();
    public_only
        .serialize(&mut bytes)
        .expect("failed to serialize public test certificate");

    let err = parse_managed_private_key_bytes(&bytes)
        .expect_err("public-only keys should not be accepted as managed private keys");
    assert!(matches!(err, PrivateKeyError::MissingPrivateKeyMaterial(_)));
}

#[test]
fn encrypted_private_keys_report_that_a_passphrase_is_required() {
    let password: Password = "hunter2".into();
    let (cert, _) = CertBuilder::general_purpose(Some("Carol Example <carol@example.com>"))
        .set_password(Some(password))
        .generate()
        .expect("failed to generate password-protected certificate");
    let mut bytes = Vec::new();
    cert.as_tsk()
        .serialize(&mut bytes)
        .expect("failed to serialize protected test certificate");

    assert!(ripasso_private_key_requires_passphrase(&bytes)
        .expect("expected password inspection to work"));
}

#[test]
fn protected_private_keys_can_be_unlocked_for_ripasso_storage() {
    let password: Password = "hunter2".into();
    let (cert, _) = CertBuilder::general_purpose(Some("Dana Example <dana@example.com>"))
        .set_password(Some(password))
        .generate()
        .expect("failed to generate password-protected certificate");
    let mut bytes = Vec::new();
    cert.as_tsk()
        .serialize(&mut bytes)
        .expect("failed to serialize protected test certificate");

    let (unlocked, key) = prepare_managed_private_key_bytes(&bytes, Some("hunter2"))
        .expect("expected protected key to unlock successfully");

    assert_eq!(key.fingerprint.len(), 40);
    assert!(unlocked
        .keys()
        .all(|key| key.key().has_unencrypted_secret()));
}

#[test]
#[cfg(feature = "hardwarekey")]
fn hardware_public_keys_can_be_stored_and_unlocked_for_a_session() {
    let env = SystemBackendTestEnv::new();
    env.activate_profile("hardware-key-store");
    let public_bytes = public_cert_bytes("Hardware User <hardware@example.com>");
    let _guard =
        HardwareTransportGuard::install(Arc::new(MockHardwareTransport::with_tokens(vec![
            DiscoveredHardwareToken {
                ident: "mock-token".to_string(),
                reader_hint: Some("Mock Reader".to_string()),
                cardholder_certificate: Some(public_bytes.clone()),
                signing_fingerprint: None,
                decryption_fingerprint: None,
            },
        ])));

    let discovered = discover_ripasso_hardware_keys().expect("discover hardware keys");
    assert_eq!(discovered.len(), 1);

    let imported = store_ripasso_hardware_key_bytes(
        &public_bytes,
        ManagedRipassoHardwareKey {
            ident: "mock-token".to_string(),
            signing_fingerprint: None,
            decryption_fingerprint: None,
            reader_hint: Some("Mock Reader".to_string()),
        },
    )
    .expect("store hardware key");

    assert_eq!(
        imported.protection,
        ManagedRipassoPrivateKeyProtection::HardwareOpenPgpCard
    );
    assert!(ripasso_private_key_requires_session_unlock(&imported.fingerprint).unwrap());

    unlock_ripasso_private_key_for_session(
        &imported.fingerprint,
        PrivateKeyUnlockRequest::HardwareExternal,
    )
    .expect("unlock hardware key");
    assert!(is_ripasso_private_key_unlocked(&imported.fingerprint).unwrap());
}

#[test]
#[cfg(feature = "hardwarekey")]
fn blank_hardware_tokens_can_generate_a_managed_openpgp_key() {
    let env = SystemBackendTestEnv::new();
    env.activate_profile("hardware-key-generate");
    let public_bytes = public_cert_bytes("Generated Hardware <generated@example.com>");
    let generated_token = DiscoveredHardwareToken {
        ident: "mock-token".to_string(),
        reader_hint: Some("Mock Reader".to_string()),
        cardholder_certificate: Some(public_bytes.clone()),
        signing_fingerprint: None,
        decryption_fingerprint: None,
    };
    let _guard = HardwareTransportGuard::install(Arc::new(
        MockHardwareTransport::with_tokens(vec![generated_token.clone()])
            .with_generation_result(Ok((generated_token, public_bytes))),
    ));

    let generated = generate_ripasso_hardware_key(
        "mock-token",
        Some("Mock Reader"),
        "Generated Hardware",
        "generated@example.com",
        SecretString::from("12345678"),
        SecretString::from("123456"),
        true,
    )
    .expect("generate hardware-backed key");

    assert_eq!(
        generated.protection,
        ManagedRipassoPrivateKeyProtection::HardwareOpenPgpCard
    );
    assert_eq!(
        generated
            .hardware
            .as_ref()
            .and_then(|hardware| hardware.reader_hint.as_deref()),
        Some("Mock Reader")
    );
}

#[test]
#[cfg(feature = "hardwarekey")]
fn hardware_key_generation_can_keep_existing_user_pin() {
    let env = SystemBackendTestEnv::new();
    env.activate_profile("hardware-key-generate-existing-pin");
    let public_bytes = public_cert_bytes("Existing Hardware <existing@example.com>");
    let transport = Arc::new(
        MockHardwareTransport::with_tokens(vec![DiscoveredHardwareToken {
            ident: "mock-token".to_string(),
            reader_hint: Some("Mock Reader".to_string()),
            cardholder_certificate: Some(public_bytes.clone()),
            signing_fingerprint: None,
            decryption_fingerprint: None,
        }])
        .with_generation_result(Ok((
            DiscoveredHardwareToken {
                ident: "mock-token".to_string(),
                reader_hint: Some("Mock Reader".to_string()),
                cardholder_certificate: Some(public_bytes.clone()),
                signing_fingerprint: None,
                decryption_fingerprint: None,
            },
            public_bytes,
        ))),
    );
    let _guard = HardwareTransportGuard::install(transport.clone());

    generate_ripasso_hardware_key(
        "mock-token",
        Some("Mock Reader"),
        "Existing Hardware",
        "existing@example.com",
        SecretString::from("12345678"),
        SecretString::from("654321"),
        false,
    )
    .expect("generate hardware-backed key while keeping user pin");

    let requests = transport
        .generation_requests
        .lock()
        .expect("generation request mutex poisoned");
    assert_eq!(requests.len(), 1);
    assert!(!requests[0].replace_user_pin);
    assert_eq!(requests[0].admin_pin.expose_secret(), "12345678");
    assert_eq!(requests[0].user_pin.expose_secret(), "654321");
    let request_debug = format!("{:?}", requests[0]);
    assert!(!request_debug.contains("12345678"));
    assert!(!request_debug.contains("654321"));
}

#[test]
#[cfg(feature = "hardwarekey")]
fn hardware_keys_can_decrypt_password_entries_after_unlock() {
    let env = SystemBackendTestEnv::new();
    env.activate_profile("hardware-key-decrypt");
    let public_bytes = public_cert_bytes("Hardware Read <hardware-read@example.com>");
    let _guard = HardwareTransportGuard::install(Arc::new(
        MockHardwareTransport::with_tokens(vec![DiscoveredHardwareToken {
            ident: "mock-token".to_string(),
            reader_hint: Some("Mock Reader".to_string()),
            cardholder_certificate: None,
            signing_fingerprint: None,
            decryption_fingerprint: None,
        }])
        .with_decrypt_response("supersecret\nusername: alice"),
    ));

    let imported = import_ripasso_hardware_key_bytes(
        &public_bytes,
        ManagedRipassoHardwareKey {
            ident: "mock-token".to_string(),
            signing_fingerprint: None,
            decryption_fingerprint: None,
            reader_hint: Some("Mock Reader".to_string()),
        },
    )
    .expect("import hardware public key");

    let store = env.root_dir().join("hardware-store");
    fs::create_dir_all(&store).expect("create hardware store");
    fs::write(store.join(".gpg-id"), format!("{}\n", imported.fingerprint))
        .expect("write recipients");

    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save entry for hardware key");

    unlock_ripasso_private_key_for_session(
        &imported.fingerprint,
        PrivateKeyUnlockRequest::HardwareExternal,
    )
    .expect("unlock hardware key");

    assert_eq!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect("read hardware-backed entry"),
        "supersecret\nusername: alice"
    );
}

#[test]
fn connected_smartcards_can_unlock_read_rewrite_and_save_recipients_without_import() {
    let env = SystemBackendTestEnv::new();
    env.activate_profile("connected-smartcard");
    let public_bytes = public_cert_bytes("Token User <token@example.com>");
    let transport = Arc::new(
        MockHardwareTransport::with_tokens(vec![DiscoveredHardwareToken {
            ident: "mock-token".to_string(),
            reader_hint: Some("Mock Reader".to_string()),
            cardholder_certificate: Some(public_bytes),
            signing_fingerprint: None,
            decryption_fingerprint: None,
        }])
        .with_decrypt_response("supersecret\nusername: alice"),
    );
    let _guard = HardwareTransportGuard::install(transport.clone());

    let connected = list_connected_smartcard_keys().expect("list connected smartcards");
    assert_eq!(connected.len(), 1);
    assert!(list_ripasso_private_keys()
        .expect("list stored keys")
        .into_iter()
        .all(|key| key.fingerprint != connected[0].fingerprint));

    let fingerprint = connected[0].fingerprint.clone();
    let store_root = env.store_root().to_string_lossy().to_string();
    fs::write(env.store_root().join(".gpg-id"), format!("{fingerprint}\n"))
        .expect("write recipients");

    assert_eq!(
        required_private_key_fingerprints_for_entry(&store_root, "team/service")
            .expect("resolve direct smartcard recipient"),
        vec![fingerprint.clone()]
    );

    save_password_entry(
        &store_root,
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save entry for direct smartcard");

    assert!(matches!(
        read_password_entry(&store_root, "team/service"),
        Err(PasswordEntryError::LockedPrivateKey(_))
    ));
    assert_eq!(
        store_recipients_private_key_requiring_unlock(&store_root)
            .expect("resolve locked direct smartcard"),
        Some(fingerprint.clone())
    );
    assert!(ripasso_private_key_requires_session_unlock(&fingerprint)
        .expect("inspect direct smartcard lock state"));

    let unlocked = unlock_ripasso_private_key_for_session(
        &fingerprint,
        PrivateKeyUnlockRequest::HardwareExternal,
    )
    .expect("unlock direct smartcard");
    assert_eq!(
        unlocked.protection,
        ManagedRipassoPrivateKeyProtection::HardwareOpenPgpCard
    );
    assert!(is_ripasso_private_key_unlocked(&fingerprint).expect("inspect unlocked state"));
    assert_eq!(
        read_password_entry(&store_root, "team/service").expect("read direct smartcard entry"),
        "supersecret\nusername: alice"
    );

    transport
        .decrypt_response
        .lock()
        .expect("decrypt mutex poisoned")
        .replace("updated\nusername: bob".to_string());
    save_password_entry(&store_root, "team/service", "updated\nusername: bob", true)
        .expect("rewrite direct smartcard entry");
    assert_eq!(
        read_password_entry(&store_root, "team/service").expect("read rewritten entry"),
        "updated\nusername: bob"
    );

    save_store_recipients(
        &store_root,
        std::slice::from_ref(&fingerprint),
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save recipients with direct smartcard");
}

#[test]
fn connected_smartcards_without_cardholder_certificates_are_not_exposed() {
    let env = SystemBackendTestEnv::new();
    env.activate_profile("connected-smartcard-no-cert");
    let _guard =
        HardwareTransportGuard::install(Arc::new(MockHardwareTransport::with_tokens(vec![
            DiscoveredHardwareToken {
                ident: "mock-token".to_string(),
                reader_hint: Some("Mock Reader".to_string()),
                cardholder_certificate: None,
                signing_fingerprint: None,
                decryption_fingerprint: None,
            },
        ])));

    assert!(list_connected_smartcard_keys()
        .expect("list connected smartcards")
        .is_empty());
}

#[test]
fn smartcard_enumeration_failures_do_not_block_loading_stored_key_ring() {
    let env = SystemBackendTestEnv::new();
    env.activate_profile("smartcard-enumeration-failure");
    let _guard = HardwareTransportGuard::install(Arc::new(FailingHardwareTransport));

    let bytes = protected_cert_bytes("Stored User <stored@example.com>");
    let (cert, _) = prepare_managed_private_key_bytes(&bytes, Some("hunter2"))
        .expect("prepare stored private key");
    let expected = cert.fingerprint().to_hex();

    import_ripasso_private_key_bytes(&bytes, Some("hunter2")).expect("import stored private key");

    let key_ring = load_available_standard_key_ring().expect("load available key ring");
    assert!(key_ring.values().any(|stored| stored
        .fingerprint()
        .to_hex()
        .eq_ignore_ascii_case(&expected)));
    assert!(list_ripasso_private_keys()
        .expect("list stored private keys")
        .into_iter()
        .any(|stored| stored.fingerprint.eq_ignore_ascii_case(&expected)));
    assert!(list_connected_smartcard_keys()
        .expect_err("direct smartcard inspection should still surface errors")
        .contains("Mock smartcard enumeration failure."));
}

#[cfg(all(not(feature = "hardwarekey"), feature = "smartcard"))]
#[test]
fn smartcard_only_build_allows_managed_hardware_key_add_import_but_not_setup() {
    let env = SystemBackendTestEnv::new();
    env.activate_profile("smartcard-only-hardware-key-import");
    let public_bytes = public_cert_bytes("Token User <token@example.com>");
    let _guard =
        HardwareTransportGuard::install(Arc::new(MockHardwareTransport::with_tokens(vec![
            DiscoveredHardwareToken {
                ident: "mock-token".to_string(),
                reader_hint: Some("Mock Reader".to_string()),
                cardholder_certificate: Some(public_bytes.clone()),
                signing_fingerprint: None,
                decryption_fingerprint: None,
            },
        ])));

    let discovered = discover_ripasso_hardware_keys().expect("discover hardware keys");
    assert_eq!(discovered.len(), 1);
    assert_eq!(discovered[0].ident, "mock-token");

    let imported = import_ripasso_hardware_key_bytes(
        &public_bytes,
        ManagedRipassoHardwareKey {
            ident: "mock-token".to_string(),
            signing_fingerprint: None,
            decryption_fingerprint: None,
            reader_hint: Some("Mock Reader".to_string()),
        },
    )
    .expect("import hardware public key");

    assert_eq!(
        imported.protection,
        ManagedRipassoPrivateKeyProtection::HardwareOpenPgpCard
    );
    assert!(matches!(
        generate_ripasso_hardware_key(
            "mock-token",
            Some("Mock Reader"),
            "Generated Hardware",
            "generated@example.com",
            SecretString::from("12345678"),
            SecretString::from("123456"),
            true,
        ),
        Err(PrivateKeyError::UnsupportedHardwareKey(_))
    ));
}

#[test]
fn generated_private_keys_are_stored_and_listed() {
    let env = SystemBackendTestEnv::new();
    env.activate_profile("generated-key");

    let key = generate_ripasso_private_key("Generated User", "generated@example.com", "hunter2")
        .expect("generate private key");

    assert!(is_ripasso_private_key_unlocked(&key.fingerprint).expect("inspect unlocked state"));
    assert!(key
        .user_ids
        .iter()
        .any(|user_id| user_id.contains("Generated User <generated@example.com>")));
    assert!(list_ripasso_private_keys()
        .expect("list generated keys")
        .into_iter()
        .any(|stored| stored.fingerprint == key.fingerprint));
}

#[test]
fn armored_private_keys_can_be_exported() {
    let env = SystemBackendTestEnv::new();
    env.activate_profile("exported-key");

    let key = generate_ripasso_private_key("Export User", "export@example.com", "hunter2")
        .expect("generate private key");
    let armored = armored_ripasso_private_key(&key.fingerprint).expect("export armored key");
    let parsed = sequoia_openpgp::Cert::from_bytes(armored.as_bytes()).expect("parse armored key");

    assert!(armored.starts_with("-----BEGIN PGP PRIVATE KEY BLOCK-----"));
    assert_eq!(parsed.fingerprint().to_hex(), key.fingerprint);
}

#[test]
fn armored_private_keys_can_be_reimported_from_text_bytes() {
    let env = SystemBackendTestEnv::new();
    env.activate_profile("clipboard-import");

    let key = generate_ripasso_private_key("Clipboard User", "clipboard@example.com", "hunter2")
        .expect("generate private key");
    let armored = armored_ripasso_private_key(&key.fingerprint).expect("export armored key");

    remove_ripasso_private_key(&key.fingerprint).expect("remove generated key");
    let imported = import_ripasso_private_key_bytes(armored.as_bytes(), Some("hunter2"))
        .expect("re-import armored private key");

    assert_eq!(imported.fingerprint, key.fingerprint);
}

#[test]
fn imported_private_keys_stay_encrypted_on_disk() {
    let _env = SystemBackendTestEnv::new();
    let password: Password = "hunter2".into();
    let (cert, _) = CertBuilder::general_purpose(Some("Eve Example <eve@example.com>"))
        .set_password(Some(password))
        .generate()
        .expect("failed to generate password-protected certificate");
    let mut bytes = Vec::new();
    cert.as_tsk()
        .serialize(&mut bytes)
        .expect("failed to serialize protected test certificate");

    let imported = import_ripasso_private_key_bytes(&bytes, Some("hunter2"))
        .expect("expected private key import to succeed");
    let stored_path = ripasso_keys_dir()
        .expect("expected keys dir")
        .join(imported.fingerprint.to_ascii_lowercase());
    let stored_bytes = fs::read(stored_path).expect("read stored key");
    let (stored_cert, _) =
        parse_managed_private_key_bytes(&stored_bytes).expect("parse stored key");

    assert!(ripasso_private_key_requires_passphrase(&stored_bytes).unwrap());
    assert!(stored_cert
        .keys()
        .any(|key| !key.key().has_unencrypted_secret()));
    assert!(is_ripasso_private_key_unlocked(&imported.fingerprint).unwrap());
}

#[test]
fn encrypted_private_keys_unlock_for_the_current_session_only() {
    let _env = SystemBackendTestEnv::new();
    let password: Password = "hunter2".into();
    let (cert, _) = CertBuilder::general_purpose(Some("Frank Example <frank@example.com>"))
        .set_password(Some(password))
        .generate()
        .expect("failed to generate password-protected certificate");
    let mut bytes = Vec::new();
    cert.as_tsk()
        .serialize(&mut bytes)
        .expect("failed to serialize protected test certificate");

    let imported = import_ripasso_private_key_bytes(&bytes, Some("hunter2"))
        .expect("expected private key import to succeed");
    assert!(ensure_ripasso_private_key_is_ready(&imported.fingerprint).is_ok());

    clear_cached_unlocked_ripasso_private_keys();
    assert!(!is_ripasso_private_key_unlocked(&imported.fingerprint).unwrap());
    assert!(matches!(
        ensure_ripasso_private_key_is_ready(&imported.fingerprint)
            .expect_err("locked key should not be ready"),
        PasswordEntryError::LockedPrivateKey(_)
    ));

    unlock_ripasso_private_key_for_session(
        &imported.fingerprint,
        PrivateKeyUnlockRequest::Password("hunter2".into()),
    )
    .expect("unlock private key for session");
    assert!(is_ripasso_private_key_unlocked(&imported.fingerprint).unwrap());
    assert!(ensure_ripasso_private_key_is_ready(&imported.fingerprint).is_ok());
}

#[cfg(feature = "fidokey")]
#[test]
fn fido2_private_key_unlocks_via_the_fidokey_feature() {
    let _env = SystemBackendTestEnv::new();
    let _guard = Fido2TransportGuard::install(Arc::new(
        MockFido2Transport::default()
            .with_enrollment_result(Ok(mock_fido2_enrollment(b"fidokey-secret")))
            .with_assertion_results(vec![
                Err(Fido2TransportError::PinRequired),
                Ok(mock_fido2_assertion(b"fidokey-secret")),
            ]),
    ));
    let generated = generate_fido2_private_key(None).expect("generate FIDO2-protected key");
    clear_cached_unlocked_ripasso_private_keys();

    let err = unlock_ripasso_private_key_for_session(
        &generated.fingerprint,
        PrivateKeyUnlockRequest::Fido2(None),
    )
    .expect_err("missing PIN should be reported");
    assert!(matches!(err, PrivateKeyError::Fido2PinRequired(_)));

    let unlocked = unlock_ripasso_private_key_for_session(
        &generated.fingerprint,
        PrivateKeyUnlockRequest::Fido2(Some("123456".into())),
    )
    .expect("unlock FIDO2-backed private key");

    assert_eq!(
        unlocked.protection,
        ManagedRipassoPrivateKeyProtection::Fido2HmacSecret
    );
    assert_eq!(unlocked.fingerprint, generated.fingerprint);
}

#[cfg(feature = "fidokey")]
#[test]
fn generating_a_fido2_private_key_reports_when_the_security_key_has_no_pin() {
    let _env = SystemBackendTestEnv::new();
    let _guard = Fido2TransportGuard::install(Arc::new(
        MockFido2Transport::default().with_enrollment_result(Err(Fido2TransportError::PinNotSet)),
    ));

    let err = generate_fido2_private_key(None).expect_err("missing PIN setup should be reported");

    assert!(matches!(err, PrivateKeyError::Fido2PinNotSet(_)));
}

#[cfg(all(feature = "fidokey", feature = "fidopin", target_os = "linux"))]
#[test]
fn setting_a_new_fido2_pin_allows_generating_a_private_key() {
    let _env = SystemBackendTestEnv::new();
    let transport = Arc::new(
        MockFido2Transport::default()
            .with_pin_setup_result(Ok(()))
            .with_enrollment_result(Ok(mock_fido2_enrollment(b"new-fidokey-secret"))),
    );
    let _guard = Fido2TransportGuard::install(transport.clone());

    set_fido2_security_key_pin("123456").expect("set FIDO2 security key PIN");
    let generated =
        generate_fido2_private_key(Some("123456")).expect("generate FIDO2-protected key");

    assert_eq!(transport.observed_pin_setups(), vec!["123456".to_string()]);
    assert_eq!(
        generated.protection,
        ManagedRipassoPrivateKeyProtection::Fido2HmacSecret
    );
}

#[cfg(feature = "fidokey")]
#[test]
fn exported_fido2_private_keys_import_as_managed_keys() {
    let _env = SystemBackendTestEnv::new();
    let _guard = Fido2TransportGuard::install(Arc::new(
        MockFido2Transport::default()
            .with_enrollment_result(Ok(mock_fido2_enrollment(b"travel-key-secret"))),
    ));
    let generated =
        generate_fido2_private_key(Some("123456")).expect("generate FIDO2-protected key");
    let exported =
        armored_ripasso_private_key(&generated.fingerprint).expect("export FIDO2-protected key");
    remove_ripasso_private_key(&generated.fingerprint).expect("remove generated FIDO key");

    let imported = import_ripasso_private_key_bytes(exported.as_bytes(), None)
        .expect("import FIDO2-protected key");

    assert_eq!(
        imported.protection,
        ManagedRipassoPrivateKeyProtection::Fido2HmacSecret
    );
    assert_eq!(imported.fingerprint, generated.fingerprint);
    assert!(!ripasso_private_key_requires_passphrase(exported.as_bytes()).unwrap());
    assert!(list_ripasso_private_keys()
        .expect("list private keys")
        .into_iter()
        .any(|key| key.fingerprint == imported.fingerprint));
}

#[cfg(feature = "fidokey")]
#[test]
fn exported_fido2_private_keys_reject_unsupported_manifest_metadata() {
    let _env = SystemBackendTestEnv::new();
    let _guard = Fido2TransportGuard::install(Arc::new(
        MockFido2Transport::default()
            .with_enrollment_result(Ok(mock_fido2_enrollment(b"travel-key-secret"))),
    ));
    let generated =
        generate_fido2_private_key(Some("123456")).expect("generate FIDO2-protected key");
    let exported =
        armored_ripasso_private_key(&generated.fingerprint).expect("export FIDO2-protected key");
    remove_ripasso_private_key(&generated.fingerprint).expect("remove generated FIDO key");

    let unsupported_format = exported.replacen("format = 1", "format = 99", 1);
    assert_ne!(unsupported_format, exported);
    let format_err = import_ripasso_private_key_bytes(unsupported_format.as_bytes(), None)
        .expect_err("unsupported FIDO2 manifest format should be rejected");
    assert!(format_err
        .to_string()
        .contains("Unsupported FIDO2 private key format 99."));
    assert!(
        ripasso_private_key_requires_passphrase(unsupported_format.as_bytes())
            .expect_err("unsupported FIDO2 manifest format should not be treated as valid")
            .to_string()
            .contains("Unsupported FIDO2 private key format 99.")
    );

    let unsupported_protection = exported.replacen(
        "protection = \"fido2-hmac-secret\"",
        "protection = \"password\"",
        1,
    );
    assert_ne!(unsupported_protection, exported);
    let protection_err = import_ripasso_private_key_bytes(unsupported_protection.as_bytes(), None)
        .expect_err("unsupported FIDO2 manifest protection should be rejected");
    assert!(protection_err
        .to_string()
        .contains("Unsupported FIDO2 private key protection 'password'."));
    assert!(
        ripasso_private_key_requires_passphrase(unsupported_protection.as_bytes())
            .expect_err("unsupported FIDO2 manifest protection should not be treated as valid")
            .to_string()
            .contains("Unsupported FIDO2 private key protection 'password'.")
    );

    assert!(list_ripasso_private_keys()
        .expect("list private keys after rejected imports")
        .is_empty());
}

#[cfg(feature = "fidokey")]
#[test]
fn generated_fido2_private_keys_are_listed_and_start_unlocked_when_a_pin_is_cached() {
    let _env = SystemBackendTestEnv::new();
    let _guard = Fido2TransportGuard::install(Arc::new(
        MockFido2Transport::default()
            .with_enrollment_result(Ok(mock_fido2_enrollment(b"generated-fidokey-secret"))),
    ));

    let generated =
        generate_fido2_private_key(Some("123456")).expect("generate FIDO2-protected key");

    assert_eq!(
        generated.protection,
        ManagedRipassoPrivateKeyProtection::Fido2HmacSecret
    );
    assert!(is_ripasso_private_key_unlocked(&generated.fingerprint).unwrap());
    assert!(list_ripasso_private_keys()
        .expect("list private keys")
        .into_iter()
        .any(|key| key.fingerprint == generated.fingerprint));
}

#[cfg(feature = "fidokey")]
#[test]
fn generated_fido2_private_keys_can_be_combined_with_password_keys() {
    let env = SystemBackendTestEnv::new();
    let _guard = Fido2TransportGuard::install(Arc::new(
        MockFido2Transport::default()
            .with_enrollment_result(Ok(mock_fido2_enrollment(b"mixed-fidokey-secret"))),
    ));
    let password_key = generate_ripasso_private_key("Alice", "alice@example.com", "hunter2")
        .expect("generate password-protected key");
    let fido_key =
        generate_fido2_private_key(Some("123456")).expect("generate FIDO2-protected key");
    let store = env.root_dir().join("mixed-managed-store");

    save_store_recipients(
        store.to_string_lossy().as_ref(),
        &[
            password_key.fingerprint.clone(),
            fido_key.fingerprint.clone(),
        ],
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save mixed recipients");

    let gpg_id = fs::read_to_string(store.join(".gpg-id")).expect("read .gpg-id");
    assert!(gpg_id.contains(&password_key.fingerprint));
    assert!(gpg_id.contains(&fido_key.fingerprint));
}

#[cfg(feature = "fidokey")]
#[test]
fn removing_fido2_private_keys_removes_the_stored_key() {
    let _env = SystemBackendTestEnv::new();
    let _guard = Fido2TransportGuard::install(Arc::new(
        MockFido2Transport::default()
            .with_enrollment_result(Ok(mock_fido2_enrollment(b"backup-key-secret"))),
    ));
    let imported =
        generate_fido2_private_key(Some("123456")).expect("generate FIDO2-protected key");

    remove_ripasso_private_key(&imported.fingerprint).expect("remove FIDO2 private key");

    assert!(!list_ripasso_private_keys()
        .expect("list private keys")
        .into_iter()
        .any(|key| key.fingerprint == imported.fingerprint));
}

#[test]
fn unprotected_private_keys_are_rejected_for_secure_import() {
    let _env = SystemBackendTestEnv::new();
    let bytes = cert_bytes("Grace Example <grace@example.com>");

    let err = import_ripasso_private_key_bytes(&bytes, None)
        .expect_err("unprotected private keys should be rejected");

    assert!(matches!(
        err,
        PrivateKeyError::RequiresPasswordProtection(_)
    ));
}

#[test]
fn dotted_entry_labels_keep_their_full_name() {
    assert_eq!(
        secret_entry_relative_path("chat/matrix.org").unwrap(),
        PathBuf::from("chat/matrix.org.gpg")
    );
}

#[test]
fn recipients_file_lookup_stays_inside_the_selected_store() {
    let env = SystemBackendTestEnv::new();
    let primary_store = env.root_dir().join("primary-store");
    let secondary_store = env.root_dir().join("secondary-store");

    fs::create_dir_all(primary_store.join("team")).expect("create primary store");
    fs::create_dir_all(secondary_store.join("team")).expect("create secondary store");
    fs::write(primary_store.join(".gpg-id"), "primary@example.com\n")
        .expect("write primary recipients");
    fs::write(secondary_store.join(".gpg-id"), "secondary@example.com\n")
        .expect("write secondary recipients");

    assert_eq!(
        recipients_file_for_label(secondary_store.to_string_lossy().as_ref(), "team/chat")
            .expect("resolve recipients file"),
        secondary_store.join(".gpg-id")
    );
}

#[test]
fn new_entries_can_be_saved_in_a_secondary_store() {
    let env = SystemBackendTestEnv::new();
    let password: Password = "hunter2".into();
    let (cert, _) = CertBuilder::general_purpose(Some("Store Example <store@example.com>"))
        .set_password(Some(password))
        .generate()
        .expect("failed to generate password-protected certificate");
    let mut bytes = Vec::new();
    cert.as_tsk()
        .serialize(&mut bytes)
        .expect("failed to serialize protected test certificate");
    let imported = import_ripasso_private_key_bytes(&bytes, Some("hunter2"))
        .expect("expected private key import to succeed");

    let primary_store = env.root_dir().join("primary-store");
    let secondary_store = env.root_dir().join("secondary-store");
    fs::create_dir_all(&primary_store).expect("create primary store");
    fs::create_dir_all(&secondary_store).expect("create secondary store");
    fs::write(
        primary_store.join(".gpg-id"),
        format!("{}\n", imported.fingerprint),
    )
    .expect("write primary recipients");
    fs::write(
        secondary_store.join(".gpg-id"),
        format!("{}\n", imported.fingerprint),
    )
    .expect("write secondary recipients");

    save_password_entry(
        secondary_store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save entry in secondary store");

    assert!(secondary_store.join("team/service.gpg").is_file());
    assert_eq!(
        read_password_entry(secondary_store.to_string_lossy().as_ref(), "team/service")
            .expect("read saved entry"),
        "supersecret\nusername: alice".to_string()
    );
}

#[test]
fn duplicate_entry_saves_are_classified_as_already_existing() {
    let env = SystemBackendTestEnv::new();
    let bytes = protected_cert_bytes("Store Example <store@example.com>");
    let imported = import_ripasso_private_key_bytes(&bytes, Some("hunter2"))
        .expect("expected private key import to succeed");

    let store = env.root_dir().join("secondary-store");
    fs::create_dir_all(&store).expect("create secondary store");
    fs::write(store.join(".gpg-id"), format!("{}\n", imported.fingerprint))
        .expect("write recipients");

    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save initial entry");

    let err = save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        false,
    )
    .expect_err("duplicate save should be rejected");

    assert!(matches!(
        err,
        PasswordEntryWriteError::EntryAlreadyExists(_)
    ));
}

#[test]
fn entries_are_encrypted_for_all_selected_private_keys() {
    let env = SystemBackendTestEnv::new();
    let bytes_a = protected_cert_bytes("Key A <a@example.com>");
    let bytes_b = protected_cert_bytes("Key B <b@example.com>");
    let key_a = import_ripasso_private_key_bytes(&bytes_a, Some("hunter2"))
        .expect("import first private key");
    let key_b = import_ripasso_private_key_bytes(&bytes_b, Some("hunter2"))
        .expect("import second private key");

    let store = env.root_dir().join("secondary-store");
    fs::create_dir_all(&store).expect("create secondary store");
    fs::write(
        store.join(".gpg-id"),
        format!("{}\n{}\n", key_a.fingerprint, key_b.fingerprint),
    )
    .expect("write recipients");

    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save entry for multiple recipients");

    remove_ripasso_private_key(&key_b.fingerprint).expect("remove second key");
    assert_eq!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect("read entry with first key only"),
        "supersecret\nusername: alice".to_string()
    );

    import_ripasso_private_key_bytes(&bytes_b, Some("hunter2")).expect("re-import second key");
    remove_ripasso_private_key(&key_a.fingerprint).expect("remove first key");
    assert_eq!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect("read entry with second key only"),
        "supersecret\nusername: alice".to_string()
    );
}

#[test]
fn all_keys_mode_requires_every_selected_private_key() {
    let env = SystemBackendTestEnv::new();
    let bytes_a = protected_cert_bytes("Key A <a@example.com>");
    let bytes_b = protected_cert_bytes("Key B <b@example.com>");
    let key_a = import_ripasso_private_key_bytes(&bytes_a, Some("hunter2"))
        .expect("import first private key");
    let key_b = import_ripasso_private_key_bytes(&bytes_b, Some("hunter2"))
        .expect("import second private key");

    let store = env.root_dir().join("secondary-store");
    save_store_recipients(
        store.to_string_lossy().as_ref(),
        &[key_a.fingerprint.clone(), key_b.fingerprint.clone()],
        StoreRecipientsPrivateKeyRequirement::AllManagedKeys,
    )
    .expect("save all-keys recipients");

    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save all-keys entry");

    assert_eq!(
        fs::read_to_string(store.join(".gpg-id")).expect("read recipients"),
        format!(
            "# keycord-private-key-requirement=all\n{}\n{}\n",
            key_a.fingerprint, key_b.fingerprint
        )
    );
    assert_eq!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect("read all-keys entry"),
        "supersecret\nusername: alice".to_string()
    );

    remove_ripasso_private_key(&key_b.fingerprint).expect("remove second key");
    assert!(matches!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect_err("missing one required key should fail"),
        PasswordEntryError::MissingPrivateKey(_)
    ));
    assert!(!password_entry_is_readable(
        store.to_string_lossy().as_ref(),
        "team/service"
    ));
}

#[test]
fn all_keys_mode_uses_a_nonstandard_layered_entry_format() {
    let env = SystemBackendTestEnv::new();
    let bytes_a = protected_cert_bytes("Key A <a@example.com>");
    let bytes_b = protected_cert_bytes("Key B <b@example.com>");
    let key_a = import_ripasso_private_key_bytes(&bytes_a, Some("hunter2"))
        .expect("import first private key");
    let key_b = import_ripasso_private_key_bytes(&bytes_b, Some("hunter2"))
        .expect("import second private key");

    let store = env.root_dir().join("secondary-store");
    save_store_recipients(
        store.to_string_lossy().as_ref(),
        &[key_a.fingerprint.clone(), key_b.fingerprint],
        StoreRecipientsPrivateKeyRequirement::AllManagedKeys,
    )
    .expect("save all-keys recipients");
    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save all-keys entry");

    let outer_layer = IntegratedCryptoContext::load_for_fingerprint(&key_a.fingerprint)
        .expect("load first-layer decrypt context")
        .decrypt_entry(&store.join("team/service.gpg"))
        .expect("decrypt only the first layer");

    assert!(outer_layer.starts_with("keycord-require-all-private-keys-v1\n"));
    assert_ne!(outer_layer, "supersecret\nusername: alice");
}

#[test]
fn readability_check_requires_at_least_one_ready_key_in_any_mode() {
    let env = SystemBackendTestEnv::new();
    let bytes_a = protected_cert_bytes("Key A <a@example.com>");
    let bytes_b = protected_cert_bytes("Key B <b@example.com>");
    let key_a = import_ripasso_private_key_bytes(&bytes_a, Some("hunter2"))
        .expect("import first private key");
    let key_b = import_ripasso_private_key_bytes(&bytes_b, Some("hunter2"))
        .expect("import second private key");

    let store = env.root_dir().join("secondary-store");
    fs::create_dir_all(&store).expect("create secondary store");
    fs::write(
        store.join(".gpg-id"),
        format!("{}\n{}\n", key_a.fingerprint, key_b.fingerprint),
    )
    .expect("write recipients");
    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save entry");

    assert!(password_entry_is_readable(
        store.to_string_lossy().as_ref(),
        "team/service"
    ));

    remove_ripasso_private_key(&key_a.fingerprint).expect("remove first key");
    assert!(password_entry_is_readable(
        store.to_string_lossy().as_ref(),
        "team/service"
    ));

    remove_ripasso_private_key(&key_b.fingerprint).expect("remove second key");
    assert!(!password_entry_is_readable(
        store.to_string_lossy().as_ref(),
        "team/service"
    ));
}

#[test]
fn readability_check_treats_locked_keys_as_openable() {
    let env = SystemBackendTestEnv::new();
    let bytes = protected_cert_bytes("Key A <a@example.com>");
    let key =
        import_ripasso_private_key_bytes(&bytes, Some("hunter2")).expect("import private key");

    let store = env.root_dir().join("secondary-store");
    fs::create_dir_all(&store).expect("create secondary store");
    fs::write(store.join(".gpg-id"), format!("{}\n", key.fingerprint)).expect("write recipients");
    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save entry");

    clear_cached_unlocked_ripasso_private_keys();

    assert!(password_entry_is_readable(
        store.to_string_lossy().as_ref(),
        "team/service"
    ));
    assert!(matches!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect_err("locked key should still block the actual read"),
        PasswordEntryError::LockedPrivateKey(_)
    ));
}

#[test]
fn missing_entry_renames_and_deletes_are_classified() {
    let env = SystemBackendTestEnv::new();
    let store = env.root_dir().join("secondary-store");
    fs::create_dir_all(&store).expect("create secondary store");

    let rename_err = rename_password_entry(
        store.to_string_lossy().as_ref(),
        "team/missing",
        "team/renamed",
    )
    .expect_err("missing rename should fail");
    assert!(matches!(
        rename_err,
        PasswordEntryWriteError::EntryNotFound(_)
    ));

    let delete_err = delete_password_entry(store.to_string_lossy().as_ref(), "team/missing")
        .expect_err("missing delete should fail");
    assert!(matches!(
        delete_err,
        PasswordEntryWriteError::EntryNotFound(_)
    ));
}

#[test]
fn recipient_saves_reject_non_directory_store_paths() {
    let env = SystemBackendTestEnv::new();
    let file_path = env.root_dir().join("store-file");
    fs::write(&file_path, "not a directory").expect("write store placeholder file");

    let err = save_store_recipients(
        file_path.to_string_lossy().as_ref(),
        &[String::from("alice@example.com")],
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect_err("non-directory store paths should fail");

    assert!(matches!(err, StoreRecipientsError::InvalidStorePath(_)));
}

#[test]
fn recipient_saves_initialize_git_for_new_stores() {
    let env = SystemBackendTestEnv::new();
    let bytes = protected_cert_bytes("Store Example <store@example.com>");
    let imported = import_ripasso_private_key_bytes(&bytes, Some("hunter2"))
        .expect("expected private key import to succeed");

    let store = env.root_dir().join("secondary-store");
    save_store_recipients(
        store.to_string_lossy().as_ref(),
        std::slice::from_ref(&imported.fingerprint),
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save recipients for a new store");

    assert!(has_git_repository(store.to_string_lossy().as_ref()));
}

#[test]
fn new_entries_can_use_email_recipients() {
    let env = SystemBackendTestEnv::new();
    let password: Password = "hunter2".into();
    let (cert, _) = CertBuilder::general_purpose(Some("Store Example <store@example.com>"))
        .set_password(Some(password))
        .generate()
        .expect("failed to generate password-protected certificate");
    let mut bytes = Vec::new();
    cert.as_tsk()
        .serialize(&mut bytes)
        .expect("failed to serialize protected test certificate");
    let imported = import_ripasso_private_key_bytes(&bytes, Some("hunter2"))
        .expect("expected private key import to succeed");

    let secondary_store = env.root_dir().join("secondary-store");
    fs::create_dir_all(&secondary_store).expect("create secondary store");
    fs::write(secondary_store.join(".gpg-id"), "store@example.com\n").expect("write recipients");

    save_password_entry(
        secondary_store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save entry with email recipient");

    assert!(secondary_store.join("team/service.gpg").is_file());
    assert_eq!(
        read_password_entry(secondary_store.to_string_lossy().as_ref(), "team/service")
            .expect("read saved entry"),
        "supersecret\nusername: alice".to_string()
    );
    assert_eq!(imported.fingerprint.len(), 40);
}

#[test]
fn store_recipient_updates_leave_nested_gpg_id_entries_on_their_own_recipients() {
    let env = SystemBackendTestEnv::new();
    let root_key = import_ripasso_private_key_bytes(
        &protected_cert_bytes("Root Key <root@example.com>"),
        Some("hunter2"),
    )
    .expect("import root key");
    let nested_key = import_ripasso_private_key_bytes(
        &protected_cert_bytes("Nested Key <nested@example.com>"),
        Some("hunter2"),
    )
    .expect("import nested key");
    let replacement_root_key = import_ripasso_private_key_bytes(
        &protected_cert_bytes("Replacement Root <replacement@example.com>"),
        Some("hunter2"),
    )
    .expect("import replacement root key");

    let store = env.root_dir().join("secondary-store");
    fs::create_dir_all(store.join("team")).expect("create nested store dir");
    fs::write(store.join(".gpg-id"), format!("{}\n", root_key.fingerprint))
        .expect("write root recipients");
    fs::write(
        store.join("team/.gpg-id"),
        format!("{}\n", nested_key.fingerprint),
    )
    .expect("write nested recipients");

    save_password_entry(
        store.to_string_lossy().as_ref(),
        "root-entry",
        "root secret",
        true,
    )
    .expect("save root entry");
    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "nested secret",
        true,
    )
    .expect("save nested entry");

    save_store_recipients(
        store.to_string_lossy().as_ref(),
        std::slice::from_ref(&replacement_root_key.fingerprint),
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("update store recipients");

    assert_eq!(
        fs::read_to_string(store.join(".gpg-id")).expect("read root recipients"),
        format!("{}\n", replacement_root_key.fingerprint)
    );
    assert_eq!(
        fs::read_to_string(store.join("team/.gpg-id")).expect("read nested recipients"),
        format!("{}\n", nested_key.fingerprint)
    );
    assert_eq!(
        read_password_entry(store.to_string_lossy().as_ref(), "root-entry")
            .expect("read root entry after update"),
        "root secret".to_string()
    );
    assert_eq!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect("read nested entry after update"),
        "nested secret".to_string()
    );
}

#[test]
fn store_recipient_updates_can_target_a_nested_gpg_id_scope() {
    let env = SystemBackendTestEnv::new();
    let root_key = import_ripasso_private_key_bytes(
        &protected_cert_bytes("Root Key <root-scope@example.com>"),
        Some("hunter2"),
    )
    .expect("import root key");
    let nested_key = import_ripasso_private_key_bytes(
        &protected_cert_bytes("Nested Key <nested-scope@example.com>"),
        Some("hunter2"),
    )
    .expect("import nested key");
    let replacement_nested_key = import_ripasso_private_key_bytes(
        &protected_cert_bytes("Replacement Nested <replacement-nested@example.com>"),
        Some("hunter2"),
    )
    .expect("import replacement nested key");

    let store = env.root_dir().join("scoped-store");
    fs::create_dir_all(store.join("team")).expect("create nested store dir");
    fs::write(store.join(".gpg-id"), format!("{}\n", root_key.fingerprint))
        .expect("write root recipients");
    fs::write(
        store.join("team/.gpg-id"),
        format!("{}\n", nested_key.fingerprint),
    )
    .expect("write nested recipients");

    save_password_entry(
        store.to_string_lossy().as_ref(),
        "root-entry",
        "root secret",
        true,
    )
    .expect("save root entry");
    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "nested secret",
        true,
    )
    .expect("save nested entry");

    save_store_recipients_for_relative_dir(
        store.to_string_lossy().as_ref(),
        "team",
        std::slice::from_ref(&replacement_nested_key.fingerprint),
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("update nested recipients");

    assert_eq!(
        fs::read_to_string(store.join(".gpg-id")).expect("read root recipients"),
        format!("{}\n", root_key.fingerprint)
    );
    assert_eq!(
        fs::read_to_string(store.join("team/.gpg-id")).expect("read nested recipients"),
        format!("{}\n", replacement_nested_key.fingerprint)
    );
    assert_eq!(
        read_password_entry(store.to_string_lossy().as_ref(), "root-entry")
            .expect("read root entry after nested update"),
        "root secret".to_string()
    );
    assert_eq!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect("read nested entry after nested update"),
        "nested secret".to_string()
    );
}

#[test]
fn store_recipients_work_without_a_selected_default_key() {
    let env = SystemBackendTestEnv::new();
    let password: Password = "hunter2".into();
    let (cert, _) = CertBuilder::general_purpose(Some("Store Example <store@example.com>"))
        .set_password(Some(password))
        .generate()
        .expect("failed to generate password-protected certificate");
    let mut bytes = Vec::new();
    cert.as_tsk()
        .serialize(&mut bytes)
        .expect("failed to serialize protected test certificate");
    let imported = import_ripasso_private_key_bytes(&bytes, Some("hunter2"))
        .expect("expected private key import to succeed");

    let store = env.root_dir().join("secondary-store");
    fs::create_dir_all(&store).expect("create store");
    fs::write(store.join(".gpg-id"), format!("{}\n", imported.fingerprint))
        .expect("write recipients");

    Preferences::new()
        .set_ripasso_own_fingerprint(None)
        .expect("clear selected fingerprint");
    assert!(resolved_ripasso_own_fingerprint().is_err());

    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save entry with store recipients only");

    assert_eq!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect("read saved entry"),
        "supersecret\nusername: alice".to_string()
    );
}

#[test]
fn store_recipients_save_can_decrypt_with_a_non_selected_imported_key() {
    let env = SystemBackendTestEnv::new();
    let password: Password = "hunter2".into();

    let (cert_a, _) = CertBuilder::general_purpose(Some("Key A <a@example.com>"))
        .set_password(Some(password.clone()))
        .generate()
        .expect("generate first certificate");
    let mut bytes_a = Vec::new();
    cert_a
        .as_tsk()
        .serialize(&mut bytes_a)
        .expect("serialize first certificate");
    let key_a = import_ripasso_private_key_bytes(&bytes_a, Some("hunter2"))
        .expect("import first private key");

    let (cert_b, _) = CertBuilder::general_purpose(Some("Key B <b@example.com>"))
        .set_password(Some(password))
        .generate()
        .expect("generate second certificate");
    let mut bytes_b = Vec::new();
    cert_b
        .as_tsk()
        .serialize(&mut bytes_b)
        .expect("serialize second certificate");
    let key_b = import_ripasso_private_key_bytes(&bytes_b, Some("hunter2"))
        .expect("import second private key");

    let store = env.root_dir().join("secondary-store");
    fs::create_dir_all(&store).expect("create store");
    fs::write(store.join(".gpg-id"), format!("{}\n", key_a.fingerprint))
        .expect("write initial recipients");

    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save initial entry");

    Preferences::new()
        .set_ripasso_own_fingerprint(Some(&key_b.fingerprint))
        .expect("select second key");

    save_store_recipients(
        store.to_string_lossy().as_ref(),
        std::slice::from_ref(&key_b.fingerprint),
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("re-encrypt store with second key");

    assert_eq!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect("read re-encrypted entry"),
        "supersecret\nusername: alice".to_string()
    );
}

#[test]
fn preferred_entry_key_uses_a_ready_private_key_before_a_locked_coworker_key() {
    let env = SystemBackendTestEnv::new();
    let coworker = import_ripasso_private_key_bytes(
        &protected_cert_bytes("Coworker Key <coworker@example.com>"),
        Some("hunter2"),
    )
    .expect("import coworker private key");
    let mine = import_ripasso_private_key_bytes(
        &protected_cert_bytes("My Key <me@example.com>"),
        Some("hunter2"),
    )
    .expect("import own private key");
    clear_cached_unlocked_ripasso_private_keys();
    unlock_ripasso_private_key_for_session(
        &mine.fingerprint,
        PrivateKeyUnlockRequest::Password("hunter2".into()),
    )
    .expect("unlock own private key");
    Preferences::new()
        .set_ripasso_own_fingerprint(None)
        .expect("clear selected fingerprint");

    let store = env.root_dir().join("shared-store");
    fs::create_dir_all(&store).expect("create store");
    let store_root = store.to_string_lossy().to_string();
    fs::write(
        store.join(".gpg-id"),
        format!("{}\n{}\n", coworker.fingerprint, mine.fingerprint),
    )
    .expect("write recipients");

    assert_eq!(
        preferred_ripasso_private_key_fingerprint_for_entry(&store_root, "team/service")
            .expect("resolve preferred private key"),
        mine.fingerprint
    );
}

#[test]
fn integrated_backend_uses_the_first_usable_private_key_before_a_coworker_recipient() {
    let env = SystemBackendTestEnv::new();
    let coworker = import_ripasso_private_key_bytes(
        &protected_cert_bytes("Coworker Entry <coworker-entry@example.com>"),
        Some("hunter2"),
    )
    .expect("import coworker private key");
    let mine = import_ripasso_private_key_bytes(
        &protected_cert_bytes("My Entry <my-entry@example.com>"),
        Some("hunter2"),
    )
    .expect("import own private key");
    clear_cached_unlocked_ripasso_private_keys();
    unlock_ripasso_private_key_for_session(
        &mine.fingerprint,
        PrivateKeyUnlockRequest::Password("hunter2".into()),
    )
    .expect("unlock own private key");
    Preferences::new()
        .set_ripasso_own_fingerprint(None)
        .expect("clear selected fingerprint");

    let store = env.root_dir().join("team-store");
    fs::create_dir_all(&store).expect("create store");
    let store_root = store.to_string_lossy().to_string();
    fs::write(
        store.join(".gpg-id"),
        format!("{}\n{}\n", coworker.fingerprint, mine.fingerprint),
    )
    .expect("write recipients");

    assert_eq!(
        IntegratedCryptoContext::fingerprint_for_label(&store_root, "team/service")
            .expect("resolve crypto context fingerprint"),
        mine.fingerprint
    );
}

#[test]
fn store_recipients_save_can_remove_the_selected_private_key_from_recipients() {
    let env = SystemBackendTestEnv::new();
    let password: Password = "hunter2".into();

    let (cert_a, _) = CertBuilder::general_purpose(Some("Key A <a@example.com>"))
        .set_password(Some(password.clone()))
        .generate()
        .expect("generate first certificate");
    let mut bytes_a = Vec::new();
    cert_a
        .as_tsk()
        .serialize(&mut bytes_a)
        .expect("serialize first certificate");
    let key_a = import_ripasso_private_key_bytes(&bytes_a, Some("hunter2"))
        .expect("import first private key");

    let (cert_b, _) = CertBuilder::general_purpose(Some("Key B <b@example.com>"))
        .set_password(Some(password))
        .generate()
        .expect("generate second certificate");
    let mut bytes_b = Vec::new();
    cert_b
        .as_tsk()
        .serialize(&mut bytes_b)
        .expect("serialize second certificate");
    let key_b = import_ripasso_private_key_bytes(&bytes_b, Some("hunter2"))
        .expect("import second private key");

    let store = env.root_dir().join("secondary-store");
    fs::create_dir_all(&store).expect("create store");
    fs::write(
        store.join(".gpg-id"),
        format!("{}\n{}\n", key_a.fingerprint, key_b.fingerprint),
    )
    .expect("write initial recipients");

    save_password_entry(
        store.to_string_lossy().as_ref(),
        "team/service",
        "supersecret\nusername: alice",
        true,
    )
    .expect("save initial entry");

    Preferences::new()
        .set_ripasso_own_fingerprint(Some(&key_a.fingerprint))
        .expect("select first key");

    save_store_recipients(
        store.to_string_lossy().as_ref(),
        std::slice::from_ref(&key_b.fingerprint),
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("re-encrypt store without the selected key");

    assert_eq!(
        read_password_entry(store.to_string_lossy().as_ref(), "team/service")
            .expect("read re-encrypted entry"),
        "supersecret\nusername: alice".to_string()
    );
}

#[test]
fn integrated_backend_commits_git_backed_store_changes_with_private_key_identity() {
    let env = SystemBackendTestEnv::new();
    let (cert, bytes) = protected_cert("Git Signer <git-flatpak@example.com>");
    let imported =
        import_ripasso_private_key_bytes(&bytes, Some("hunter2")).expect("import private key");
    Preferences::new()
        .set_ripasso_own_fingerprint(Some(&imported.fingerprint))
        .expect("select signing key");

    let mut public_bytes = Vec::new();
    cert.serialize(&mut public_bytes)
        .expect("serialize public certificate");
    SystemBackendTestEnv::import_public_key(&public_bytes)
        .expect("import public key for signature verification");
    env.init_store_git_repository()
        .expect("initialize git repository");
    let store_root = env.store_root().to_string_lossy().to_string();

    save_store_recipients(
        &store_root,
        std::slice::from_ref(&imported.fingerprint),
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save store recipients");
    save_password_entry(
        &store_root,
        "team/service",
        "secret-value\nusername: alice",
        true,
    )
    .expect("save password entry");

    let subjects = env
        .store_git_commit_subjects()
        .expect("read commit subjects");
    assert_eq!(subjects.len(), 2);
    assert_eq!(subjects[0], "Add password for team/service");
    assert_eq!(subjects[1], "Update password store recipients");
    assert_eq!(
        env.store_git_head_author().expect("read head author"),
        "Git Signer <git-flatpak@example.com>"
    );
    assert!(env
        .store_head_commit_has_signature()
        .expect("inspect commit headers"));
    env.verify_store_head_commit_signature()
        .expect("verify head commit signature");
}

#[test]
fn integrated_backend_commits_with_the_entry_private_key_instead_of_an_unrelated_selected_key() {
    let env = SystemBackendTestEnv::new();
    let (cert_a, bytes_a) = protected_cert("Entry Key <entry@example.com>");
    let imported_a =
        import_ripasso_private_key_bytes(&bytes_a, Some("hunter2")).expect("import entry key");
    let (cert_b, bytes_b) = protected_cert("Selected Key <selected@example.com>");
    let imported_b = import_ripasso_private_key_bytes(&bytes_b, Some("hunter2"))
        .expect("import unrelated selected key");
    Preferences::new()
        .set_ripasso_own_fingerprint(Some(&imported_b.fingerprint))
        .expect("select unrelated key");

    let mut public_bytes_a = Vec::new();
    cert_a
        .serialize(&mut public_bytes_a)
        .expect("serialize entry public certificate");
    SystemBackendTestEnv::import_public_key(&public_bytes_a)
        .expect("import entry public key for signature verification");

    let mut public_bytes_b = Vec::new();
    cert_b
        .serialize(&mut public_bytes_b)
        .expect("serialize selected public certificate");
    SystemBackendTestEnv::import_public_key(&public_bytes_b)
        .expect("import unrelated selected public key for signature verification");
    env.init_store_git_repository()
        .expect("initialize git repository");
    let store_root = env.store_root().to_string_lossy().to_string();

    save_store_recipients(
        &store_root,
        std::slice::from_ref(&imported_a.fingerprint),
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save store recipients");
    save_password_entry(
        &store_root,
        "team/service",
        "secret-value\nusername: alice",
        true,
    )
    .expect("save password entry");

    let subjects = env
        .store_git_commit_subjects()
        .expect("read commit subjects");
    assert_eq!(subjects.len(), 2);
    assert_eq!(
        env.store_git_head_author().expect("read head author"),
        "Entry Key <entry@example.com>"
    );
    assert!(env
        .store_head_commit_has_signature()
        .expect("inspect commit headers"));
    env.verify_store_head_commit_signature()
        .expect("verify head commit signature");
}

#[test]
fn integrated_backend_commits_without_signature_when_private_key_is_locked() {
    let env = SystemBackendTestEnv::new();
    let bytes = protected_cert_bytes("Locked Signer <locked-flatpak@example.com>");
    let imported =
        import_ripasso_private_key_bytes(&bytes, Some("hunter2")).expect("import private key");
    Preferences::new()
        .set_ripasso_own_fingerprint(Some(&imported.fingerprint))
        .expect("select signing key");
    clear_cached_unlocked_ripasso_private_keys();
    env.init_store_git_repository()
        .expect("initialize git repository");
    let store_root = env.store_root().to_string_lossy().to_string();

    save_store_recipients(
        &store_root,
        std::slice::from_ref(&imported.fingerprint),
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save store recipients");
    save_password_entry(
        &store_root,
        "team/service",
        "secret-value\nusername: alice",
        true,
    )
    .expect("save password entry");

    let subjects = env
        .store_git_commit_subjects()
        .expect("read commit subjects");
    assert_eq!(subjects.len(), 2);
    assert_eq!(
        env.store_git_head_author().expect("read head author"),
        "Locked Signer <locked-flatpak@example.com>"
    );
    assert!(!env
        .store_head_commit_has_signature()
        .expect("inspect commit headers"));
}

#[test]
fn unreadable_entry_rename_commits_without_a_signature() {
    let env = SystemBackendTestEnv::new();
    let bytes_a = protected_cert_bytes("Entry Key <entry-unreadable@example.com>");
    let bytes_b = protected_cert_bytes("Missing Key <missing-unreadable@example.com>");
    let imported_a = import_ripasso_private_key_bytes(&bytes_a, Some("hunter2"))
        .expect("import first private key");
    let imported_b = import_ripasso_private_key_bytes(&bytes_b, Some("hunter2"))
        .expect("import second private key");
    Preferences::new()
        .set_ripasso_own_fingerprint(Some(&imported_a.fingerprint))
        .expect("select signing key");
    env.init_store_git_repository()
        .expect("initialize git repository");
    let store_root = env.store_root().to_string_lossy().to_string();

    save_store_recipients(
        &store_root,
        &[
            imported_a.fingerprint.clone(),
            imported_b.fingerprint.clone(),
        ],
        StoreRecipientsPrivateKeyRequirement::AllManagedKeys,
    )
    .expect("save store recipients");
    save_password_entry(
        &store_root,
        "team/service",
        "secret-value\nusername: alice",
        true,
    )
    .expect("save password entry");
    remove_ripasso_private_key(&imported_b.fingerprint).expect("remove second key");

    rename_password_entry(&store_root, "team/service", "team/renamed")
        .expect("rename unreadable entry");

    let subjects = env
        .store_git_commit_subjects()
        .expect("read commit subjects");
    assert_eq!(
        subjects[0],
        "Rename password from team/service to team/renamed"
    );
    assert_eq!(
        env.store_git_head_author().expect("read head author"),
        "Keycord <git@keycord.invalid>"
    );
    assert!(!env
        .store_head_commit_has_signature()
        .expect("inspect commit headers"));
}

#[test]
fn unreadable_entry_delete_commits_without_a_signature() {
    let env = SystemBackendTestEnv::new();
    let bytes_a = protected_cert_bytes("Entry Key <entry-delete@example.com>");
    let bytes_b = protected_cert_bytes("Missing Key <missing-delete@example.com>");
    let imported_a = import_ripasso_private_key_bytes(&bytes_a, Some("hunter2"))
        .expect("import first private key");
    let imported_b = import_ripasso_private_key_bytes(&bytes_b, Some("hunter2"))
        .expect("import second private key");
    Preferences::new()
        .set_ripasso_own_fingerprint(Some(&imported_a.fingerprint))
        .expect("select signing key");
    env.init_store_git_repository()
        .expect("initialize git repository");
    let store_root = env.store_root().to_string_lossy().to_string();

    save_store_recipients(
        &store_root,
        &[
            imported_a.fingerprint.clone(),
            imported_b.fingerprint.clone(),
        ],
        StoreRecipientsPrivateKeyRequirement::AllManagedKeys,
    )
    .expect("save store recipients");
    save_password_entry(
        &store_root,
        "team/service",
        "secret-value\nusername: alice",
        true,
    )
    .expect("save password entry");
    remove_ripasso_private_key(&imported_b.fingerprint).expect("remove second key");

    delete_password_entry(&store_root, "team/service").expect("delete unreadable entry");

    let subjects = env
        .store_git_commit_subjects()
        .expect("read commit subjects");
    assert_eq!(subjects[0], "Remove password for team/service");
    assert_eq!(
        env.store_git_head_author().expect("read head author"),
        "Keycord <git@keycord.invalid>"
    );
    assert!(!env
        .store_head_commit_has_signature()
        .expect("inspect commit headers"));
}

#[test]
fn integrated_backend_saves_entries_with_empty_password_lines() {
    let env = SystemBackendTestEnv::new();
    let (cert, bytes) = protected_cert("Empty Password <empty-password@example.com>");
    let imported =
        import_ripasso_private_key_bytes(&bytes, Some("hunter2")).expect("import private key");

    let mut public_bytes = Vec::new();
    cert.serialize(&mut public_bytes)
        .expect("serialize public certificate");
    SystemBackendTestEnv::import_public_key(&public_bytes).expect("import public key");
    SystemBackendTestEnv::trust_public_key(&imported.fingerprint).expect("trust public key");

    let store_root = env.store_root().to_string_lossy().to_string();
    save_store_recipients(
        &store_root,
        std::slice::from_ref(&imported.fingerprint),
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save store recipients");
    save_password_entry(
        &store_root,
        "team/empty-password",
        "\nusername: alice",
        true,
    )
    .expect("save password entry with empty first line");

    assert_eq!(
        read_password_entry(&store_root, "team/empty-password").expect("read saved entry"),
        "\nusername: alice"
    );
}

#[test]
fn integrated_backend_save_leaves_git_worktree_clean() {
    let env = SystemBackendTestEnv::new();
    let (cert, bytes) = protected_cert("Git Clean <git-clean@example.com>");
    let imported =
        import_ripasso_private_key_bytes(&bytes, Some("hunter2")).expect("import private key");

    let mut public_bytes = Vec::new();
    cert.serialize(&mut public_bytes)
        .expect("serialize public certificate");
    SystemBackendTestEnv::import_public_key(&public_bytes).expect("import public key");
    SystemBackendTestEnv::trust_public_key(&imported.fingerprint).expect("trust public key");

    env.init_store_git_repository()
        .expect("initialize git repository");
    let store_root = env.store_root().to_string_lossy().to_string();
    save_store_recipients(
        &store_root,
        std::slice::from_ref(&imported.fingerprint),
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save store recipients");
    save_password_entry(&store_root, "example/user", "secret\nusername: alice", true)
        .expect("save password entry");

    assert_eq!(
        env.store_git_status_porcelain()
            .expect("read store git status after integrated save"),
        ""
    );
}

#[test]
fn git_commit_unlock_helper_detects_a_locked_entry_signing_key() {
    let env = SystemBackendTestEnv::new();
    let bytes = protected_cert_bytes("Locked Signer <locked-entry@example.com>");
    let imported =
        import_ripasso_private_key_bytes(&bytes, Some("hunter2")).expect("import private key");
    env.init_store_git_repository()
        .expect("initialize git repository");
    let store_root = env.store_root().to_string_lossy().to_string();

    save_store_recipients(
        &store_root,
        std::slice::from_ref(&imported.fingerprint),
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save store recipients");
    save_password_entry(
        &store_root,
        "team/service",
        "secret-value\nusername: alice",
        true,
    )
    .expect("save password entry");
    clear_cached_unlocked_ripasso_private_keys();

    assert_eq!(
        git_commit_private_key_requiring_unlock_for_entry(&store_root, "team/service",)
            .expect("resolve locked signing key"),
        Some(imported.fingerprint)
    );
}

#[test]
fn git_commit_unlock_helper_detects_a_locked_recipients_signing_key() {
    let env = SystemBackendTestEnv::new();
    let bytes = protected_cert_bytes("Locked Signer <locked-store@example.com>");
    let imported =
        import_ripasso_private_key_bytes(&bytes, Some("hunter2")).expect("import private key");
    env.init_store_git_repository()
        .expect("initialize git repository");
    clear_cached_unlocked_ripasso_private_keys();
    let store_root = env.store_root().to_string_lossy().to_string();

    assert_eq!(
        git_commit_private_key_requiring_unlock_for_store_recipients(
            &store_root,
            std::slice::from_ref(&imported.fingerprint),
            StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
        )
        .expect("resolve locked signing key"),
        Some(imported.fingerprint)
    );
}

#[test]
fn store_recipients_unlock_helper_detects_a_locked_entry_key() {
    let env = SystemBackendTestEnv::new();
    let bytes = protected_cert_bytes("Locked Store Entry <locked-entry@example.com>");
    let imported =
        import_ripasso_private_key_bytes(&bytes, Some("hunter2")).expect("import private key");
    let store_root = env.store_root().to_string_lossy().to_string();

    save_store_recipients(
        &store_root,
        std::slice::from_ref(&imported.fingerprint),
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    )
    .expect("save store recipients");
    save_password_entry(&store_root, "team/service", "secret\nusername: alice", true)
        .expect("save password entry");
    clear_cached_unlocked_ripasso_private_keys();

    assert_eq!(
        store_recipients_private_key_requiring_unlock(&store_root)
            .expect("resolve locked entry key"),
        Some(imported.fingerprint)
    );
}
