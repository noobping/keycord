//! Android Password Store's first-line Base64URL/CBOR credential format.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use openssl::{bn::BigNum, ec::EcGroup, nid::Nid};
use serde::{Deserialize, Serialize};
use std::{fmt, io::Cursor, path::Path};
use zeroize::{Zeroize, Zeroizing};

use crate::common::{key_error, validate_rp_id, MAX_STORAGE_BYTES};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelyingParty {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: Vec<u8>,
    pub name: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub reveal_name: bool,
}

/// Byte vectors intentionally serialize as CBOR integer arrays, not byte strings.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PasskeyCredential {
    pub id: Vec<u8>,
    pub rp: RelyingParty,
    pub user: UserInfo,
    #[serde(default)]
    pub sign_count: u32,
    pub alg: i32,
    pub private_key: Vec<u8>,
    pub created: i64,
    #[serde(default = "utc")]
    pub zone: String,
}

fn utc() -> String {
    "UTC".into()
}

impl Drop for PasskeyCredential {
    fn drop(&mut self) {
        self.private_key.zeroize();
    }
}

impl fmt::Debug for PasskeyCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PasskeyCredential")
            .field("id", &self.id)
            .field("rp", &self.rp)
            .field("user", &self.user)
            .field("alg", &self.alg)
            .field("sign_count", &self.sign_count)
            .field("created", &self.created)
            .field("zone", &self.zone)
            .field("private_key", &"[redacted]")
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct PasskeyStorageEntry {
    pub label: String,
    pub contents: String,
}

impl fmt::Debug for PasskeyStorageEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PasskeyStorageEntry")
            .field("label", &self.label)
            .field("contents", &"[redacted]")
            .finish()
    }
}

impl PasskeyCredential {
    pub fn credential_id_hex(&self) -> String {
        self.id.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.id.len() != 32 {
            return Err("Android Password Store requires a 32-byte credential ID.".into());
        }
        validate_rp_id(&self.rp.id)?;
        if self.user.id.is_empty() || self.user.id.len() > 64 {
            return Err("The passkey user handle must contain 1 to 64 bytes.".into());
        }
        if self.user.name.is_empty() || self.created < 0 || self.zone.is_empty() {
            return Err("The passkey contains invalid account or creation metadata.".into());
        }
        match self.alg {
            -7 if self.private_key.len() == 32 => {
                let scalar = BigNum::from_slice(&self.private_key).map_err(key_error)?;
                let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1).map_err(key_error)?;
                let mut order = BigNum::new().map_err(key_error)?;
                let mut context = openssl::bn::BigNumContext::new().map_err(key_error)?;
                group.order(&mut order, &mut context).map_err(key_error)?;
                if scalar.num_bits() == 0 || scalar >= order {
                    return Err("The passkey contains an invalid P-256 private scalar.".into());
                }
            }
            -8 if self.private_key.len() == 32 => {}
            -257 if self.private_key.len() == 512 => {
                let n = BigNum::from_slice(&self.private_key[..256]).map_err(key_error)?;
                let d = BigNum::from_slice(&self.private_key[256..]).map_err(key_error)?;
                if n.num_bits() != 2048 || !n.is_odd() || d.num_bits() == 0 || d >= n {
                    return Err("The passkey contains invalid RSA-2048 key material.".into());
                }
            }
            -7 | -8 | -257 => return Err("The passkey private key has an invalid length.".into()),
            _ => {
                return Err(
                    "This passkey algorithm is not supported by Android Password Store.".into(),
                )
            }
        }
        Ok(())
    }
}

pub fn encode_passkey_storage_value(credential: &PasskeyCredential) -> Result<String, String> {
    credential.validate()?;
    let mut bytes = Zeroizing::new(Vec::new());
    ciborium::into_writer(credential, &mut *bytes)
        .map_err(|_| "Couldn't encode the passkey record.".to_string())?;
    let encoded = URL_SAFE_NO_PAD.encode(&bytes);
    if encoded.len() > MAX_STORAGE_BYTES {
        return Err("The passkey record is too large.".into());
    }
    Ok(encoded)
}

pub fn decode_passkey_storage_value(value: &str) -> Result<PasskeyCredential, String> {
    if value.len() > MAX_STORAGE_BYTES {
        return Err("The passkey record is too large.".into());
    }
    let bytes = Zeroizing::new(
        URL_SAFE_NO_PAD
            .decode(value)
            .map_err(|_| "Invalid passkey Base64URL encoding.".to_string())?,
    );
    let mut reader = Cursor::new(bytes.as_slice());
    let credential: PasskeyCredential = ciborium::from_reader(&mut reader)
        .map_err(|_| "Invalid Android Password Store CBOR record.".to_string())?;
    if reader.position() != bytes.len() as u64 {
        return Err("Unexpected data after the passkey record.".into());
    }
    credential.validate()?;
    Ok(credential)
}

/// Recognizable damaged/unsupported credentials must not fall back to password copy/export.
/// Only a bounded prefix is decoded before semantic validation.
pub fn inspect_passkey_storage_value(value: &str) -> Option<Result<PasskeyCredential, String>> {
    let prefix_len = value
        .bytes()
        .take(MAX_STORAGE_BYTES)
        .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        .count()
        / 4
        * 4;
    let prefix = value.get(..prefix_len)?;
    let bytes = Zeroizing::new(URL_SAFE_NO_PAD.decode(prefix).ok()?);
    if bytes.first().is_none_or(|byte| byte >> 5 != 5)
        || !bytes.windows(12).any(|part| part == b"\x6bprivate_key")
    {
        return None;
    }
    Some(decode_passkey_storage_value(value))
}

pub fn build_passkey_storage_entry(
    credential: &PasskeyCredential,
) -> Result<PasskeyStorageEntry, String> {
    Ok(PasskeyStorageEntry {
        label: format!(
            "passkeys/{}/{}",
            credential.rp.id,
            credential.credential_id_hex()
        ),
        contents: encode_passkey_storage_value(credential)?,
    })
}

pub fn validate_passkey_entry_label(
    credential: &PasskeyCredential,
    label: &str,
) -> Result<(), String> {
    let path = Path::new(label.strip_suffix(".gpg").unwrap_or(label));
    let valid = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(&credential.credential_id_hex()))
        && path
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            == Some(credential.rp.id.as_str());
    if valid {
        Ok(())
    } else {
        Err(
            "Keep the passkey's credential ID as its filename and its RP ID as the parent folder."
                .into(),
        )
    }
}

#[cfg(test)]
mod tests;
