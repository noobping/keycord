use crate::crypto::{decode_base64, encode_base64};
use crate::FidoError;
use keycord_runtime::bounded_toml::{parse_toml_with_limits, TomlParseLimits};
use serde::{Deserialize, Serialize};

pub const FIDO_REQUIRED_LAYER_HEADER: &str = "keycord-fido2-required-layer-v1";
pub(crate) const FIDO_REQUIRED_LAYER_FORMAT: u32 = 1;
pub(crate) const FIDO_REQUIRED_LAYER_KIND: &str = "fido2-required-layer";
pub(crate) const FIDO_REQUIRED_LAYER_AAD_PREFIX: &[u8] =
    b"keycord/fido2-required-layer/payload/v1:";
const FIDO2_TEXT_ENVELOPE_TOML_LIMITS: TomlParseLimits = TomlParseLimits::new(1024 * 1024, 16);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FidoBinding {
    pub fingerprint: String,
    pub label: String,
    pub rp_id: String,
    pub credential_id: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FidoBindingDescriptor {
    pub fingerprint: String,
    pub label: String,
    pub credential_id: Vec<u8>,
}

impl FidoBindingDescriptor {
    pub fn binding(&self) -> FidoBinding {
        FidoBinding {
            fingerprint: self.fingerprint.clone(),
            label: self.label.clone(),
            rp_id: crate::FIDO_RP_ID.to_string(),
            credential_id: self.credential_id.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RequiredLayerEnvelope {
    pub(crate) format: u32,
    pub(crate) protection: String,
    pub(crate) fingerprint: String,
    pub(crate) rp_id: String,
    pub(crate) credential_id: String,
    pub(crate) hmac_salt: String,
    pub(crate) payload_nonce: String,
    pub(crate) payload_ciphertext: String,
}

impl RequiredLayerEnvelope {
    pub(crate) fn new(
        binding: &FidoBinding,
        hmac_salt: &[u8],
        payload_nonce: &[u8],
        payload_ciphertext: &[u8],
    ) -> Self {
        Self {
            format: FIDO_REQUIRED_LAYER_FORMAT,
            protection: FIDO_REQUIRED_LAYER_KIND.to_string(),
            fingerprint: binding.fingerprint.clone(),
            rp_id: binding.rp_id.clone(),
            credential_id: encode_base64(&binding.credential_id),
            hmac_salt: encode_base64(hmac_salt),
            payload_nonce: encode_base64(payload_nonce),
            payload_ciphertext: encode_base64(payload_ciphertext),
        }
    }

    pub(crate) fn validate(&self) -> Result<(), FidoError> {
        if self.format != FIDO_REQUIRED_LAYER_FORMAT {
            return Err(FidoError::invalid(format!(
                "Unsupported FIDO2 password-entry format {}.",
                self.format
            )));
        }
        if self.protection != FIDO_REQUIRED_LAYER_KIND {
            return Err(FidoError::invalid(format!(
                "Unsupported FIDO2 password-entry protection '{}'.",
                self.protection
            )));
        }
        decode_base64(&self.credential_id)?;
        decode_base64(&self.hmac_salt)?;
        decode_base64(&self.payload_nonce)?;
        decode_base64(&self.payload_ciphertext)?;
        Ok(())
    }
}

pub(crate) fn serialize(envelope: &RequiredLayerEnvelope) -> Result<Vec<u8>, FidoError> {
    let body = toml::to_string(envelope).map_err(|error| FidoError::invalid(error.to_string()))?;
    let mut encoded = format!("{FIDO_REQUIRED_LAYER_HEADER}\n").into_bytes();
    encoded.extend_from_slice(body.as_bytes());
    Ok(encoded)
}

pub(crate) fn parse(ciphertext: &[u8]) -> Result<Option<RequiredLayerEnvelope>, FidoError> {
    let prefix = format!("{FIDO_REQUIRED_LAYER_HEADER}\n");
    let Some(body) = ciphertext.strip_prefix(prefix.as_bytes()) else {
        return Ok(None);
    };
    let body = std::str::from_utf8(body).map_err(|error| FidoError::invalid(error.to_string()))?;
    parse_toml_with_limits(body, FIDO2_TEXT_ENVELOPE_TOML_LIMITS, "FIDO2 text envelope")
        .map_err(FidoError::invalid)
        .map(Some)
}

pub(crate) fn required_layer_aad(fingerprint: &str) -> Vec<u8> {
    let mut aad = FIDO_REQUIRED_LAYER_AAD_PREFIX.to_vec();
    aad.extend_from_slice(fingerprint.as_bytes());
    aad
}

#[cfg(test)]
mod tests {
    use super::{
        parse, serialize, RequiredLayerEnvelope, FIDO2_TEXT_ENVELOPE_TOML_LIMITS,
        FIDO_REQUIRED_LAYER_HEADER,
    };

    const EXISTING_ENVELOPE: &str = concat!(
        "keycord-fido2-required-layer-v1\n",
        "format = 1\n",
        "protection = \"fido2-required-layer\"\n",
        "fingerprint = \"0123456789ABCDEF0123456789ABCDEF01234567\"\n",
        "rp_id = \"io.github.noobping.keycord\"\n",
        "credential_id = \"Y3JlZGVudGlhbA==\"\n",
        "hmac_salt = \"c2FsdA==\"\n",
        "payload_nonce = \"bm9uY2U=\"\n",
        "payload_ciphertext = \"Y2lwaGVydGV4dA==\"\n",
    );

    #[test]
    fn existing_required_layer_envelope_round_trips_byte_for_byte() {
        let envelope = parse(EXISTING_ENVELOPE.as_bytes())
            .unwrap()
            .expect("existing envelope");
        envelope.validate().unwrap();
        assert_eq!(serialize(&envelope).unwrap(), EXISTING_ENVELOPE.as_bytes());
    }

    #[test]
    fn unrelated_payload_is_not_claimed() {
        assert!(parse(b"-----BEGIN PGP MESSAGE-----\n").unwrap().is_none());
    }

    #[test]
    fn unsupported_metadata_is_rejected() {
        let mut envelope: RequiredLayerEnvelope =
            parse(EXISTING_ENVELOPE.as_bytes()).unwrap().unwrap();
        envelope.format = 99;
        assert_eq!(
            envelope.validate().unwrap_err().to_string(),
            "Unsupported FIDO2 password-entry format 99."
        );
        assert_eq!(
            FIDO_REQUIRED_LAYER_HEADER,
            "keycord-fido2-required-layer-v1"
        );
    }

    #[test]
    fn text_envelope_limit_is_owned_by_fido() {
        let oversized_body = "x".repeat(FIDO2_TEXT_ENVELOPE_TOML_LIMITS.max_bytes + 1);
        let input = format!("{FIDO_REQUIRED_LAYER_HEADER}\n{oversized_body}");
        assert!(parse(input.as_bytes())
            .unwrap_err()
            .to_string()
            .contains("size limit"));
    }
}
