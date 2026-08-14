#[cfg(feature = "fido")]
use super::super::cert::parse_fido2_public_key_bytes;
#[cfg(any(feature = "smartcard", feature = "hardwarekey", feature = "fido"))]
use super::super::cert::ManagedRipassoPrivateKey;
#[cfg(feature = "fido")]
use super::super::cert::{normalized_fingerprint, ManagedRipassoPrivateKeyProtection};
use super::super::cert::{parse_hardware_public_key_bytes, ManagedRipassoHardwareKey};
#[cfg(feature = "fido")]
use crate::PrivateKeyError;
#[cfg(feature = "fido")]
use keycord_fido::FidoPrivateKeyManifest;
use keycord_runtime::bounded_toml::{parse_toml_with_limits, TomlParseLimits};
#[cfg(feature = "fido")]
use sequoia_openpgp::Cert;
use serde::{Deserialize, Serialize as SerdeSerialize};
use std::path::Path;

const HARDWARE_MANIFEST_FORMAT: u32 = 1;
const HARDWARE_PROTECTION_KIND: &str = "hardware-openpgp-card";
const MANAGED_KEY_MANIFEST_TOML_LIMITS: TomlParseLimits = TomlParseLimits::new(128 * 1024, 16);

#[derive(Debug, Clone, SerdeSerialize, Deserialize)]
pub(super) struct HardwarePrivateKeyManifest {
    format: u32,
    protection: String,
    fingerprint: String,
    user_ids: Vec<String>,
    ident: String,
    signing_fingerprint: Option<String>,
    decryption_fingerprint: Option<String>,
    reader_hint: Option<String>,
}

impl HardwarePrivateKeyManifest {
    #[cfg(feature = "smartcard")]
    pub(super) fn from_key(
        key: &ManagedRipassoPrivateKey,
        hardware: &ManagedRipassoHardwareKey,
    ) -> Self {
        Self {
            format: HARDWARE_MANIFEST_FORMAT,
            protection: HARDWARE_PROTECTION_KIND.to_string(),
            fingerprint: key.fingerprint.clone(),
            user_ids: key.user_ids.clone(),
            ident: hardware.ident.clone(),
            signing_fingerprint: hardware.signing_fingerprint.clone(),
            decryption_fingerprint: hardware.decryption_fingerprint.clone(),
            reader_hint: hardware.reader_hint.clone(),
        }
    }

    pub(super) fn hardware(&self) -> ManagedRipassoHardwareKey {
        ManagedRipassoHardwareKey {
            ident: self.ident.clone(),
            signing_fingerprint: self.signing_fingerprint.clone(),
            decryption_fingerprint: self.decryption_fingerprint.clone(),
            reader_hint: self.reader_hint.clone(),
        }
    }
}

#[cfg(feature = "fido")]
pub(super) fn managed_fido2_private_key_from_cert(cert: &Cert) -> ManagedRipassoPrivateKey {
    ManagedRipassoPrivateKey {
        fingerprint: cert.fingerprint().to_hex(),
        user_ids: cert
            .userids()
            .map(|user_id| user_id.userid().to_string())
            .filter(|value| !value.trim().is_empty())
            .collect(),
        protection: ManagedRipassoPrivateKeyProtection::Fido2HmacSecret,
        hardware: None,
    }
}

#[cfg(feature = "fido")]
pub(super) fn parse_fido2_private_key_manifest(
    contents: &str,
) -> Result<Option<FidoPrivateKeyManifest>, String> {
    FidoPrivateKeyManifest::parse(contents, MANAGED_KEY_MANIFEST_TOML_LIMITS)
        .map_err(|error| error.to_string())
}

#[cfg(feature = "fido")]
pub(super) fn parse_fido2_private_key_manifest_bytes(
    bytes: &[u8],
) -> Result<Option<FidoPrivateKeyManifest>, String> {
    FidoPrivateKeyManifest::parse_bytes(bytes, MANAGED_KEY_MANIFEST_TOML_LIMITS)
        .map_err(|error| error.to_string())
}

