use crate::FidoError;
use keycord_runtime::bounded_toml::{validate_toml_input, TomlParseLimits};
use serde::{Deserialize, Serialize};

pub const FIDO_PRIVATE_KEY_MANIFEST_FORMAT: u32 = 1;
pub const FIDO_PRIVATE_KEY_PROTECTION_KIND: &str = "fido2-hmac-secret";

/// On-disk wrapper for an OpenPGP public key and its FIDO-protected secret bytes.
///
/// The FIDO crate validates the wrapper. The Keys crate remains responsible for
/// parsing `public_key` and checking that its fingerprint matches `fingerprint`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FidoPrivateKeyManifest {
    pub format: u32,
    pub protection: String,
    pub fingerprint: String,
    pub public_key: String,
    pub encrypted_private_key: String,
}

impl FidoPrivateKeyManifest {
    pub fn new(
        fingerprint: impl Into<String>,
        public_key: impl Into<String>,
        encrypted_private_key: impl Into<String>,
    ) -> Self {
        Self {
            format: FIDO_PRIVATE_KEY_MANIFEST_FORMAT,
            protection: FIDO_PRIVATE_KEY_PROTECTION_KIND.to_string(),
            fingerprint: fingerprint.into(),
            public_key: public_key.into(),
            encrypted_private_key: encrypted_private_key.into(),
        }
    }

    /// Parse a manifest under the caller's subject-owned persistence limits.
    pub fn parse(contents: &str, limits: TomlParseLimits) -> Result<Option<Self>, FidoError> {
        validate_toml_input(contents, limits, "FIDO2 private key manifest")
            .map_err(FidoError::invalid)?;
        Ok(toml::from_str(contents).ok())
    }

    pub fn parse_bytes(bytes: &[u8], limits: TomlParseLimits) -> Result<Option<Self>, FidoError> {
        let Ok(contents) = std::str::from_utf8(bytes) else {
            return Ok(None);
        };
        Self::parse(contents, limits)
    }

    pub fn validate_metadata(&self) -> Result<(), FidoError> {
        if self.format != FIDO_PRIVATE_KEY_MANIFEST_FORMAT {
            return Err(FidoError::invalid(format!(
                "Unsupported FIDO2 private key format {}.",
                self.format
            )));
        }
        if self.protection != FIDO_PRIVATE_KEY_PROTECTION_KIND {
            return Err(FidoError::invalid(format!(
                "Unsupported FIDO2 private key protection '{}'.",
                self.protection
            )));
        }
        Ok(())
    }

    pub fn to_pretty_toml(&self) -> Result<String, FidoError> {
        toml::to_string_pretty(self).map_err(|error| FidoError::invalid(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::FidoPrivateKeyManifest;
    use keycord_runtime::bounded_toml::TomlParseLimits;

    const TEST_LIMITS: TomlParseLimits = TomlParseLimits::new(usize::MAX, usize::MAX);

    const EXISTING_MANIFEST: &str = concat!(
        "format = 1\n",
        "protection = \"fido2-hmac-secret\"\n",
        "fingerprint = \"0123456789ABCDEF0123456789ABCDEF01234567\"\n",
        "public_key = \"\"\"\n",
        "-----BEGIN PGP PUBLIC KEY BLOCK-----\n",
        "fixture\n",
        "-----END PGP PUBLIC KEY BLOCK-----\n",
        "\"\"\"\n",
        "encrypted_private_key = \"\"\"\n",
        "keycord-fido2-required-layer-v1\n",
        "fixture = true\n",
        "\"\"\"\n",
    );

    #[test]
    fn existing_manifest_round_trips_byte_for_byte() {
        let manifest = FidoPrivateKeyManifest::parse(EXISTING_MANIFEST, TEST_LIMITS)
            .unwrap()
            .expect("existing manifest");
        manifest.validate_metadata().unwrap();
        assert_eq!(manifest.to_pretty_toml().unwrap(), EXISTING_MANIFEST);
    }

    #[test]
    fn invalid_toml_is_not_claimed_as_a_fido_manifest() {
        assert!(FidoPrivateKeyManifest::parse("not toml", TEST_LIMITS)
            .unwrap()
            .is_none());
        assert!(FidoPrivateKeyManifest::parse_bytes(&[0xff], TEST_LIMITS)
            .unwrap()
            .is_none());
    }

    #[test]
    fn unsupported_metadata_keeps_existing_error_text() {
        let mut manifest = FidoPrivateKeyManifest::new("fingerprint", "public", "encrypted");
        manifest.format = 99;
        assert_eq!(
            manifest.validate_metadata().unwrap_err().to_string(),
            "Unsupported FIDO2 private key format 99."
        );
        manifest.format = 1;
        manifest.protection = "password".into();
        assert_eq!(
            manifest.validate_metadata().unwrap_err().to_string(),
            "Unsupported FIDO2 private key protection 'password'."
        );
    }
}
