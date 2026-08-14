use crate::PrivateKeyError;
use keycord_fido::{FidoBinding, FidoBindingDescriptor, FidoError, FidoErrorKind, FidoService};

fn service() -> FidoService {
    keycord_fido::shared_native_service()
}

fn private_key_error_from_fido_error(error: FidoError) -> PrivateKeyError {
    let kind = error.kind();
    let message = error.to_string();
    match kind {
        FidoErrorKind::PinNotSet => PrivateKeyError::fido2_pin_not_set(message),
        FidoErrorKind::PinRequired => PrivateKeyError::fido2_pin_required(message),
        FidoErrorKind::IncorrectPin => PrivateKeyError::incorrect_fido2_pin(message),
        FidoErrorKind::PinUnsupported => PrivateKeyError::fido2_pin_unsupported(message),
        FidoErrorKind::TokenNotPresent => PrivateKeyError::fido2_token_not_present(message),
        FidoErrorKind::UserActionTimeout => PrivateKeyError::fido2_user_action_timeout(message),
        FidoErrorKind::TokenRemoved => PrivateKeyError::fido2_token_removed(message),
        FidoErrorKind::Unsupported => PrivateKeyError::unsupported_fido2_key(message),
        FidoErrorKind::InvalidData | FidoErrorKind::Crypto | FidoErrorKind::Other => {
            PrivateKeyError::other(message)
        }
    }
}

pub(crate) fn create_fido2_private_key_binding(
    pin: Option<&str>,
) -> Result<FidoBindingDescriptor, PrivateKeyError> {
    service()
        .create_binding(pin)
        .map_err(private_key_error_from_fido_error)
}

pub(crate) fn encrypt_fido2_direct_required_layer(
    binding: &FidoBinding,
    payload: &[u8],
) -> Result<Vec<u8>, String> {
    service()
        .encrypt_required_layer(binding, payload)
        .map_err(|error| error.to_string())
}

pub(crate) fn unlock_fido2_private_key_material_for_session(
    ciphertext: &[u8],
    pin: Option<&str>,
) -> Result<Vec<u8>, PrivateKeyError> {
    service()
        .unlock_required_layer(ciphertext, pin)
        .map_err(private_key_error_from_fido_error)
}

pub fn set_fido2_security_key_pin(new_pin: &str) -> Result<(), PrivateKeyError> {
    service()
        .set_new_pin(new_pin)
        .map_err(private_key_error_from_fido_error)
}

pub(crate) fn remove_cached_fido2_secrets(fingerprint: &str) -> Result<(), String> {
    service()
        .remove_cached_secrets(fingerprint)
        .map_err(|error| error.to_string())
}

pub(crate) fn clear_cached_fido2_secrets() {
    service().clear_cached_secrets();
}
