use super::super::cache::borrow_pending_fido2_enrollment;
use super::common::{
    cached_pin_string, create_fido2_binding_descriptor, decode_base64, decrypt_aes_256_gcm,
    derive_direct_hmac_assertion_with_pin, derive_kek, encode_base64, encrypt_aes_256_gcm,
    parse_text_envelope, private_key_error_from_fido2_error, random_bytes, serialize_text_envelope,
    validate_direct_layer_envelope, Fido2DirectBinding, Fido2DirectBindingDescriptor,
    Fido2DirectLayerEnvelope, FIDO2_DIRECT_ENTRY_FORMAT, FIDO2_DIRECT_LAYER_AAD_PREFIX,
    FIDO2_DIRECT_LAYER_HEADER, FIDO2_DIRECT_LAYER_KIND, FIDO2_HMAC_SALT_LEN,
};
use crate::backend::PrivateKeyError;
use secrecy::ExposeSecret;

pub(in crate::backend::integrated) fn create_fido2_private_key_binding(
    pin: Option<&str>,
) -> Result<Fido2DirectBindingDescriptor, PrivateKeyError> {
    create_fido2_binding_descriptor(pin)
}

pub(in crate::backend::integrated) fn encrypt_fido2_direct_required_layer(
    binding: &Fido2DirectBinding,
    payload: &[u8],
) -> Result<Vec<u8>, String> {
    let (hmac_salt, hmac_secret) = direct_hmac_material_for_binding(binding)?;
    let kek = derive_kek(&hmac_secret, &binding.fingerprint, &hmac_salt)
        .map_err(|err| err.to_string())?;
    let payload_nonce = random_bytes::<12>();
    let payload_ciphertext = encrypt_aes_256_gcm(
        &kek,
        &payload_nonce,
        &direct_required_layer_aad(&binding.fingerprint),
        payload,
    )
    .map_err(|err| err.to_string())?;

    serialize_text_envelope(
        FIDO2_DIRECT_LAYER_HEADER,
        &Fido2DirectLayerEnvelope {
            format: FIDO2_DIRECT_ENTRY_FORMAT,
            protection: FIDO2_DIRECT_LAYER_KIND.to_string(),
            fingerprint: binding.fingerprint.clone(),
            rp_id: binding.rp_id.clone(),
            credential_id: encode_base64(&binding.credential_id),
            hmac_salt: encode_base64(&hmac_salt),
            payload_nonce: encode_base64(&payload_nonce),
            payload_ciphertext: encode_base64(&payload_ciphertext),
        },
    )
}

pub(in crate::backend::integrated) fn unlock_fido2_private_key_material_for_session(
    ciphertext: &[u8],
    pin: Option<&str>,
) -> Result<Vec<u8>, PrivateKeyError> {
    let Some(envelope) =
        parse_text_envelope::<Fido2DirectLayerEnvelope>(FIDO2_DIRECT_LAYER_HEADER, ciphertext)
            .map_err(PrivateKeyError::other)?
    else {
        return Err(PrivateKeyError::other(
            "That FIDO2-protected key data is invalid.",
        ));
    };
    validate_direct_layer_envelope(&envelope).map_err(PrivateKeyError::other)?;

    let resolved_pin = match pin {
        Some(pin) => {
            let trimmed = pin.trim();
            if trimmed.is_empty() {
                return Err(PrivateKeyError::fido2_pin_required(
                    "Enter the FIDO2 security key PIN.",
                ));
            }
            Some(secrecy::SecretString::from(trimmed))
        }
        None => cached_pin_string(&envelope.fingerprint).map_err(PrivateKeyError::other)?,
    };
    let hmac_salt = decode_base64(&envelope.hmac_salt).map_err(PrivateKeyError::other)?;
    let credential_id = decode_base64(&envelope.credential_id).map_err(PrivateKeyError::other)?;
    let payload_nonce = decode_base64(&envelope.payload_nonce).map_err(PrivateKeyError::other)?;
    let payload_ciphertext =
        decode_base64(&envelope.payload_ciphertext).map_err(PrivateKeyError::other)?;
    let mut excluded_devices = Vec::new();

    loop {
        let assertion = derive_direct_hmac_assertion_with_pin(
            &envelope.fingerprint,
            &envelope.rp_id,
            &credential_id,
            &hmac_salt,
            &excluded_devices,
            resolved_pin.as_ref().map(|pin| pin.expose_secret()),
        )
        .map_err(private_key_error_from_fido2_error)?;
        let kek = derive_kek(&assertion.hmac_secret, &envelope.fingerprint, &hmac_salt)?;

        match decrypt_aes_256_gcm(
            &kek,
            &payload_nonce,
            &direct_required_layer_aad(&envelope.fingerprint),
            &payload_ciphertext,
        ) {
            Ok(plaintext) => {
                if let Some(pin) = resolved_pin.as_ref().map(|pin| pin.expose_secret()) {
                    super::super::cache::cache_fido2_pin(&envelope.fingerprint, pin)
                        .map_err(PrivateKeyError::other)?;
                }
                return Ok(plaintext);
            }
            Err(err) if assertion.device.is_some() => {
                let device = assertion.device.expect("checked above");
                if excluded_devices.iter().any(|excluded| excluded == &device) {
                    return Err(err);
                }
                excluded_devices.push(device);
            }
            Err(err) => return Err(err),
        }
    }
}

fn direct_hmac_material_for_binding(
    binding: &Fido2DirectBinding,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    if let Some(enrollment) = borrow_pending_fido2_enrollment(&binding.fingerprint)?
        .filter(|enrollment| enrollment.matches_credential_id(&binding.credential_id))
    {
        return Ok((
            enrollment.hmac_salt().to_vec(),
            enrollment.hmac_secret().to_vec(),
        ));
    }

    let hmac_salt = random_bytes::<{ FIDO2_HMAC_SALT_LEN }>().to_vec();
    let cached_pin = cached_pin_string(&binding.fingerprint)?;
    let assertion = derive_direct_hmac_assertion_with_pin(
        &binding.fingerprint,
        &binding.rp_id,
        &binding.credential_id,
        &hmac_salt,
        &[],
        cached_pin.as_ref().map(|pin| pin.expose_secret()),
    )
    .map_err(private_key_error_from_fido2_error)
    .map_err(|err| err.to_string())?;
    Ok((hmac_salt, assertion.hmac_secret))
}

fn direct_required_layer_aad(fingerprint: &str) -> Vec<u8> {
    let mut aad = FIDO2_DIRECT_LAYER_AAD_PREFIX.to_vec();
    aad.extend_from_slice(fingerprint.as_bytes());
    aad
}