#[cfg(feature = "fido")]
pub(super) fn validate_fido2_private_key_manifest(
    manifest: &FidoPrivateKeyManifest,
) -> Result<(Cert, ManagedRipassoPrivateKey), String> {
    manifest
        .validate_metadata()
        .map_err(|error| error.to_string())?;

    let (cert, key) = parse_fido2_public_key_bytes(manifest.public_key.as_bytes())
        .map_err(|err| err.to_string())?;
    let expected = normalized_fingerprint(&manifest.fingerprint)?;
    if !key.fingerprint.eq_ignore_ascii_case(&expected) {
        return Err("That FIDO2-protected key is invalid.".to_string());
    }

    Ok((cert, key))
}

#[cfg(feature = "fido")]
pub(super) fn read_fido2_private_key_manifest_entry(
    path: &Path,
    manifest: FidoPrivateKeyManifest,
) -> Result<super::storage::StoredPrivateKeyEntry, String> {
    let (cert, key) = validate_fido2_private_key_manifest(&manifest)?;

    Ok(super::storage::StoredPrivateKeyEntry {
        cert: Some(cert),
        key,
        location: super::storage::StoredPrivateKeyLocation::Fido2 {
            path: path.to_path_buf(),
        },
    })
}

#[cfg(feature = "fido")]
pub(super) fn fido2_private_key_manifest_contents(
    manifest: &FidoPrivateKeyManifest,
) -> Result<String, PrivateKeyError> {
    manifest
        .to_pretty_toml()
        .map_err(|error| PrivateKeyError::other(error.to_string()))
}

pub(super) fn read_hardware_private_key_manifest_entry(
    dir: &Path,
    manifest: HardwarePrivateKeyManifest,
) -> Result<super::storage::StoredPrivateKeyEntry, String> {
    if manifest.format != HARDWARE_MANIFEST_FORMAT {
        return Err(format!(
            "Unsupported hardware key manifest format {}.",
            manifest.format
        ));
    }
    if manifest.protection != HARDWARE_PROTECTION_KIND {
        return Err(format!(
            "Unsupported hardware key protection '{}'.",
            manifest.protection
        ));
    }

    let hardware = manifest.hardware();
    let (cert, mut key) = parse_hardware_public_key_bytes(
        &std::fs::read(super::paths::hardware_public_key_path(dir))
            .map_err(|err| err.to_string())?,
        hardware.clone(),
    )
    .map_err(|err| err.to_string())?;
    key.user_ids = manifest.user_ids;

    Ok(super::storage::StoredPrivateKeyEntry {
        cert: Some(cert),
        key,
        location: super::storage::StoredPrivateKeyLocation::Hardware {
            dir: dir.to_path_buf(),
            hardware,
        },
    })
}

pub(super) fn read_hardware_private_key_manifest(
    dir: &Path,
) -> Result<HardwarePrivateKeyManifest, String> {
    let contents = std::fs::read_to_string(super::paths::hardware_manifest_path(dir))
        .map_err(|err| err.to_string())?;
    parse_toml_with_limits(
        &contents,
        MANAGED_KEY_MANIFEST_TOML_LIMITS,
        "hardware key manifest",
    )
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "fido")]
    use super::parse_fido2_private_key_manifest;
    use super::{HardwarePrivateKeyManifest, MANAGED_KEY_MANIFEST_TOML_LIMITS};
    use keycord_runtime::bounded_toml::parse_toml_with_limits;

    #[test]
    fn managed_key_manifest_limit_is_owned_by_keys() {
        let oversized = "x".repeat(MANAGED_KEY_MANIFEST_TOML_LIMITS.max_bytes + 1);
        let error = parse_toml_with_limits::<HardwarePrivateKeyManifest>(
            &oversized,
            MANAGED_KEY_MANIFEST_TOML_LIMITS,
            "hardware key manifest",
        )
        .expect_err("oversized managed-key manifests must be rejected");

        assert!(error.contains("size limit"));
    }

    #[cfg(feature = "fido")]
    #[test]
    fn fido_private_key_manifests_use_the_keys_owned_limit() {
        let oversized = "x".repeat(MANAGED_KEY_MANIFEST_TOML_LIMITS.max_bytes + 1);
        let error = parse_fido2_private_key_manifest(&oversized)
            .expect_err("oversized FIDO private-key manifests must be rejected");

        assert!(error.contains("size limit"));
    }
}
