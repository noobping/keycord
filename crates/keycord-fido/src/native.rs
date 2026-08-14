use crate::crypto::random_bytes;
use crate::{FidoAssertion, FidoDeviceLabel, FidoEnrollment, FidoTransport, FidoTransportError};
#[cfg(any(target_os = "linux", target_os = "windows"))]
use fido2_rs::{
    assertion::AssertRequest,
    credentials::{CoseType, Credential, Extensions, Opt},
    device::{Device, DeviceInfo, DeviceList},
    error::Error as FidoLibraryError,
};
#[cfg(all(target_os = "linux", feature = "pin-setup"))]
use libfido2_sys as ffi;
#[cfg(any(target_os = "linux", target_os = "windows"))]
use sha2::{Digest, Sha256};
#[cfg(all(target_os = "linux", feature = "pin-setup"))]
use std::ffi::{CStr, CString};
#[cfg(all(target_os = "linux", feature = "pin-setup"))]
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, Default)]
pub struct NativeFidoTransport;

#[cfg(any(target_os = "linux", target_os = "windows"))]
const CLIENT_DATA_HASH_LEN: usize = 32;
#[cfg(any(target_os = "linux", target_os = "windows"))]
const USER_ID_LEN: usize = 32;

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn enroll_with_discoverable_fallback(
    mut enroll: impl FnMut(bool) -> Result<FidoEnrollment, FidoTransportError>,
) -> Result<FidoEnrollment, FidoTransportError> {
    match enroll(true) {
        Ok(enrollment) => Ok(enrollment),
        Err(FidoTransportError::Unsupported) => enroll(false),
        Err(error) => Err(error),
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn map_library_error(error: FidoLibraryError) -> FidoTransportError {
    map_error_message(&error.to_string())
}

fn map_error_message(message: &str) -> FidoTransportError {
    let normalized = message.to_ascii_lowercase().replace('_', " ");
    if normalized.contains("pin not set") {
        FidoTransportError::PinNotSet
    } else if normalized.contains("pin required") || normalized.contains("uv invalid") {
        FidoTransportError::PinRequired
    } else if normalized.contains("pin invalid")
        || normalized.contains("pin auth invalid")
        || normalized.contains("pin auth blocked")
    {
        FidoTransportError::IncorrectPin
    } else if normalized.contains("no credentials")
        || normalized.contains("not found")
        || normalized.contains("open")
        || normalized.contains("device not found")
    {
        FidoTransportError::TokenNotPresent
    } else if normalized.contains("unsupported") || normalized.contains("invalid option") {
        FidoTransportError::Unsupported
    } else if normalized.contains("action timeout") || normalized.contains("operation denied") {
        FidoTransportError::UserActionTimeout
    } else if normalized.contains("rx")
        || normalized.contains("keepalive")
        || normalized.contains("removed")
        || normalized.contains("cancelled")
    {
        FidoTransportError::TokenRemoved
    } else {
        FidoTransportError::Other(message.to_string())
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn transport_error_rank(error: &FidoTransportError) -> usize {
    match error {
        FidoTransportError::PinNotSet => 0,
        FidoTransportError::PinRequired => 1,
        FidoTransportError::IncorrectPin => 2,
        FidoTransportError::PinUnsupported => 3,
        FidoTransportError::UserActionTimeout => 4,
        FidoTransportError::TokenRemoved => 5,
        FidoTransportError::Unsupported => 6,
        FidoTransportError::Other(_) => 7,
        FidoTransportError::TokenNotPresent => 8,
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn prefer_transport_error(
    current: Option<FidoTransportError>,
    candidate: FidoTransportError,
) -> Option<FidoTransportError> {
    match current {
        None => Some(candidate),
        Some(current) if transport_error_rank(&candidate) < transport_error_rank(&current) => {
            Some(candidate)
        }
        Some(current) => Some(current),
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn select_matching_hmac_secret<'a>(
    assertions: impl IntoIterator<Item = (&'a [u8], &'a [u8])>,
    assertion_count: usize,
    credential_id: &[u8],
) -> Result<Vec<u8>, FidoTransportError> {
    let mut unnamed_secret = None;
    for (assertion_id, secret) in assertions {
        if assertion_id == credential_id {
            return if secret.is_empty() {
                Err(FidoTransportError::Unsupported)
            } else {
                Ok(secret.to_vec())
            };
        }
        if assertion_count == 1 && assertion_id.is_empty() {
            unnamed_secret = Some(secret.to_vec());
        }
    }

    match unnamed_secret {
        Some(secret) if secret.is_empty() => Err(FidoTransportError::Unsupported),
        Some(secret) => Ok(secret),
        None => Err(FidoTransportError::TokenNotPresent),
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn client_data_hash(label: &str) -> [u8; CLIENT_DATA_HASH_LEN] {
    let mut hasher = Sha256::new();
    hasher.update(random_bytes::<CLIENT_DATA_HASH_LEN>());
    hasher.update(label.as_bytes());
    let digest = hasher.finalize();
    let mut hash = [0u8; CLIENT_DATA_HASH_LEN];
    hash.copy_from_slice(&digest);
    hash
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn client_data(label: &str) -> Vec<u8> {
    let mut data = random_bytes::<CLIENT_DATA_HASH_LEN>().to_vec();
    data.extend_from_slice(label.as_bytes());
    data
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn set_assert_client_data(
    device: &Device,
    request: &mut AssertRequest,
    label: &str,
) -> Result<(), FidoTransportError> {
    if device.is_winhello() {
        request
            .set_client_data(client_data(label))
            .map_err(map_library_error)
    } else {
        request
            .set_client_data_hash(client_data_hash(label))
            .map_err(map_library_error)
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn set_credential_client_data(
    device: &Device,
    credential: &mut Credential,
    label: &str,
) -> Result<(), FidoTransportError> {
    if device.is_winhello() {
        credential
            .set_client_data(client_data(label))
            .map_err(map_library_error)
    } else {
        credential
            .set_client_data_hash(client_data_hash(label))
            .map_err(map_library_error)
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn user_id() -> [u8; USER_ID_LEN] {
    random_bytes::<USER_ID_LEN>()
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn owned_device_label(info: DeviceInfo<'_>) -> FidoDeviceLabel {
    FidoDeviceLabel {
        manufacturer: Some(info.manufacturer.to_string_lossy().into_owned())
            .filter(|value| !value.trim().is_empty()),
        product: Some(info.product.to_string_lossy().into_owned())
            .filter(|value| !value.trim().is_empty()),
        vendor_id: u16::try_from(info.vendor_id).ok(),
        product_id: u16::try_from(info.product_id).ok(),
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn ensure_device_pin_is_ready(device: &Device) -> Result<(), FidoTransportError> {
    if !device.supports_pin() {
        return Err(FidoTransportError::PinUnsupported);
    }
    if !device.has_pin() {
        return Err(FidoTransportError::PinNotSet);
    }
    Ok(())
}

#[cfg(all(target_os = "linux", feature = "pin-setup"))]
fn libfido_error(code: i32) -> FidoTransportError {
    let message = unsafe {
        let pointer = ffi::fido_strerr(code);
        CStr::from_ptr(pointer).to_string_lossy().into_owned()
    };
    map_error_message(&message)
}

#[cfg(all(target_os = "linux", feature = "pin-setup"))]
fn set_pin_on_device_path(device_path: &str, new_pin: &str) -> Result<(), FidoTransportError> {
    let device_path = CString::new(device_path).map_err(|error| {
        FidoTransportError::Other(format!("Invalid FIDO2 device path: {error}"))
    })?;
    let mut pin = Zeroizing::new(new_pin.as_bytes().to_vec());
    if pin.contains(&0) {
        return Err(FidoTransportError::Other(
            "The FIDO2 security key PIN contains an unsupported NUL byte.".to_string(),
        ));
    }
    pin.push(0);

    let mut device = unsafe { ffi::fido_dev_new() };
    if device.is_null() {
        return Err(FidoTransportError::Other(
            "Couldn't initialize the FIDO2 security key.".to_string(),
        ));
    }

    let open_result = unsafe { ffi::fido_dev_open(device, device_path.as_ptr()) };
    if open_result != 0 {
        unsafe { ffi::fido_dev_free(&mut device) };
        return Err(libfido_error(open_result));
    }

    let set_pin_result =
        unsafe { ffi::fido_dev_set_pin(device, pin.as_ptr().cast(), std::ptr::null()) };
    unsafe {
        ffi::fido_dev_close(device);
        ffi::fido_dev_free(&mut device);
    }
    if set_pin_result == 0 {
        Ok(())
    } else {
        Err(libfido_error(set_pin_result))
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
struct EnrollmentRequest<'a> {
    label: &'a FidoDeviceLabel,
    rp_id: &'a str,
    user_name: &'a str,
    user_display_name: &'a str,
    pin: Option<&'a str>,
    salt: &'a [u8],
    discoverable: bool,
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
impl NativeFidoTransport {
    fn single_enrollment_device() -> Result<(Device, FidoDeviceLabel), FidoTransportError> {
        let mut devices = DeviceList::list_devices(16);
        let Some(info) = devices.next() else {
            return Err(FidoTransportError::TokenNotPresent);
        };
        if devices.next().is_some() {
            return Err(FidoTransportError::Other(
                "Connect only one FIDO2 security key before continuing.".to_string(),
            ));
        }
        let label = owned_device_label(info);
        let device = info.open().map_err(map_library_error)?;
        Ok((device, label))
    }

    fn hmac_secret_for_device(
        device: &Device,
        rp_id: &str,
        credential_id: &[u8],
        pin: Option<&str>,
        salt: &[u8],
    ) -> Result<Vec<u8>, FidoTransportError> {
        ensure_device_pin_is_ready(device)?;
        let mut request = AssertRequest::new();
        request.set_rp(rp_id).map_err(map_library_error)?;
        set_assert_client_data(device, &mut request, rp_id)?;
        request
            .set_allow_credential(credential_id)
            .map_err(map_library_error)?;
        request
            .set_extensions(Extensions::HMAC_SECRET)
            .map_err(map_library_error)?;
        request.set_hmac_salt(salt).map_err(map_library_error)?;
        request.set_uv(Opt::Omit).map_err(map_library_error)?;
        let assertions = device
            .get_assertion(request, pin)
            .map_err(map_library_error)?;
        let assertion_count = assertions.count();
        let candidates: Vec<(Vec<u8>, Vec<u8>)> = assertions
            .iter()
            .map(|assertion| (assertion.id().to_vec(), assertion.hmac_secret().to_vec()))
            .collect();
        select_matching_hmac_secret(
            candidates
                .iter()
                .map(|(assertion_id, secret)| (assertion_id.as_slice(), secret.as_slice())),
            assertion_count,
            credential_id,
        )
    }

    fn enroll_hmac_secret_on_device(
        device: &Device,
        request: EnrollmentRequest<'_>,
    ) -> Result<FidoEnrollment, FidoTransportError> {
        ensure_device_pin_is_ready(device)?;
        let mut credential = Credential::new();
        set_credential_client_data(device, &mut credential, request.user_name)?;
        credential
            .set_rp(request.rp_id, request.rp_id)
            .map_err(map_library_error)?;
        credential
            .set_user(
                user_id(),
                request.user_name,
                Some(request.user_display_name),
                Some(""),
            )
            .map_err(map_library_error)?;
        credential
            .set_extension(Extensions::HMAC_SECRET)
            .map_err(map_library_error)?;
        credential
            .set_rk(if request.discoverable {
                Opt::True
            } else {
                Opt::False
            })
            .map_err(map_library_error)?;
        credential.set_uv(Opt::Omit).map_err(map_library_error)?;
        credential
            .set_cose_type(CoseType::ES256)
            .map_err(map_library_error)?;
        device
            .make_credential(&mut credential, request.pin)
            .map_err(map_library_error)?;
        let credential_id = credential.id().to_vec();
        if credential_id.is_empty() {
            return Err(FidoTransportError::Other(
                "The FIDO2 security key did not return a credential identifier.".to_string(),
            ));
        }
        let hmac_secret = Self::hmac_secret_for_device(
            device,
            request.rp_id,
            &credential_id,
            request.pin,
            request.salt,
        )?;
        Ok(FidoEnrollment {
            credential_id,
            device: request.label.clone(),
            hmac_secret,
        })
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
impl FidoTransport for NativeFidoTransport {
    fn enroll_hmac_secret(
        &self,
        rp_id: &str,
        user_name: &str,
        user_display_name: &str,
        pin: Option<&str>,
        salt: &[u8],
    ) -> Result<FidoEnrollment, FidoTransportError> {
        let (device, label) = Self::single_enrollment_device()?;
        enroll_with_discoverable_fallback(|discoverable| {
            Self::enroll_hmac_secret_on_device(
                &device,
                EnrollmentRequest {
                    label: &label,
                    rp_id,
                    user_name,
                    user_display_name,
                    pin,
                    salt,
                    discoverable,
                },
            )
        })
    }

    fn derive_hmac_secret(
        &self,
        rp_id: &str,
        credential_id: &[u8],
        pin: Option<&str>,
        salt: &[u8],
        excluded_devices: &[FidoDeviceLabel],
    ) -> Result<FidoAssertion, FidoTransportError> {
        let mut last_error = None;
        let mut found_any_device = false;
        for info in DeviceList::list_devices(16) {
            found_any_device = true;
            let label = owned_device_label(info);
            if excluded_devices.iter().any(|excluded| excluded == &label) {
                continue;
            }
            let device = match info.open() {
                Ok(device) => device,
                Err(error) => {
                    last_error = prefer_transport_error(last_error, map_library_error(error));
                    continue;
                }
            };
            match Self::hmac_secret_for_device(&device, rp_id, credential_id, pin, salt) {
                Ok(hmac_secret) => {
                    return Ok(FidoAssertion {
                        hmac_secret,
                        device: Some(label),
                    });
                }
                Err(error) => last_error = prefer_transport_error(last_error, error),
            }
        }

        if !found_any_device {
            return Err(FidoTransportError::TokenNotPresent);
        }
        Err(last_error.unwrap_or(FidoTransportError::TokenNotPresent))
    }

    #[cfg(all(target_os = "linux", feature = "pin-setup"))]
    fn set_new_pin(&self, new_pin: &str) -> Result<(), FidoTransportError> {
        let mut devices = DeviceList::list_devices(16);
        let Some(info) = devices.next() else {
            return Err(FidoTransportError::TokenNotPresent);
        };
        if devices.next().is_some() {
            return Err(FidoTransportError::Other(
                "Connect only one FIDO2 security key before continuing.".to_string(),
            ));
        }
        {
            let device = info.open().map_err(map_library_error)?;
            if !device.supports_pin() {
                return Err(FidoTransportError::PinUnsupported);
            }
            if device.has_pin() {
                return Ok(());
            }
        }
        set_pin_on_device_path(&info.path.to_string_lossy(), new_pin)
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
impl FidoTransport for NativeFidoTransport {
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
}

#[cfg(test)]
mod tests {
    use super::map_error_message;
    use crate::FidoTransportError;

    #[test]
    fn error_mapping_preserves_pin_and_touch_guidance() {
        assert!(matches!(
            map_error_message("FIDO_ERR_PIN_NOT_SET"),
            FidoTransportError::PinNotSet
        ));
        assert!(matches!(
            map_error_message("FIDO_ERR_PIN_REQUIRED"),
            FidoTransportError::PinRequired
        ));
        assert!(matches!(
            map_error_message("FIDO_ERR_USER_ACTION_TIMEOUT"),
            FidoTransportError::UserActionTimeout
        ));
    }

    #[cfg(any(target_os = "linux", target_os = "windows"))]
    #[test]
    fn discoverable_enrollment_falls_back_only_for_unsupported_tokens() {
        let mut attempts = Vec::new();
        let enrollment = super::enroll_with_discoverable_fallback(|discoverable| {
            attempts.push(discoverable);
            if discoverable {
                Err(FidoTransportError::Unsupported)
            } else {
                Ok(crate::FidoEnrollment {
                    credential_id: b"credential".to_vec(),
                    device: crate::FidoDeviceLabel {
                        manufacturer: None,
                        product: None,
                        vendor_id: None,
                        product_id: None,
                    },
                    hmac_secret: b"secret".to_vec(),
                })
            }
        })
        .unwrap();
        assert_eq!(attempts, [true, false]);
        assert_eq!(enrollment.credential_id, b"credential");
    }

    #[cfg(any(target_os = "linux", target_os = "windows"))]
    #[test]
    fn matching_assertions_do_not_accept_a_different_credential() {
        let error = super::select_matching_hmac_secret(
            [(b"other".as_slice(), b"secret".as_slice())],
            1,
            b"expected",
        )
        .unwrap_err();
        assert!(matches!(error, FidoTransportError::TokenNotPresent));
    }
}
