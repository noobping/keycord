use std::fmt;

pub const PASSKEY_FIELD_KEY: &str = "passkey";
pub const PASSKEY_ENVELOPE_PREFIX: &str = "keycord-passkey-v1:";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(feature = "passkey"), allow(dead_code))]
pub enum PasskeyRegistrationState {
    Imported,
    GeneratedUnregistered,
    Registered,
}

impl PasskeyRegistrationState {
    #[cfg(all(test, feature = "passkey"))]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Imported => "imported",
            Self::GeneratedUnregistered => "generated-unregistered",
            Self::Registered => "registered",
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct PasskeyCredential {
    pub credential_id: String,
    pub rp_id: String,
    pub username: String,
    pub user_display_name: String,
    pub user_handle: String,
    pub key: String,
    pub fido2_extensions: Option<String>,
    pub registration_state: PasskeyRegistrationState,
}

impl fmt::Debug for PasskeyCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PasskeyCredential")
            .field("credential_id", &self.credential_id)
            .field("rp_id", &self.rp_id)
            .field("username", &self.username)
            .field("user_display_name", &self.user_display_name)
            .field("user_handle", &self.user_handle)
            .field("key", &"[redacted]")
            .field(
                "fido2_extensions",
                &self.fido2_extensions.as_ref().map(|_| "[present]"),
            )
            .field("registration_state", &self.registration_state)
            .finish()
    }
}

impl PasskeyCredential {
    #[cfg(all(test, feature = "passkey"))]
    pub fn with_registration_state(&self, registration_state: PasskeyRegistrationState) -> Self {
        let mut updated = self.clone();
        updated.registration_state = registration_state;
        updated
    }
}

#[cfg(all(test, not(feature = "passkey")))]
pub const fn passkey_support_available() -> bool {
    cfg!(feature = "passkey")
}

#[cfg(feature = "passkey")]
mod implementation {
    use super::{PasskeyCredential, PasskeyRegistrationState, PASSKEY_ENVELOPE_PREFIX};
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    #[cfg(test)]
    use openssl::ec::{EcGroup, EcKey};
    #[cfg(test)]
    use openssl::nid::Nid;
    use openssl::pkey::{PKey, Private};
    #[cfg(test)]
    use rand::random;
    use serde::{Deserialize, Serialize};
    use serde_json::Value;
    use zeroize::Zeroizing;

    const CXF_PASSKEY_TYPE: &str = "passkey";

