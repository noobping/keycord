//! Passless's native binary CBOR. Existing records are kept as opaque bytes by Entries.
use crate::common::{key_error, validate_rp_id, MAX_STORAGE_BYTES};
use crate::{ImportedCredential, PreparedPasskey};
use serde::{Deserialize, Serialize};
use std::{fmt, io::Cursor, path::Path};
use zeroize::{Zeroize, Zeroizing};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PasslessRp {
    pub id: String,
    pub name: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PasslessUser {
    #[serde(deserialize_with = "bytes")]
    pub id: Vec<u8>,
    pub name: Option<String>,
    pub display_name: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BackupState {
    Legacy(bool),
    Named(String),
}
impl Default for BackupState {
    fn default() -> Self {
        Self::Named("notEligible".into())
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Extensions {
    pub cred_protect: Option<u8>,
    pub hmac_secret: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cred_random: Option<Vec<u8>>,
}
impl Drop for Extensions {
    fn drop(&mut self) {
        self.cred_random.zeroize();
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct PasslessCredential {
    #[serde(deserialize_with = "bytes")]
    pub id: Vec<u8>,
    pub rp: PasslessRp,
    pub user: PasslessUser,
    pub sign_count: u32,
    pub alg: i32,
    #[serde(deserialize_with = "bytes")]
    private_key: Vec<u8>,
    #[serde(default)]
    pub key_provider: Option<Vec<u8>>,
    #[serde(default)]
    pub key_format_version: Option<u16>,
    pub created: i64,
    pub discoverable: bool,
    #[serde(default)]
    pub backup_state: BackupState,
    #[serde(default)]
    pub extensions: Extensions,
}
impl Drop for PasslessCredential {
    fn drop(&mut self) {
        self.private_key.zeroize();
    }
}
impl fmt::Debug for PasslessCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PasslessCredential")
            .field("rp", &self.rp)
            .field("private_key", &"[redacted]")
            .finish_non_exhaustive()
    }
}
fn bytes<'de, D: serde::Deserializer<'de>>(de: D) -> Result<Vec<u8>, D::Error> {
    struct Bytes;
    impl<'de> serde::de::Visitor<'de> for Bytes {
        type Value = Vec<u8>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("bytes")
        }
        fn visit_bytes<E: serde::de::Error>(self, value: &[u8]) -> Result<Self::Value, E> {
            Ok(value.to_vec())
        }
        fn visit_byte_buf<E: serde::de::Error>(self, value: Vec<u8>) -> Result<Self::Value, E> {
            Ok(value)
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            let mut out = Vec::new();
            while let Some(byte) = seq.next_element::<u8>()? {
                if out.len() >= MAX_STORAGE_BYTES {
                    return Err(serde::de::Error::custom("oversized"));
                }
                out.push(byte);
            }
            Ok(out)
        }
    }
    de.deserialize_any(Bytes)
}
impl PasslessCredential {
    fn validate(&self) -> Result<(), String> {
        validate_rp_id(&self.rp.id)?;
        if self.id.is_empty()
            || self.id.len() > 1023
            || self.user.id.is_empty()
            || self.user.id.len() > 64
            || self.created < 0
        {
            return Err("The passkey contains invalid account or creation metadata.".into());
        }
        if self
            .key_provider
            .as_deref()
            .is_some_and(|v| v != b"software-v1")
            || self.key_format_version.is_some_and(|v| v != 1)
        {
            return Err("This Passless key provider is not supported.".into());
        }
        if !matches!(self.alg, -7 | -8 | -19) {
            return Err("This passkey algorithm is not supported by Passless.".into());
        }
        if self.private_key.len() != 32 {
            return Err("The passkey private key has an invalid length.".into());
        }
        if self.alg == -7 {
            let scalar = openssl::bn::BigNum::from_slice(&self.private_key).map_err(key_error)?;
            let group = openssl::ec::EcGroup::from_curve_name(openssl::nid::Nid::X9_62_PRIME256V1)
                .map_err(key_error)?;
            let mut order = openssl::bn::BigNum::new().map_err(key_error)?;
            let mut ctx = openssl::bn::BigNumContext::new().map_err(key_error)?;
            group.order(&mut order, &mut ctx).map_err(key_error)?;
            if scalar.num_bits() == 0 || scalar >= order {
                return Err("The passkey contains an invalid P-256 private scalar.".into());
            }
        }
        if matches!(&self.backup_state, BackupState::Named(v) if !matches!(v.as_str(), "notEligible" | "eligible" | "backedUp"))
            || self
                .extensions
                .cred_protect
                .is_some_and(|v| !(1..=3).contains(&v))
            || self
                .extensions
                .cred_random
                .as_ref()
                .is_some_and(|v| v.len() != 32)
        {
            return Err("The Passless record contains invalid metadata.".into());
        }
        Ok(())
    }
    pub fn validate_label(&self, label: &str) -> Result<(), String> {
        let path = Path::new(label.strip_suffix(".gpg").unwrap_or(label));
        let hex: String = self.id.iter().map(|b| format!("{b:02x}")).collect();
        if path
            .file_name()
            .and_then(|p| p.to_str())
            .is_some_and(|n| n.eq_ignore_ascii_case(&hex))
            && path
                .parent()
                .and_then(Path::file_name)
                .and_then(|p| p.to_str())
                == Some(&self.rp.id)
        {
            Ok(())
        } else {
            Err("Keep the passkey's credential ID as its filename and its RP ID as the parent folder.".into())
        }
    }
}
pub fn decode_passless(data: &[u8]) -> Result<PasslessCredential, String> {
    if data.len() > MAX_STORAGE_BYTES {
        return Err("The passkey record is too large.".into());
    }
    let mut cursor = Cursor::new(data);
    let value: PasslessCredential = ciborium::from_reader(&mut cursor)
        .map_err(|_| "Invalid Passless CBOR record.".to_string())?;
    if cursor.position() != data.len() as u64 {
        return Err("Unexpected data after the passkey record.".into());
    }
    value.validate()?;
    Ok(value)
}
pub fn inspect_passless(data: &[u8]) -> Option<Result<PasslessCredential, String>> {
    let prefix = &data[..data.len().min(MAX_STORAGE_BYTES)];
    if prefix.first().is_none_or(|b| b >> 5 != 5)
        || !prefix.windows(12).any(|v| v == b"\x6bprivate_key")
    {
        return None;
    }
    Some(decode_passless(data))
}
pub(crate) fn prepare_import(value: &ImportedCredential) -> Result<PreparedPasskey, String> {
    let credential = PasslessCredential {
        id: value.id.clone(),
        rp: PasslessRp {
            id: value.rp.id.clone(),
            name: value.rp.name.clone(),
        },
        user: PasslessUser {
            id: value.user.id.clone(),
            name: Some(value.user.name.clone()),
            display_name: value.user.display_name.clone(),
        },
        sign_count: 0,
        alg: value.alg,
        private_key: value.private_key.clone(),
        key_provider: None,
        key_format_version: None,
        created: value.created,
        discoverable: true,
        backup_state: BackupState::default(),
        extensions: Extensions::default(),
    };
    credential.validate()?;
    let mut contents = Zeroizing::new(Vec::new());
    ciborium::into_writer(&credential, &mut *contents)
        .map_err(|_| "Couldn't encode the passkey record.".to_string())?;
    let id: String = value.id.iter().map(|b| format!("{b:02x}")).collect();
    Ok(PreparedPasskey {
        label: format!("fido2/{}/{id}", value.rp.id),
        contents,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ciborium::Value;
    fn imported() -> PreparedPasskey {
        crate::parse_cxf_passkey_json(include_str!("../tests/fixtures/es256.cxf.json"))
            .unwrap()
            .prepare(crate::PasskeyFormat::Passless)
            .unwrap()
    }
    fn changed(key: &str, value: Value) -> Vec<u8> {
        let entry = imported();
        let mut map: Value = ciborium::from_reader(entry.contents.as_slice()).unwrap();
        let Value::Map(fields) = &mut map else {
            panic!()
        };
        fields.retain(|(k, _)| k.as_text() != Some(key));
        fields.push((Value::Text(key.into()), value));
        let mut bytes = Vec::new();
        ciborium::into_writer(&map, &mut bytes).unwrap();
        bytes
    }
    #[test]
    fn imports_use_native_arrays_defaults_and_discoverable_paths() {
        for name in [
            include_str!("../tests/fixtures/es256.cxf.json"),
            include_str!("../tests/fixtures/ed25519.cxf.json"),
        ] {
            let value = crate::parse_cxf_passkey_json(name).unwrap();
            let entry = value.prepare(crate::PasskeyFormat::Passless).unwrap();
            let decoded = decode_passless(&entry.contents).unwrap();
            decoded.validate_label(&entry.label).unwrap();
            assert!(entry.label.starts_with("fido2/"));
            assert!(crate::is_passkey_entry_label(&entry.label));
            assert!(decoded.validate_label("example.com/wrong").is_err());
            assert_eq!(decoded.sign_count, 0);
            assert_eq!(decoded.created, value.created);
            assert!(decoded.discoverable);
            assert!(
                matches!(decoded.backup_state, BackupState::Named(ref s) if s == "notEligible")
            );
            let Value::Map(fields) =
                ciborium::from_reader::<Value, _>(entry.contents.as_slice()).unwrap()
            else {
                panic!()
            };
            for key in ["id", "private_key"] {
                assert!(matches!(
                    fields
                        .iter()
                        .find(|(k, _)| k.as_text() == Some(key))
                        .unwrap()
                        .1,
                    Value::Array(_)
                ));
            }
        }
    }
    #[test]
    fn unsupported_imports_never_silently_change_algorithms_or_extensions() {
        let rsa = crate::parse_cxf_passkey_json(include_str!("../tests/fixtures/rs256.cxf.json"));
        #[cfg(feature = "passkey")]
        assert!(rsa
            .unwrap()
            .prepare(crate::PasskeyFormat::Passless)
            .is_err());
        #[cfg(not(feature = "passkey"))]
        assert!(rsa.is_err());
        let mut cxf: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/es256.cxf.json")).unwrap();
        cxf["fido2Extensions"] = serde_json::json!({"hmacSecret": true});
        assert!(crate::parse_cxf_passkey_json(&cxf.to_string()).is_err());
    }
    #[test]
    fn malformed_native_records_remain_recognizable() {
        for bytes in [
            changed("alg", Value::Integer((-257).into())),
            changed(
                "private_key",
                Value::Array(vec![Value::Integer(256.into())]),
            ),
            changed(
                "private_key",
                Value::Array(vec![Value::Integer((-1).into())]),
            ),
            changed("discoverable", Value::Null),
            changed(
                "key_provider",
                Value::Array(vec![Value::Integer(99.into())]),
            ),
            changed("backup_state", Value::Text("invalid".into())),
        ] {
            assert!(inspect_passless(&bytes).unwrap().is_err());
        }
        let entry = imported();
        let mut trailing = entry.contents.to_vec();
        trailing.push(0);
        assert!(inspect_passless(&trailing).unwrap().is_err());
        let mut oversized = entry.contents.to_vec();
        oversized.resize(MAX_STORAGE_BYTES + 1, 0);
        assert!(inspect_passless(&oversized).unwrap().is_err());
        assert!(inspect_passless(b"ordinary password").is_none());
        assert!(inspect_passless(include_str!("../tests/fixtures/es256.b64").as_bytes()).is_none());
    }
    #[test]
    fn native_optional_metadata_and_byte_strings_are_supported() {
        let data = changed("backup_state", Value::Bool(true));
        assert!(matches!(
            decode_passless(&data).unwrap().backup_state,
            BackupState::Legacy(true)
        ));
        assert!(decode_passless(&changed("id", Value::Bytes(vec![7; 32]))).is_ok());
        assert!(decode_passless(&changed(
            "future_extension",
            Value::Text("preserve me".into())
        ))
        .is_ok());
        let entry = imported();
        let mut value: Value = ciborium::from_reader(entry.contents.as_slice()).unwrap();
        let Value::Map(fields) = &mut value else {
            panic!()
        };
        let user = &mut fields
            .iter_mut()
            .find(|(k, _)| k.as_text() == Some("user"))
            .unwrap()
            .1;
        let Value::Map(fields) = user else { panic!() };
        fields.retain(|(k, _)| !matches!(k.as_text(), Some("name" | "display_name")));
        let mut data = Vec::new();
        ciborium::into_writer(&value, &mut data).unwrap();
        assert!(decode_passless(&data).unwrap().user.name.is_none());
    }
}
