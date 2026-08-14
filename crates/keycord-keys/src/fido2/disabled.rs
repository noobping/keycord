use crate::PrivateKeyError;

const FIDO2_FEATURE_DISABLED_MESSAGE: &str = "FIDO2 support is disabled in this build of Keycord.";

pub fn set_fido2_security_key_pin(_new_pin: &str) -> Result<(), PrivateKeyError> {
    Err(PrivateKeyError::unsupported_fido2_key(
        FIDO2_FEATURE_DISABLED_MESSAGE,
    ))
}