    struct PasskeyMetadata<'a> {
        fido2_extensions: Option<&'a str>,
        registration_state: PasskeyRegistrationState,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct PasskeyEnvelope {
        credential_id: String,
        rp_id: String,
        username: String,
        user_display_name: String,
        user_handle: String,
        key: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fido2_extensions: Option<Value>,
        registration_state: EnvelopeRegistrationState,
    }

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(rename_all = "kebab-case")]
    enum EnvelopeRegistrationState {
        Imported,
        GeneratedUnregistered,
        Registered,
    }

    #[cfg(test)]
    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct CxfPasskey<'a> {
        #[serde(rename = "type")]
        credential_type: &'static str,
        credential_id: &'a str,
        rp_id: &'a str,
        username: &'a str,
        user_display_name: &'a str,
        user_handle: &'a str,
        key: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        fido2_extensions: Option<Value>,
    }

    impl TryFrom<PasskeyCredential> for PasskeyEnvelope {
        type Error = String;

        fn try_from(passkey: PasskeyCredential) -> Result<Self, Self::Error> {
            Ok(Self {
                credential_id: passkey.credential_id,
                rp_id: passkey.rp_id,
                username: passkey.username,
                user_display_name: passkey.user_display_name,
                user_handle: passkey.user_handle,
                key: passkey.key,
                fido2_extensions: passkey
                    .fido2_extensions
                    .as_deref()
                    .map(serde_json::from_str)
                    .transpose()
                    .map_err(|err| format!("Invalid passkey FIDO2 extensions: {err}"))?,
                registration_state: passkey.registration_state.into(),
            })
        }
    }

    impl TryFrom<PasskeyEnvelope> for PasskeyCredential {
        type Error = String;

        fn try_from(envelope: PasskeyEnvelope) -> Result<Self, Self::Error> {
            let fido2_extensions = envelope
                .fido2_extensions
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|err| format!("Invalid passkey FIDO2 extensions: {err}"))?;
            normalize_passkey(
                &envelope.credential_id,
                &envelope.rp_id,
                &envelope.username,
                &envelope.user_display_name,
                &envelope.user_handle,
                &envelope.key,
                PasskeyMetadata {
                    fido2_extensions: fido2_extensions.as_deref(),
                    registration_state: envelope.registration_state.into(),
                },
            )
        }
    }

    impl From<PasskeyRegistrationState> for EnvelopeRegistrationState {
        fn from(value: PasskeyRegistrationState) -> Self {
            match value {
                PasskeyRegistrationState::Imported => Self::Imported,
                PasskeyRegistrationState::GeneratedUnregistered => Self::GeneratedUnregistered,
                PasskeyRegistrationState::Registered => Self::Registered,
            }
        }
    }

    impl From<EnvelopeRegistrationState> for PasskeyRegistrationState {
        fn from(value: EnvelopeRegistrationState) -> Self {
            match value {
                EnvelopeRegistrationState::Imported => Self::Imported,
                EnvelopeRegistrationState::GeneratedUnregistered => Self::GeneratedUnregistered,
                EnvelopeRegistrationState::Registered => Self::Registered,
            }
        }
    }

    pub fn encode_passkey_envelope(passkey: &PasskeyCredential) -> Result<String, String> {
        let passkey = normalized_passkey(passkey)?;
        let envelope = PasskeyEnvelope::try_from(passkey)?;
        let json = Zeroizing::new(
            serde_json::to_vec(&envelope)
                .map_err(|err| format!("Failed to serialize passkey data: {err}"))?,
        );
        Ok(format!(
            "{PASSKEY_ENVELOPE_PREFIX}{}",
            URL_SAFE_NO_PAD.encode(json)
        ))
    }

    pub fn decode_passkey_envelope(value: &str) -> Result<PasskeyCredential, String> {
        let encoded = value
            .trim()
            .strip_prefix(PASSKEY_ENVELOPE_PREFIX)
            .ok_or_else(|| "Unsupported passkey envelope.".to_string())?;
        let json = Zeroizing::new(decode_base64url(encoded, "envelope")?);
        let envelope: PasskeyEnvelope = serde_json::from_slice(&json)
            .map_err(|err| format!("Invalid passkey envelope: {err}"))?;
        envelope.try_into()
    }

    pub fn import_cxf_passkey_json(input: &str) -> Result<PasskeyCredential, String> {
        let value: Value =
            serde_json::from_str(input).map_err(|err| format!("Invalid passkey JSON: {err}"))?;
        let passkey = cxf_passkey_value(&value)?;
        passkey_from_cxf_value(passkey, PasskeyRegistrationState::Imported)
    }

    #[cfg(test)]
    pub fn export_cxf_passkey_json(passkey: &PasskeyCredential) -> Result<String, String> {
        let passkey = normalized_passkey(passkey)?;
        let fido2_extensions = passkey
            .fido2_extensions
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(|err| format!("Invalid passkey FIDO2 extensions: {err}"))?;
        let cxf = CxfPasskey {
            credential_type: CXF_PASSKEY_TYPE,
            credential_id: &passkey.credential_id,
            rp_id: &passkey.rp_id,
            username: &passkey.username,
            user_display_name: &passkey.user_display_name,
            user_handle: &passkey.user_handle,
            key: &passkey.key,
            fido2_extensions,
        };
        serde_json::to_string_pretty(&cxf)
            .map_err(|err| format!("Failed to export passkey JSON: {err}"))
    }

    #[cfg(test)]
    pub fn generate_passkey_credential(
        rp_id: &str,
        username: &str,
        user_display_name: &str,
    ) -> Result<PasskeyCredential, String> {
        let key = generate_p256_private_key()?;
        normalize_passkey(
            &encode_base64url(&random::<[u8; 32]>()),
            rp_id,
            username,
            user_display_name,
            &encode_base64url(&random::<[u8; 32]>()),
            &key,
            PasskeyMetadata {
                fido2_extensions: None,
                registration_state: PasskeyRegistrationState::GeneratedUnregistered,
            },
        )
    }

    fn normalized_passkey(passkey: &PasskeyCredential) -> Result<PasskeyCredential, String> {
        normalize_passkey(
            &passkey.credential_id,
            &passkey.rp_id,
            &passkey.username,
            &passkey.user_display_name,
            &passkey.user_handle,
            &passkey.key,
            PasskeyMetadata {
                fido2_extensions: passkey.fido2_extensions.as_deref(),
                registration_state: passkey.registration_state,
            },
        )
    }

    fn cxf_passkey_value(value: &Value) -> Result<&Value, String> {
        if is_cxf_passkey(value) {
            return Ok(value);
        }

        let mut passkeys = Vec::new();
        collect_cxf_passkeys(value, &mut passkeys);
        match passkeys.as_slice() {
            [passkey] => Ok(*passkey),
            [] => Err("Choose a JSON object containing one passkey credential.".to_string()),
            _ => Err("Choose a JSON object containing exactly one passkey credential.".to_string()),
        }
    }

    fn collect_cxf_passkeys<'a>(value: &'a Value, passkeys: &mut Vec<&'a Value>) {
        let Some(object) = value.as_object() else {
            return;
        };

        for key in ["passkey", "credential"] {
            if let Some(candidate) = object.get(key) {
                if is_cxf_passkey(candidate) {
                    passkeys.push(candidate);
                }
            }
        }

        if let Some(credentials) = object.get("credentials").and_then(Value::as_array) {
            passkeys.extend(
                credentials
                    .iter()
                    .filter(|credential| is_cxf_passkey(credential)),
            );
        }

        for key in ["items", "accounts"] {
            if let Some(children) = object.get(key).and_then(Value::as_array) {
                for child in children {
                    collect_cxf_passkeys(child, passkeys);
                }
            }
        }
    }

    fn is_cxf_passkey(value: &Value) -> bool {
        value
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|credential_type| credential_type == CXF_PASSKEY_TYPE)
    }

    fn passkey_from_cxf_value(
        value: &Value,
        registration_state: PasskeyRegistrationState,
    ) -> Result<PasskeyCredential, String> {
        let username = required_string(value, &["username", "userName"], "username")?;
        let display_name = optional_string(value, &["userDisplayName", "displayName"])
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(username);
        let fido2_extensions = value
            .get("fido2Extensions")
            .map(serde_json::to_string)
            .transpose()
            .map_err(|err| format!("Invalid passkey FIDO2 extensions: {err}"))?;

        normalize_passkey(
            required_string(value, &["credentialId"], "credential ID")?,
            required_string(value, &["rpId"], "RP ID")?,
            username,
            display_name,
            required_string(value, &["userHandle"], "user handle")?,
            required_string(value, &["key"], "private key")?,
            PasskeyMetadata {
                fido2_extensions: fido2_extensions.as_deref(),
                registration_state,
            },
        )
    }

    fn required_string<'a>(
        value: &'a Value,
        keys: &[&str],
        label: &str,
    ) -> Result<&'a str, String> {
        optional_string(value, keys)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .ok_or_else(|| format!("The passkey is missing a {label}."))
    }

    fn optional_string<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
        keys.iter().find_map(|key| value.get(*key)?.as_str())
    }

    fn normalize_passkey(
        credential_id: &str,
        rp_id: &str,
        username: &str,
        user_display_name: &str,
        user_handle: &str,
        key: &str,
        metadata: PasskeyMetadata<'_>,
    ) -> Result<PasskeyCredential, String> {
        let rp_id = normalize_rp_id(rp_id)?;
        let username = username.trim();
        if username.is_empty() {
            return Err("Enter a passkey username.".to_string());
        }
        let user_display_name = user_display_name.trim();
        let user_display_name = if user_display_name.is_empty() {
            username
        } else {
            user_display_name
        };

        Ok(PasskeyCredential {
            credential_id: normalize_base64url(credential_id, "credential ID")?,
            rp_id,
            username: username.to_string(),
            user_display_name: user_display_name.to_string(),
            user_handle: normalize_base64url(user_handle, "user handle")?,
            key: normalize_private_key(key)?,
            fido2_extensions: normalize_fido2_extensions(metadata.fido2_extensions)?,
            registration_state: metadata.registration_state,
        })
    }

    fn normalize_fido2_extensions(value: Option<&str>) -> Result<Option<String>, String> {
        let Some(value) = value else {
            return Ok(None);
        };
        let extensions: Value = serde_json::from_str(value)
            .map_err(|err| format!("Invalid passkey FIDO2 extensions: {err}"))?;
        if !extensions.is_object() {
            return Err("Passkey FIDO2 extensions must be a JSON object.".to_string());
        }
        serde_json::to_string(&extensions)
            .map(Some)
            .map_err(|err| format!("Invalid passkey FIDO2 extensions: {err}"))
    }

    fn normalize_rp_id(rp_id: &str) -> Result<String, String> {
        let rp_id = rp_id.trim().to_ascii_lowercase();
        let rp_id = rp_id.strip_suffix('.').unwrap_or(&rp_id);
        if rp_id.is_empty() || rp_id.len() > 253 || !rp_id.is_ascii() {
            return Err("Enter a valid passkey RP ID.".to_string());
        }

        let valid = rp_id.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                && label
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && label
                    .as_bytes()
                    .last()
                    .is_some_and(u8::is_ascii_alphanumeric)
        });
        if !valid {
            return Err("Enter a valid passkey RP ID.".to_string());
        }

        Ok(rp_id.to_string())
    }

    fn normalize_base64url(value: &str, label: &str) -> Result<String, String> {
        let decoded = decode_base64url(value, label)?;
        if decoded.is_empty() {
            return Err(format!("The passkey {label} is empty."));
        }

        Ok(encode_base64url(&decoded))
    }

    fn normalize_private_key(value: &str) -> Result<String, String> {
        let decoded = Zeroizing::new(decode_base64url(value, "private key")?);
        let pkey = validate_private_key(&decoded)?;
        let pkcs8 = Zeroizing::new(
            pkey.private_key_to_pkcs8()
                .map_err(|err| format!("Failed to normalize passkey private key: {err}"))?,
        );
        Ok(encode_base64url(&pkcs8))
    }

    fn decode_base64url(value: &str, label: &str) -> Result<Vec<u8>, String> {
        let value = value.trim();
        if value.is_empty() || value.contains(['+', '/', '=']) {
            return Err(format!("The passkey {label} must use unpadded base64url."));
        }
        URL_SAFE_NO_PAD
            .decode(value)
            .map_err(|err| format!("Invalid passkey {label}: {err}"))
    }

    fn encode_base64url(bytes: &[u8]) -> String {
        URL_SAFE_NO_PAD.encode(bytes)
    }

    #[cfg(test)]
    fn generate_p256_private_key() -> Result<String, String> {
        let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1)
            .map_err(|err| format!("Failed to prepare passkey key generation: {err}"))?;
        let ec_key = EcKey::generate(&group)
            .map_err(|err| format!("Failed to generate passkey key material: {err}"))?;
        let pkey = PKey::from_ec_key(ec_key)
            .map_err(|err| format!("Failed to prepare passkey key material: {err}"))?;
        let der = Zeroizing::new(
            pkey.private_key_to_pkcs8()
                .map_err(|err| format!("Failed to encode passkey key material: {err}"))?,
        );
        Ok(encode_base64url(&der))
    }

    fn validate_private_key(der: &[u8]) -> Result<PKey<Private>, String> {
        PKey::private_key_from_pkcs8(der)
            .map_err(|_| "Enter a valid PKCS#8 passkey private key.".to_string())
    }
}

