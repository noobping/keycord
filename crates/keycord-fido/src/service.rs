use crate::cache::{FidoCaches, DEFAULT_SECRET_CACHE_IDLE_TIMEOUT};
use crate::crypto::{
    decode_base64, decrypt_aes_256_gcm, derive_kek, encrypt_aes_256_gcm, random_bytes,
};
use crate::envelope::{
    parse, required_layer_aad, serialize, FidoBinding, FidoBindingDescriptor, RequiredLayerEnvelope,
};
use crate::{FidoAssertion, FidoDeviceLabel, FidoError, FidoTransport, FidoTransportError};
use sha2::{Digest, Sha256};
use std::sync::Arc;
#[cfg(feature = "native-transport")]
use std::sync::{OnceLock, RwLock};
use std::thread;
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

pub const FIDO_RP_ID: &str = "io.github.noobping.keycord";
const FIDO_HMAC_SALT_LEN: usize = 32;
const PAYLOAD_NONCE_LEN: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetryPolicy {
    pub matching_key_window: Duration,
    pub retry_interval: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            matching_key_window: Duration::from_secs(4),
            retry_interval: Duration::from_millis(150),
        }
    }
}

/// Stateful FIDO application service.
///
/// Clones share their transport and short-lived secret caches. Production
/// native workflows use the FIDO-owned `shared_native_service`; explicit
/// instances remain useful for tests and alternate transports.
#[derive(Clone)]
pub struct FidoService {
    transport: Arc<dyn FidoTransport>,
    caches: Arc<FidoCaches>,
    retry_policy: RetryPolicy,
}

#[cfg(feature = "native-transport")]
fn shared_native_service_cell() -> &'static RwLock<FidoService> {
    static SERVICE: OnceLock<RwLock<FidoService>> = OnceLock::new();
    SERVICE.get_or_init(|| RwLock::new(FidoService::native()))
}

