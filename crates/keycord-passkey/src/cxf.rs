//! Format-independent CXF import data.

use crate::common::{key_error, validate_rp_id, MAX_STORAGE_BYTES};
use crate::import::{ImportRp, ImportUser, ImportedCredential};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use openssl::{
    nid::Nid,
    pkey::{Id, PKey},
};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};
use zeroize::Zeroizing;

pub fn parse_cxf_passkey_json(input: &str) -> Result<ImportedCredential, String> {
    if input.len() > MAX_STORAGE_BYTES {
        return Err("The passkey import is too large.".into());
    }
    let value: Value =
        serde_json::from_str(input).map_err(|_| "Invalid passkey JSON.".to_string())?;
    let mut candidates = Vec::new();
    collect_passkeys(&value, &mut candidates);
    let [value] = candidates.as_slice() else {
        return Err("Choose a JSON object containing exactly one passkey credential.".into());
    };
    if value.get("fido2Extensions").is_some_and(|extensions| {
        !extensions
            .as_object()
            .is_some_and(|object| object.is_empty())
    }) {
        return Err("This import does not support FIDO2 extensions.".into());
    }
    let key = Zeroizing::new(binary(value, "key")?);
    let pkey = PKey::private_key_from_pkcs8(&key).map_err(key_error)?;
    let (alg, private_key) = match pkey.id() {
        Id::EC => {
            let ec = pkey.ec_key().map_err(key_error)?;
            if ec.group().curve_name() != Some(Nid::X9_62_PRIME256V1) {
                return Err("Only the P-256 EC curve is supported.".into());
            }
            ec.check_key().map_err(key_error)?;
            (-7, ec.private_key().to_vec_padded(32).map_err(key_error)?)
        }
        Id::ED25519 => (-8, pkey.raw_private_key().map_err(key_error)?),
        #[cfg(feature = "passkey")]
        Id::RSA => {
            let rsa = pkey.rsa().map_err(key_error)?;
            if rsa.n().num_bits() != 2048
                || rsa.e().to_vec() != [1, 0, 1]
                || !rsa.check_key().map_err(key_error)?
            {
                return Err("RSA imports require RSA-2048 with exponent 65537.".into());
            }
            let mut bytes = rsa.n().to_vec_padded(256).map_err(key_error)?;
            bytes.extend(rsa.d().to_vec_padded(256).map_err(key_error)?);
            (-257, bytes)
        }
        #[cfg(not(feature = "passkey"))]
        Id::RSA => return Err("This passkey algorithm is not supported by Passless.".into()),
        _ => return Err("This passkey algorithm is not supported.".into()),
    };
    let credential = ImportedCredential {
        id: binary(value, "credentialId")?,
        rp: ImportRp {
            id: required(value, &["rpId"])?.into(),
            name: None,
        },
        user: ImportUser {
            id: binary(value, "userHandle")?,
            name: required(value, &["username", "userName"])?.into(),
            display_name: optional(value, &["userDisplayName", "displayName"]).map(str::to_string),
        },
        alg,
        private_key,
        created: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "Couldn't determine the passkey creation time.".to_string())?
            .as_secs() as i64,
    };
    validate_rp_id(&credential.rp.id)?;
    if credential.id.len() != 32 {
        return Err("Passkey imports require a 32-byte credential ID.".into());
    }
    if credential.user.id.is_empty() || credential.user.id.len() > 64 {
        return Err("The passkey user handle must contain 1 to 64 bytes.".into());
    }
    Ok(credential)
}

fn optional<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| value.get(*key)?.as_str())
}

fn required<'a>(value: &'a Value, keys: &[&str]) -> Result<&'a str, String> {
    optional(value, keys)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("The passkey is missing {}.", keys[0]))
}

fn binary(value: &Value, key: &str) -> Result<Vec<u8>, String> {
    URL_SAFE_NO_PAD
        .decode(required(value, &[key])?)
        .map_err(|_| format!("The passkey {key} must use unpadded Base64URL."))
}

fn collect_passkeys<'a>(value: &'a Value, found: &mut Vec<&'a Value>) {
    if value.get("type").and_then(Value::as_str) == Some("passkey") {
        found.push(value);
        return;
    }
    for key in ["passkey", "credential"] {
        if let Some(child) = value.get(key) {
            collect_passkeys(child, found);
        }
    }
    for key in ["credentials", "items", "accounts"] {
        if let Some(children) = value.get(key).and_then(Value::as_array) {
            for child in children {
                collect_passkeys(child, found);
            }
        }
    }
}