#[cfg(not(feature = "passkey"))]
mod implementation {
    use super::PasskeyCredential;

    const UNSUPPORTED: &str = "This build does not include passkey support.";

    #[cfg(test)]
    pub fn encode_passkey_envelope(_passkey: &PasskeyCredential) -> Result<String, String> {
        Err(UNSUPPORTED.to_string())
    }

    pub fn decode_passkey_envelope(_value: &str) -> Result<PasskeyCredential, String> {
        Err(UNSUPPORTED.to_string())
    }

    #[cfg(test)]
    pub fn import_cxf_passkey_json(_input: &str) -> Result<PasskeyCredential, String> {
        Err(UNSUPPORTED.to_string())
    }

    #[cfg(test)]
    pub fn export_cxf_passkey_json(_passkey: &PasskeyCredential) -> Result<String, String> {
        Err(UNSUPPORTED.to_string())
    }

    #[cfg(test)]
    pub fn generate_passkey_credential(
        _rp_id: &str,
        _username: &str,
        _user_display_name: &str,
    ) -> Result<PasskeyCredential, String> {
        Err(UNSUPPORTED.to_string())
    }
}

pub use implementation::decode_passkey_envelope;
#[cfg(feature = "passkey")]
pub use implementation::{encode_passkey_envelope, import_cxf_passkey_json};
#[cfg(all(test, not(feature = "passkey")))]
pub use implementation::{encode_passkey_envelope, import_cxf_passkey_json};
#[cfg(test)]
pub use implementation::{export_cxf_passkey_json, generate_passkey_credential};