/// Process-wide native service whose clones share FIDO caches and transport.
#[cfg(feature = "native-transport")]
pub fn shared_native_service() -> FidoService {
    shared_native_service_cell()
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

#[cfg(all(feature = "native-transport", feature = "test-support"))]
pub fn set_shared_native_transport_for_tests(transport: Arc<dyn FidoTransport>) {
    *shared_native_service_cell()
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = FidoService::new(transport);
}

#[cfg(all(feature = "native-transport", feature = "test-support"))]
pub fn reset_shared_native_transport_for_tests() {
    *shared_native_service_cell()
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = FidoService::native();
}

impl FidoService {
    pub fn new(transport: Arc<dyn FidoTransport>) -> Self {
        Self {
            transport,
            caches: Arc::new(FidoCaches::new(DEFAULT_SECRET_CACHE_IDLE_TIMEOUT)),
            retry_policy: RetryPolicy::default(),
        }
    }

    #[cfg(feature = "native-transport")]
    pub fn native() -> Self {
        Self::new(Arc::new(crate::NativeFidoTransport))
    }

    pub fn with_retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.retry_policy = retry_policy;
        self
    }

    pub const fn supports_pin_setup() -> bool {
        cfg!(all(target_os = "linux", feature = "pin-setup"))
    }

    pub fn create_binding(&self, pin: Option<&str>) -> Result<FidoBindingDescriptor, FidoError> {
        let enrollment_salt = random_bytes::<FIDO_HMAC_SALT_LEN>();
        let enrollment = self.transport.enroll_hmac_secret(
            FIDO_RP_ID,
            "keycord-fido2-private-key",
            "Keycord FIDO2-protected private key",
            pin,
            &enrollment_salt,
        )?;
        let fingerprint = binding_fingerprint(&enrollment.credential_id);
        let label = binding_label(&enrollment.device);
        self.caches.cache_enrollment(
            &fingerprint,
            &enrollment.credential_id,
            &enrollment_salt,
            &enrollment.hmac_secret,
        )?;
        if let Some(pin) = pin {
            self.caches.cache_pin(&fingerprint, pin.as_bytes())?;
        }
        Ok(FidoBindingDescriptor {
            fingerprint,
            label,
            credential_id: enrollment.credential_id,
        })
    }

    pub fn encrypt_required_layer(
        &self,
        binding: &FidoBinding,
        payload: &[u8],
    ) -> Result<Vec<u8>, FidoError> {
        let (hmac_salt, hmac_secret) = self.hmac_material_for_binding(binding)?;
        let kek = derive_kek(&hmac_secret, &binding.fingerprint, &hmac_salt)?;
        let payload_nonce = random_bytes::<PAYLOAD_NONCE_LEN>();
        let payload_ciphertext = encrypt_aes_256_gcm(
            &kek,
            &payload_nonce,
            &required_layer_aad(&binding.fingerprint),
            payload,
        )?;
        serialize(&RequiredLayerEnvelope::new(
            binding,
            &hmac_salt,
            &payload_nonce,
            &payload_ciphertext,
        ))
    }

    pub fn unlock_required_layer(
        &self,
        ciphertext: &[u8],
        pin: Option<&str>,
    ) -> Result<Vec<u8>, FidoError> {
        let envelope = parse(ciphertext)?
            .ok_or_else(|| FidoError::invalid("That FIDO2-protected key data is invalid."))?;
        envelope.validate()?;

        let supplied_pin = match pin {
            Some(pin) => {
                let trimmed = pin.trim();
                if trimmed.is_empty() {
                    return Err(FidoError::PinRequired);
                }
                Some(Zeroizing::new(trimmed.as_bytes().to_vec()))
            }
            None => None,
        };
        let cached_pin = if supplied_pin.is_none() {
            self.caches.borrow_pin(&envelope.fingerprint)?
        } else {
            None
        };
        let resolved_pin = supplied_pin
            .as_ref()
            .map(|pin| pin.as_slice())
            .or_else(|| cached_pin.as_ref().map(|pin| pin.as_slice()));
        let resolved_pin = resolved_pin
            .map(std::str::from_utf8)
            .transpose()
            .map_err(|error| {
                FidoError::invalid(format!("Stored FIDO2 PIN is not valid UTF-8: {error}"))
            })?;

        let hmac_salt = decode_base64(&envelope.hmac_salt)?;
        let credential_id = decode_base64(&envelope.credential_id)?;
        let payload_nonce = decode_base64(&envelope.payload_nonce)?;
        let payload_ciphertext = decode_base64(&envelope.payload_ciphertext)?;
        let mut excluded_devices = Vec::new();

        loop {
            let assertion = self.derive_with_retry(
                &envelope.rp_id,
                &credential_id,
                &hmac_salt,
                &excluded_devices,
                resolved_pin,
            )?;
            let kek = derive_kek(&assertion.hmac_secret, &envelope.fingerprint, &hmac_salt)?;
            match decrypt_aes_256_gcm(
                &kek,
                &payload_nonce,
                &required_layer_aad(&envelope.fingerprint),
                &payload_ciphertext,
            ) {
                Ok(plaintext) => {
                    if let Some(pin) = resolved_pin {
                        self.caches
                            .cache_pin(&envelope.fingerprint, pin.as_bytes())?;
                    }
                    return Ok(plaintext);
                }
                Err(error) if assertion.device.is_some() => {
                    let device = assertion.device.expect("device checked above");
                    if excluded_devices.iter().any(|excluded| excluded == &device) {
                        return Err(error);
                    }
                    excluded_devices.push(device);
                }
                Err(error) => return Err(error),
            }
        }
    }

    pub fn set_new_pin(&self, new_pin: &str) -> Result<(), FidoError> {
        let trimmed = new_pin.trim();
        if trimmed.is_empty() {
            return Err(FidoError::PinRequired);
        }

        #[cfg(all(target_os = "linux", feature = "pin-setup"))]
        {
            self.transport.set_new_pin(trimmed).map_err(FidoError::from)
        }

        #[cfg(not(all(target_os = "linux", feature = "pin-setup")))]
        {
            let _ = trimmed;
            Err(FidoError::PinSetupUnavailable)
        }
    }

    pub fn remove_cached_secrets(&self, fingerprint: &str) -> Result<(), FidoError> {
        self.caches.remove(fingerprint)
    }

    pub fn clear_cached_secrets(&self) {
        self.caches.clear();
    }

    fn hmac_material_for_binding(
        &self,
        binding: &FidoBinding,
    ) -> Result<(Vec<u8>, Vec<u8>), FidoError> {
        if let Some(enrollment) = self
            .caches
            .borrow_enrollment(&binding.fingerprint)?
            .filter(|enrollment| enrollment.matches_credential_id(&binding.credential_id))
        {
            return Ok((
                enrollment.hmac_salt().to_vec(),
                enrollment.hmac_secret().to_vec(),
            ));
        }

        let hmac_salt = random_bytes::<FIDO_HMAC_SALT_LEN>().to_vec();
        let cached_pin = self.caches.borrow_pin(&binding.fingerprint)?;
        let pin = cached_pin
            .as_ref()
            .map(|pin| std::str::from_utf8(pin.as_slice()))
            .transpose()
            .map_err(|error| {
                FidoError::invalid(format!("Stored FIDO2 PIN is not valid UTF-8: {error}"))
            })?;
        let assertion =
            self.derive_with_retry(&binding.rp_id, &binding.credential_id, &hmac_salt, &[], pin)?;
        Ok((hmac_salt, assertion.hmac_secret))
    }

    fn derive_with_retry(
        &self,
        rp_id: &str,
        credential_id: &[u8],
        salt: &[u8],
        excluded_devices: &[FidoDeviceLabel],
        pin: Option<&str>,
    ) -> Result<FidoAssertion, FidoError> {
        let deadline = Instant::now() + self.retry_policy.matching_key_window;
        loop {
            match self.transport.derive_hmac_secret(
                rp_id,
                credential_id,
                pin,
                salt,
                excluded_devices,
            ) {
                Ok(assertion) => return Ok(assertion),
                Err(error) if should_retry(&error) && Instant::now() < deadline => {
                    thread::sleep(self.retry_policy.retry_interval);
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
}

#[cfg(feature = "native-transport")]
impl Default for FidoService {
    fn default() -> Self {
        Self::native()
    }
}

fn should_retry(error: &FidoTransportError) -> bool {
    matches!(
        error,
        FidoTransportError::TokenNotPresent
            | FidoTransportError::UserActionTimeout
            | FidoTransportError::TokenRemoved
    )
}

fn binding_fingerprint(credential_id: &[u8]) -> String {
    let digest = Sha256::digest(credential_id);
    digest[..20]
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect()
}

fn binding_label(device: &FidoDeviceLabel) -> String {
    match (device.manufacturer.as_deref(), device.product.as_deref()) {
        (Some(manufacturer), Some(product))
            if !manufacturer.trim().is_empty() && !product.trim().is_empty() =>
        {
            format!("{manufacturer} {product}")
        }
        (_, Some(product)) if !product.trim().is_empty() => product.to_string(),
        (Some(manufacturer), _) if !manufacturer.trim().is_empty() => manufacturer.to_string(),
        _ => "FIDO2 security key".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{FidoService, RetryPolicy};
    use crate::{
        FidoAssertion, FidoDeviceLabel, FidoEnrollment, FidoErrorKind, FidoTransport,
        FidoTransportError,
    };
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    struct FakeTransport {
        enrollments: Mutex<VecDeque<Result<FidoEnrollment, FidoTransportError>>>,
        assertions: Mutex<VecDeque<Result<FidoAssertion, FidoTransportError>>>,
    }

    impl FakeTransport {
        fn new(
            enrollments: impl IntoIterator<Item = Result<FidoEnrollment, FidoTransportError>>,
            assertions: impl IntoIterator<Item = Result<FidoAssertion, FidoTransportError>>,
        ) -> Self {
            Self {
                enrollments: Mutex::new(enrollments.into_iter().collect()),
                assertions: Mutex::new(assertions.into_iter().collect()),
            }
        }
    }

    impl FidoTransport for FakeTransport {
        fn enroll_hmac_secret(
            &self,
            _rp_id: &str,
            _user_name: &str,
            _user_display_name: &str,
            _pin: Option<&str>,
            _salt: &[u8],
        ) -> Result<FidoEnrollment, FidoTransportError> {
            self.enrollments
                .lock()
                .unwrap()
                .pop_front()
                .expect("fake enrollment")
        }

        fn derive_hmac_secret(
            &self,
            _rp_id: &str,
            _credential_id: &[u8],
            _pin: Option<&str>,
            _salt: &[u8],
            _excluded_devices: &[FidoDeviceLabel],
        ) -> Result<FidoAssertion, FidoTransportError> {
            self.assertions
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Err(FidoTransportError::TokenNotPresent))
        }
    }

    fn device() -> FidoDeviceLabel {
        FidoDeviceLabel {
            manufacturer: Some("Example".into()),
            product: Some("Security Key".into()),
            vendor_id: Some(1),
            product_id: Some(2),
        }
    }

    fn no_retry(service: FidoService) -> FidoService {
        service.with_retry_policy(RetryPolicy {
            matching_key_window: Duration::ZERO,
            retry_interval: Duration::ZERO,
        })
    }

    #[test]
    fn injected_transports_encrypt_and_unlock_existing_required_layer_format() {
        let secret = b"shared hmac secret".to_vec();
        let creator = no_retry(FidoService::new(Arc::new(FakeTransport::new(
            [Ok(FidoEnrollment {
                credential_id: b"credential-id".to_vec(),
                device: device(),
                hmac_secret: secret.clone(),
            })],
            [],
        ))));
        let descriptor = creator.create_binding(Some("123456")).unwrap();
        assert_eq!(descriptor.label, "Example Security Key");
        let encrypted = creator
            .encrypt_required_layer(&descriptor.binding(), b"private key bytes")
            .unwrap();

        let unlocker = no_retry(FidoService::new(Arc::new(FakeTransport::new(
            [],
            [Ok(FidoAssertion {
                hmac_secret: secret,
                device: Some(device()),
            })],
        ))));
        assert_eq!(
            unlocker
                .unlock_required_layer(&encrypted, Some("123456"))
                .unwrap(),
            b"private key bytes"
        );
    }

    #[test]
    fn transport_failures_are_exposed_as_typed_domain_errors() {
        let service = no_retry(FidoService::new(Arc::new(FakeTransport::new(
            [Err(FidoTransportError::PinNotSet)],
            [],
        ))));
        let error = service.create_binding(None).unwrap_err();
        assert_eq!(error.kind(), FidoErrorKind::PinNotSet);
        assert_eq!(
            error.to_string(),
            "Set a PIN on the FIDO2 security key first."
        );
    }

    #[test]
    fn cloned_services_share_and_clear_secret_caches() {
        let service = no_retry(FidoService::new(Arc::new(FakeTransport::new(
            [Ok(FidoEnrollment {
                credential_id: b"credential-id".to_vec(),
                device: device(),
                hmac_secret: b"secret".to_vec(),
            })],
            [],
        ))));
        let binding = service.create_binding(Some("123456")).unwrap().binding();
        let clone = service.clone();
        clone.clear_cached_secrets();
        assert!(service
            .encrypt_required_layer(&binding, b"payload")
            .is_err());
    }

    #[cfg(all(target_os = "linux", feature = "pin-setup"))]
    #[test]
    fn pin_setup_uses_the_injected_transport() {
        use std::sync::atomic::{AtomicBool, Ordering};

        struct PinTransport(AtomicBool);

        impl FidoTransport for PinTransport {
            fn enroll_hmac_secret(
                &self,
                _rp_id: &str,
                _user_name: &str,
                _user_display_name: &str,
                _pin: Option<&str>,
                _salt: &[u8],
            ) -> Result<FidoEnrollment, FidoTransportError> {
                Err(FidoTransportError::Unsupported)
            }

            fn derive_hmac_secret(
                &self,
                _rp_id: &str,
                _credential_id: &[u8],
                _pin: Option<&str>,
                _salt: &[u8],
                _excluded_devices: &[FidoDeviceLabel],
            ) -> Result<FidoAssertion, FidoTransportError> {
                Err(FidoTransportError::Unsupported)
            }

            fn set_new_pin(&self, new_pin: &str) -> Result<(), FidoTransportError> {
                assert_eq!(new_pin, "123456");
                self.0.store(true, Ordering::SeqCst);
                Ok(())
            }
        }

        let transport = Arc::new(PinTransport(AtomicBool::new(false)));
        FidoService::new(transport.clone())
            .set_new_pin(" 123456 ")
            .unwrap();
        assert!(transport.0.load(Ordering::SeqCst));
    }
}