#[cfg(all(test, feature = "passkey"))]
mod tests {
    use super::{
        decode_passkey_envelope, encode_passkey_envelope, export_cxf_passkey_json,
        generate_passkey_credential, import_cxf_passkey_json, PasskeyCredential,
        PasskeyRegistrationState, PASSKEY_ENVELOPE_PREFIX,
    };
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    use openssl::ec::{EcGroup, EcKey};
    use openssl::nid::Nid;
    use openssl::pkey::PKey;
    use serde_json::{json, Value};

    fn exported_value(passkey: &PasskeyCredential) -> Value {
        serde_json::from_str(
            &export_cxf_passkey_json(passkey).expect("export generated passkey as CXF JSON"),
        )
        .expect("parse exported CXF JSON")
    }

    #[test]
    fn passkey_envelopes_round_trip_all_registration_states() {
        let generated =
            generate_passkey_credential("Example.COM", "alice", "").expect("generate passkey");

        for state in [
            PasskeyRegistrationState::GeneratedUnregistered,
            PasskeyRegistrationState::Registered,
            PasskeyRegistrationState::Imported,
        ] {
            let passkey = generated.with_registration_state(state);
            let encoded = encode_passkey_envelope(&passkey).expect("encode passkey");
            assert!(encoded.starts_with(PASSKEY_ENVELOPE_PREFIX));

            let decoded = decode_passkey_envelope(&encoded).expect("decode passkey");
            assert_eq!(decoded, passkey);
            assert_eq!(decoded.registration_state.as_str(), state.as_str());
        }
    }

    #[test]
    fn generated_passkeys_use_normalized_rp_ids_and_pkcs8_p256_keys() {
        let passkey =
            generate_passkey_credential("Example.COM.", "alice", "").expect("generate passkey");

        assert_eq!(passkey.rp_id, "example.com");
        assert_eq!(passkey.user_display_name, "alice");
        assert_eq!(
            passkey.registration_state,
            PasskeyRegistrationState::GeneratedUnregistered
        );
        assert!(!passkey.credential_id.is_empty());
        assert!(!passkey.user_handle.is_empty());

        let key_bytes = URL_SAFE_NO_PAD.decode(&passkey.key).expect("decode key");
        let key = PKey::private_key_from_pkcs8(&key_bytes).expect("parse PKCS#8 key");
        let ec_key = key.ec_key().expect("extract EC key");
        assert_eq!(ec_key.group().curve_name(), Some(Nid::X9_62_PRIME256V1));
    }

    #[test]
    fn cxf_export_is_a_standalone_standard_passkey_object() {
        let generated =
            generate_passkey_credential("example.com", "alice", "Alice").expect("generate passkey");
        let value = exported_value(&generated);

        assert_eq!(value.get("type").and_then(Value::as_str), Some("passkey"));
        assert_eq!(
            value.get("credentialId").and_then(Value::as_str),
            Some(generated.credential_id.as_str())
        );
        assert!(value.get("registrationState").is_none());

        let key = value.get("key").and_then(Value::as_str).expect("CXF key");
        let key = URL_SAFE_NO_PAD.decode(key).expect("decode CXF key");
        PKey::private_key_from_pkcs8(&key).expect("CXF key must be PKCS#8");
    }

    #[test]
    fn cxf_standalone_passkeys_import_as_zero_counter_compatible_credentials() {
        let generated =
            generate_passkey_credential("example.com", "alice", "Alice").expect("generate passkey");
        let exported = export_cxf_passkey_json(&generated).expect("export passkey");
        let imported = import_cxf_passkey_json(&exported).expect("import passkey");

        assert_eq!(
            imported.registration_state,
            PasskeyRegistrationState::Imported
        );
        assert_eq!(imported.credential_id, generated.credential_id);
        assert_eq!(imported.rp_id, generated.rp_id);
        assert_eq!(imported.key, generated.key);
    }

    #[test]
    fn cxf_item_and_account_containers_accept_exactly_one_passkey() {
        let generated =
            generate_passkey_credential("example.com", "alice", "Alice").expect("generate passkey");
        let passkey = exported_value(&generated);
        let item = json!({
            "credentials": [
                {"type": "basic-auth", "username": "alice", "password": "secret"},
                passkey.clone()
            ]
        });
        let account = json!({"accounts": [{"items": [item.clone()]}]});

        for container in [item, account] {
            let imported = import_cxf_passkey_json(&container.to_string())
                .expect("import one passkey from container");
            assert_eq!(imported.credential_id, generated.credential_id);
        }

        let ambiguous = json!({"credentials": [passkey.clone(), passkey]});
        assert!(import_cxf_passkey_json(&ambiguous.to_string()).is_err());
    }

    #[test]
    fn cxf_import_requires_the_passkey_type_discriminator() {
        let generated =
            generate_passkey_credential("example.com", "alice", "Alice").expect("generate passkey");
        let mut value = exported_value(&generated);
        value
            .as_object_mut()
            .expect("passkey object")
            .remove("type");

        assert!(import_cxf_passkey_json(&value.to_string()).is_err());
        value["type"] = Value::String("basic-auth".to_string());
        assert!(import_cxf_passkey_json(&value.to_string()).is_err());
    }

    #[test]
    fn invalid_rp_ids_and_non_url_safe_binary_fields_are_rejected() {
        for rp_id in [
            "",
            ".example.com",
            "-example.com",
            "example..com",
            "exa_mple.com",
        ] {
            assert!(generate_passkey_credential(rp_id, "alice", "Alice").is_err());
        }

        let generated =
            generate_passkey_credential("example.com", "alice", "Alice").expect("generate passkey");
        let mut value = exported_value(&generated);
        value["credentialId"] = Value::String("YWJjZA==".to_string());
        assert!(import_cxf_passkey_json(&value.to_string()).is_err());
        value["credentialId"] = Value::String(generated.credential_id);
        value["userHandle"] = Value::String("not/urlsafe".to_string());
        assert!(import_cxf_passkey_json(&value.to_string()).is_err());
    }

    #[test]
    fn cxf_import_rejects_key_specific_der_and_accepts_generic_pkcs8() {
        let generated =
            generate_passkey_credential("example.com", "alice", "Alice").expect("generate passkey");
        let mut value = exported_value(&generated);

        let p256_group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1).expect("P-256 group");
        let key_specific = EcKey::generate(&p256_group)
            .expect("generate P-256 key")
            .private_key_to_der()
            .expect("serialize SEC1 key");
        value["key"] = Value::String(URL_SAFE_NO_PAD.encode(key_specific));
        assert!(import_cxf_passkey_json(&value.to_string()).is_err());

        let p384_group = EcGroup::from_curve_name(Nid::SECP384R1).expect("P-384 group");
        let p384 = PKey::from_ec_key(EcKey::generate(&p384_group).expect("generate P-384 key"))
            .expect("wrap P-384 key")
            .private_key_to_pkcs8()
            .expect("serialize P-384 PKCS#8 key");
        value["key"] = Value::String(URL_SAFE_NO_PAD.encode(p384));
        let imported = import_cxf_passkey_json(&value.to_string()).expect("import P-384 PKCS#8");
        let imported_key = URL_SAFE_NO_PAD
            .decode(imported.key)
            .expect("decode imported key");
        let imported_key = PKey::private_key_from_pkcs8(&imported_key).expect("parse imported key");
        let imported_ec_key = imported_key.ec_key().expect("extract imported EC key");
        assert_eq!(imported_ec_key.group().curve_name(), Some(Nid::SECP384R1));
    }

    #[test]
    fn cxf_fido2_extensions_survive_import_and_pass_file_round_trips() {
        let generated =
            generate_passkey_credential("example.com", "alice", "Alice").expect("generate passkey");
        let mut value = exported_value(&generated);
        let extensions = json!({
            "credBlob": "AQID",
            "payments": true,
            "futureExtension": {"opaque": "value"}
        });
        value["fido2Extensions"] = extensions.clone();

        let imported = import_cxf_passkey_json(&value.to_string()).expect("import extensions");
        let encoded = encode_passkey_envelope(&imported).expect("encode passkey envelope");
        let decoded = decode_passkey_envelope(&encoded).expect("decode passkey envelope");
        assert_eq!(decoded, imported);

        let exported = exported_value(&decoded);
        assert_eq!(exported["fido2Extensions"], extensions);
    }

    #[test]
    fn cxf_fido2_extensions_must_be_an_object() {
        let generated =
            generate_passkey_credential("example.com", "alice", "Alice").expect("generate passkey");
        let mut value = exported_value(&generated);

        for invalid in [Value::Null, json!([]), json!("extension")] {
            value["fido2Extensions"] = invalid;
            assert!(import_cxf_passkey_json(&value.to_string()).is_err());
        }
    }

    #[test]
    fn passkey_debug_output_does_not_expose_private_key_material() {
        let mut generated =
            generate_passkey_credential("example.com", "alice", "Alice").expect("generate passkey");
        generated.fido2_extensions =
            Some(json!({"hmacCredentials": {"secret": "extension-secret"}}).to_string());
        let debug = format!("{generated:?}");

        assert!(!debug.contains(&generated.key));
        assert!(!debug.contains("extension-secret"));
        assert!(debug.contains("[redacted]"));
    }

    #[test]
    fn malformed_envelopes_and_invalid_public_values_are_rejected() {
        assert!(decode_passkey_envelope("keycord-passkey-v1:not-valid").is_err());
        assert!(decode_passkey_envelope("other-prefix:abc").is_err());

        let generated =
            generate_passkey_credential("example.com", "alice", "Alice").expect("generate passkey");
        let invalid = PasskeyCredential {
            rp_id: "invalid/rp".to_string(),
            ..generated
        };
        assert!(encode_passkey_envelope(&invalid).is_err());
        assert!(export_cxf_passkey_json(&invalid).is_err());
    }
}

#[cfg(all(test, not(feature = "passkey")))]
mod disabled_tests {
    use super::{
        decode_passkey_envelope, encode_passkey_envelope, export_cxf_passkey_json,
        generate_passkey_credential, import_cxf_passkey_json, passkey_support_available,
        PasskeyCredential, PasskeyRegistrationState,
    };

    #[test]
    fn passkey_operations_are_explicitly_unavailable_without_the_feature() {
        let passkey = PasskeyCredential {
            credential_id: "credential-id".to_string(),
            rp_id: "example.com".to_string(),
            username: "alice".to_string(),
            user_display_name: "Alice".to_string(),
            user_handle: "user-handle".to_string(),
            key: "private-key".to_string(),
            fido2_extensions: None,
            registration_state: PasskeyRegistrationState::Imported,
        };

        assert!(!passkey_support_available());
        assert!(generate_passkey_credential("example.com", "alice", "Alice").is_err());
        assert!(encode_passkey_envelope(&passkey).is_err());
        assert!(decode_passkey_envelope("keycord-passkey-v1:value").is_err());
        assert!(import_cxf_passkey_json(r#"{"type":"passkey"}"#).is_err());
        assert!(export_cxf_passkey_json(&passkey).is_err());
    }
}
