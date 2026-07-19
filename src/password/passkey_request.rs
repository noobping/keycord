#![cfg(feature = "passkey")]

use super::passkey::{import_cxf_passkey_json, PasskeyCredential};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::ffi::OsString;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
#[cfg(target_os = "linux")]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use thiserror::Error;
use zeroize::Zeroizing;

pub const CXP_EXPORT_REQUEST_VERSION: u16 = 0;
pub const MAX_PASSKEY_REQUEST_BYTES: usize = 256 * 1024;

const CXP_INDIRECT_MODE: &str = "indirect";
const CXF_PASSKEY_CREDENTIAL_TYPE: &str = "passkey";
const DEFLATE_ARCHIVE: &str = "deflate";
const MAX_LIST_ITEMS: usize = 32;
const MAX_IMPORTER_BYTES: usize = 1_024;
const MAX_IDENTIFIER_BYTES: usize = 256;
const MAX_JWK_MEMBERS: usize = 32;

#[derive(Clone, Debug, PartialEq)]
pub struct PasskeyExportRequestFile {
    pub source_path: PathBuf,
    pub suggested_response_path: PathBuf,
    pub request: CxpExportRequest,
}

#[derive(Clone, Debug, PartialEq)]
pub enum OpenedPasskeyFile {
    ExportRequest(PasskeyExportRequestFile),
    Credential(PasskeyCredential),
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CxpExportRequest {
    pub version: u16,
    pub hpke: Vec<CxpHpkeParameters>,
    pub archive: Vec<String>,
    pub mode: String,
    pub importer: String,
    pub credential_types: Option<Vec<String>>,
    pub known_extensions: Option<Vec<String>>,
}

#[derive(Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CxpHpkeParameters {
    pub mode: String,
    pub kem: u16,
    pub kdf: u16,
    pub aead: u16,
    pub key: Option<Map<String, Value>>,
}

impl fmt::Debug for CxpHpkeParameters {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CxpHpkeParameters")
            .field("mode", &self.mode)
            .field("kem", &self.kem)
            .field("kdf", &self.kdf)
            .field("aead", &self.aead)
            .field("key", &self.key.as_ref().map(|_| "[present]"))
            .finish()
    }
}

#[derive(Debug, Error)]
pub enum PasskeyRequestError {
    #[error("The file is not a CXP passkey export request.")]
    NotPasskeyRequest,
    #[error("The passkey request source is not a regular local file: {path:?}")]
    InvalidSource { path: PathBuf },
    #[error("Failed to read the passkey request file {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("The passkey request is larger than the {limit}-byte limit.")]
    TooLarge { limit: usize },
    #[error("The passkey request contains malformed JSON: {0}")]
    MalformedJson(#[source] serde_json::Error),
    #[error("The passkey request is malformed: {0}")]
    Malformed(String),
    #[error("CXP export request version {0} is not supported.")]
    UnsupportedVersion(u16),
    #[error("CXP response mode {0:?} is not supported for an opened request file.")]
    UnsupportedMode(String),
    #[error("The CXP request does not offer the supported deflate archive format.")]
    UnsupportedArchive,
    #[error("The CXP request does not contain a recognized HPKE mode.")]
    UnsupportedHpke,
}

impl PasskeyRequestError {
    pub const fn is_not_passkey_request(&self) -> bool {
        matches!(self, Self::NotPasskeyRequest)
    }
}

/// Reads and structurally checks a bounded CXP export-request JSON file from the local filesystem.
///
/// Symbolic links and non-regular files are rejected. The response path is only a safe,
/// deterministic suggestion; response writers must still create the file without replacing an
/// existing file.
#[cfg(test)]
pub fn read_passkey_export_request(
    source_path: impl AsRef<Path>,
) -> Result<PasskeyExportRequestFile, PasskeyRequestError> {
    let source_path = source_path.as_ref();
    let bytes = Zeroizing::new(read_bounded_regular_file(source_path)?);
    let request = parse_passkey_export_request(&bytes)?;
    let suggested_response_path = sibling_response_path(source_path)?;

    Ok(PasskeyExportRequestFile {
        source_path: source_path.to_path_buf(),
        suggested_response_path,
        request,
    })
}

pub fn read_opened_passkey_file(
    source_path: impl AsRef<Path>,
) -> Result<OpenedPasskeyFile, PasskeyRequestError> {
    let source_path = source_path.as_ref();
    let bytes = Zeroizing::new(read_bounded_regular_file(source_path)?);
    match parse_passkey_export_request(&bytes) {
        Ok(request) => Ok(OpenedPasskeyFile::ExportRequest(PasskeyExportRequestFile {
            source_path: source_path.to_path_buf(),
            suggested_response_path: sibling_response_path(source_path)?,
            request,
        })),
        Err(PasskeyRequestError::NotPasskeyRequest) => {
            let input =
                std::str::from_utf8(&bytes).map_err(|_| PasskeyRequestError::NotPasskeyRequest)?;
            match import_cxf_passkey_json(input) {
                Ok(credential) => Ok(OpenedPasskeyFile::Credential(credential)),
                Err(error) if contains_cxf_passkey_type(&bytes) => {
                    Err(PasskeyRequestError::Malformed(error))
                }
                Err(_) => Err(PasskeyRequestError::NotPasskeyRequest),
            }
        }
        Err(error) => Err(error),
    }
}

/// Parses a CXP request without performing filesystem access.
///
/// Valid JSON that does not resemble a CXP export request, and valid CXP requests that do not ask
/// for passkeys, return [`PasskeyRequestError::NotPasskeyRequest`]. Once the input is recognizable
/// as a CXP request, syntax/schema errors and unsupported protocol choices are reported distinctly.
pub fn parse_passkey_export_request(input: &[u8]) -> Result<CxpExportRequest, PasskeyRequestError> {
    if input.len() > MAX_PASSKEY_REQUEST_BYTES {
        return Err(PasskeyRequestError::TooLarge {
            limit: MAX_PASSKEY_REQUEST_BYTES,
        });
    }

    let value: Value = match serde_json::from_slice(input) {
        Ok(value) => value,
        Err(err) if starts_like_json_object(input) => {
            return Err(PasskeyRequestError::MalformedJson(err));
        }
        Err(_) => return Err(PasskeyRequestError::NotPasskeyRequest),
    };

    let Some(object) = value.as_object() else {
        return Err(PasskeyRequestError::NotPasskeyRequest);
    };
    if !looks_like_cxp_export_request(object) {
        return Err(PasskeyRequestError::NotPasskeyRequest);
    }

    let request: CxpExportRequest = serde_json::from_value(value)
        .map_err(|err| PasskeyRequestError::Malformed(err.to_string()))?;
    validate_request(&request)?;
    Ok(request)
}

/// Derives a response filename beside the request without using any request-controlled JSON value.
pub fn sibling_response_path(source_path: &Path) -> Result<PathBuf, PasskeyRequestError> {
    let file_name = source_path
        .file_name()
        .filter(|name| *name != "." && *name != "..")
        .ok_or_else(|| PasskeyRequestError::InvalidSource {
            path: source_path.to_path_buf(),
        })?;
    let stem = source_path.file_stem().unwrap_or(file_name);
    let mut response_name = OsString::from(stem);
    response_name.push(".response.json");
    let response_path = source_path.with_file_name(response_name);

    if response_path == source_path {
        return Err(PasskeyRequestError::InvalidSource {
            path: source_path.to_path_buf(),
        });
    }
    Ok(response_path)
}

fn read_bounded_regular_file(path: &Path) -> Result<Vec<u8>, PasskeyRequestError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| PasskeyRequestError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    if !metadata.file_type().is_file() {
        return Err(PasskeyRequestError::InvalidSource {
            path: path.to_path_buf(),
        });
    }
    if metadata.len() > MAX_PASSKEY_REQUEST_BYTES as u64 {
        return Err(PasskeyRequestError::TooLarge {
            limit: MAX_PASSKEY_REQUEST_BYTES,
        });
    }

    let file = open_request_file(path).map_err(|source| PasskeyRequestError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let opened_metadata = file
        .metadata()
        .map_err(|source| PasskeyRequestError::Read {
            path: path.to_path_buf(),
            source,
        })?;
    if !opened_metadata.is_file() {
        return Err(PasskeyRequestError::InvalidSource {
            path: path.to_path_buf(),
        });
    }
    if opened_metadata.len() > MAX_PASSKEY_REQUEST_BYTES as u64 {
        return Err(PasskeyRequestError::TooLarge {
            limit: MAX_PASSKEY_REQUEST_BYTES,
        });
    }

    let capacity = opened_metadata.len().min(MAX_PASSKEY_REQUEST_BYTES as u64) as usize;
    let mut bytes = Vec::with_capacity(capacity);
    file.take(MAX_PASSKEY_REQUEST_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|source| PasskeyRequestError::Read {
            path: path.to_path_buf(),
            source,
        })?;
    if bytes.len() > MAX_PASSKEY_REQUEST_BYTES {
        return Err(PasskeyRequestError::TooLarge {
            limit: MAX_PASSKEY_REQUEST_BYTES,
        });
    }
    Ok(bytes)
}

fn open_request_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    options.open(path)
}

fn starts_like_json_object(input: &[u8]) -> bool {
    input
        .iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace())
        == Some(b'{')
}

fn contains_cxf_passkey_type(input: &[u8]) -> bool {
    fn contains(value: &Value) -> bool {
        match value {
            Value::Array(values) => values.iter().any(contains),
            Value::Object(object) => {
                object.get("type").and_then(Value::as_str) == Some("passkey")
                    || object.values().any(contains)
            }
            _ => false,
        }
    }

    serde_json::from_slice(input).is_ok_and(|value| contains(&value))
}

fn looks_like_cxp_export_request(object: &Map<String, Value>) -> bool {
    if object.contains_key("hpke") {
        return true;
    }

    let core_fields = ["version", "archive", "mode", "importer"]
        .into_iter()
        .filter(|field| object.contains_key(*field))
        .count();
    core_fields >= 3
        || (core_fields >= 1
            && (object.contains_key("credentialTypes") || object.contains_key("knownExtensions")))
}

fn validate_request(request: &CxpExportRequest) -> Result<(), PasskeyRequestError> {
    if request.version != CXP_EXPORT_REQUEST_VERSION {
        return Err(PasskeyRequestError::UnsupportedVersion(request.version));
    }
    validate_text(&request.importer, "importer", MAX_IMPORTER_BYTES)?;
    validate_rp_id(&request.importer, "importer")?;

    if request.mode != CXP_INDIRECT_MODE {
        return Err(PasskeyRequestError::UnsupportedMode(request.mode.clone()));
    }

    validate_string_list(&request.archive, "archive", false)?;
    if !request
        .archive
        .iter()
        .any(|archive| archive == DEFLATE_ARCHIVE)
    {
        return Err(PasskeyRequestError::UnsupportedArchive);
    }

    validate_hpke(&request.hpke)?;

    if let Some(credential_types) = &request.credential_types {
        validate_string_list(credential_types, "credentialTypes", true)?;
        if !credential_types
            .iter()
            .any(|credential_type| credential_type == CXF_PASSKEY_CREDENTIAL_TYPE)
        {
            return Err(PasskeyRequestError::NotPasskeyRequest);
        }
    }

    if let Some(known_extensions) = &request.known_extensions {
        validate_string_list(known_extensions, "knownExtensions", true)?;
    }

    Ok(())
}

fn validate_hpke(hpke: &[CxpHpkeParameters]) -> Result<(), PasskeyRequestError> {
    if hpke.is_empty() {
        return Err(PasskeyRequestError::Malformed(
            "hpke must contain at least one parameter set".to_string(),
        ));
    }
    if hpke.len() > MAX_LIST_ITEMS {
        return Err(PasskeyRequestError::Malformed(format!(
            "hpke contains more than {MAX_LIST_ITEMS} parameter sets"
        )));
    }

    let mut has_recognized_mode = false;
    for parameters in hpke {
        validate_text(&parameters.mode, "hpke.mode", MAX_IDENTIFIER_BYTES)?;
        let recognized_mode = matches!(
            parameters.mode.as_str(),
            "base" | "psk" | "auth" | "auth-psk"
        );
        has_recognized_mode |= recognized_mode;
        if !recognized_mode || !hpke_suite_is_known(parameters) {
            continue;
        }

        if let Some(key) = &parameters.key {
            validate_jwk(key, parameters.kem)?;
        } else if matches!(parameters.mode.as_str(), "base" | "auth") {
            return Err(PasskeyRequestError::Malformed(format!(
                "hpke mode {:?} requires a public JWK",
                parameters.mode
            )));
        }
    }

    if !has_recognized_mode {
        return Err(PasskeyRequestError::UnsupportedHpke);
    }
    Ok(())
}

const fn hpke_suite_is_known(parameters: &CxpHpkeParameters) -> bool {
    matches!(parameters.kem, 0x0010..=0x0012 | 0x0020..=0x0021)
        && matches!(parameters.kdf, 0x0001..=0x0003)
        && matches!(parameters.aead, 0x0001..=0x0003)
}

fn validate_jwk(key: &Map<String, Value>, kem: u16) -> Result<(), PasskeyRequestError> {
    if key.is_empty() {
        return Err(PasskeyRequestError::Malformed(
            "hpke.key must not be an empty JWK".to_string(),
        ));
    }
    if key.len() > MAX_JWK_MEMBERS {
        return Err(PasskeyRequestError::Malformed(format!(
            "hpke.key contains more than {MAX_JWK_MEMBERS} members"
        )));
    }

    for member in key.keys() {
        validate_text(member, "hpke.key member", MAX_IDENTIFIER_BYTES)?;
    }
    let key_type = key.get("kty").and_then(Value::as_str).ok_or_else(|| {
        PasskeyRequestError::Malformed("hpke.key must contain a string kty member".to_string())
    })?;
    validate_text(key_type, "hpke.key.kty", MAX_IDENTIFIER_BYTES)?;
    if key.contains_key("d") {
        return Err(PasskeyRequestError::Malformed(
            "hpke.key must contain only public key material".to_string(),
        ));
    }

    let curve = required_jwk_string(key, "crv")?;
    let x = required_jwk_bytes(key, "x")?;
    let (expected_key_type, expected_curve, expected_coordinate_bytes) = match kem {
        0x0010 => ("EC", "P-256", Some(32)),
        0x0011 => ("EC", "P-384", Some(48)),
        0x0012 => ("EC", "P-521", Some(66)),
        0x0020 => ("OKP", "X25519", Some(32)),
        0x0021 => ("OKP", "X448", Some(56)),
        _ => (key_type, curve, None),
    };
    if key_type != expected_key_type || curve != expected_curve {
        return Err(PasskeyRequestError::Malformed(format!(
            "hpke.key {key_type}/{curve} does not match KEM {kem}"
        )));
    }
    if expected_coordinate_bytes.is_some_and(|expected| x.len() != expected) {
        return Err(PasskeyRequestError::Malformed(format!(
            "hpke.key.x has the wrong length for KEM {kem}"
        )));
    }

    if key_type == "EC" {
        let y = required_jwk_bytes(key, "y")?;
        if y.len() != x.len() {
            return Err(PasskeyRequestError::Malformed(
                "hpke.key EC coordinates must have equal lengths".to_string(),
            ));
        }
    } else if key_type != "OKP" {
        return Err(PasskeyRequestError::Malformed(format!(
            "hpke.key uses unsupported key type {key_type:?}"
        )));
    }

    Ok(())
}

fn required_jwk_string<'a>(
    key: &'a Map<String, Value>,
    member: &str,
) -> Result<&'a str, PasskeyRequestError> {
    let value = key.get(member).and_then(Value::as_str).ok_or_else(|| {
        PasskeyRequestError::Malformed(format!("hpke.key must contain a string {member} member"))
    })?;
    validate_text(value, &format!("hpke.key.{member}"), MAX_IDENTIFIER_BYTES)?;
    Ok(value)
}

fn required_jwk_bytes(
    key: &Map<String, Value>,
    member: &str,
) -> Result<Vec<u8>, PasskeyRequestError> {
    let value = required_jwk_string(key, member)?;
    if value.contains(['+', '/', '=']) {
        return Err(PasskeyRequestError::Malformed(format!(
            "hpke.key.{member} must use unpadded base64url"
        )));
    }
    let decoded = URL_SAFE_NO_PAD.decode(value).map_err(|_| {
        PasskeyRequestError::Malformed(format!("hpke.key.{member} must use unpadded base64url"))
    })?;
    if decoded.is_empty() {
        return Err(PasskeyRequestError::Malformed(format!(
            "hpke.key.{member} must not be empty"
        )));
    }
    Ok(decoded)
}

fn validate_rp_id(value: &str, field: &str) -> Result<(), PasskeyRequestError> {
    if value.len() > 253 || !value.is_ascii() {
        return Err(PasskeyRequestError::Malformed(format!(
            "{field} must be a valid relying-party ID"
        )));
    }
    let valid = value.split('.').all(|label| {
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
        return Err(PasskeyRequestError::Malformed(format!(
            "{field} must be a valid relying-party ID"
        )));
    }
    Ok(())
}

fn validate_string_list(
    values: &[String],
    field: &str,
    empty_is_valid: bool,
) -> Result<(), PasskeyRequestError> {
    if values.is_empty() && !empty_is_valid {
        return Err(PasskeyRequestError::Malformed(format!(
            "{field} must contain at least one value"
        )));
    }
    if values.len() > MAX_LIST_ITEMS {
        return Err(PasskeyRequestError::Malformed(format!(
            "{field} contains more than {MAX_LIST_ITEMS} values"
        )));
    }
    for value in values {
        validate_text(value, field, MAX_IDENTIFIER_BYTES)?;
    }
    Ok(())
}

fn validate_text(value: &str, field: &str, max_bytes: usize) -> Result<(), PasskeyRequestError> {
    if value.is_empty() {
        return Err(PasskeyRequestError::Malformed(format!(
            "{field} must not be empty"
        )));
    }
    if value.len() > max_bytes {
        return Err(PasskeyRequestError::Malformed(format!(
            "{field} is longer than {max_bytes} bytes"
        )));
    }
    if value.trim() != value || value.chars().any(char::is_control) {
        return Err(PasskeyRequestError::Malformed(format!(
            "{field} contains invalid whitespace or control characters"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn valid_request_json(credential_types: &str) -> String {
        format!(
            r#"{{
                "version": 0,
                "hpke": [{{
                    "mode": "base",
                    "kem": 32,
                    "kdf": 1,
                    "aead": 1,
                    "key": {{
                        "kty": "OKP",
                        "crv": "X25519",
                        "x": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
                    }}
                }}],
                "archive": ["future-archive", "deflate"],
                "mode": "indirect",
                "importer": "importer.example",
                {credential_types}
                "knownExtensions": []
            }}"#
        )
    }

    fn unique_temp_path() -> PathBuf {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        std::env::temp_dir().join(format!(
            "keycord-passkey-request-{}-{}.json",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn parses_indirect_passkey_request() {
        let input = valid_request_json(r#""credentialTypes": ["passkey"],"#);
        let request = parse_passkey_export_request(input.as_bytes()).unwrap();

        assert_eq!(request.version, CXP_EXPORT_REQUEST_VERSION);
        assert_eq!(request.mode, "indirect");
        assert_eq!(request.importer, "importer.example");
        assert_eq!(request.archive, ["future-archive", "deflate"]);
        assert_eq!(request.credential_types, Some(vec!["passkey".to_string()]));
        assert_eq!(request.known_extensions, Some(Vec::new()));
        assert_eq!(request.hpke.len(), 1);
        assert_eq!(request.hpke[0].mode, "base");
    }

    #[test]
    fn missing_credential_types_requests_passkeys() {
        let input = valid_request_json("");
        let request = parse_passkey_export_request(input.as_bytes()).unwrap();

        assert_eq!(request.credential_types, None);
    }

    #[test]
    fn non_passkey_cxp_request_is_classified_separately() {
        let input = valid_request_json(r#""credentialTypes": ["basic-auth"],"#);
        let err = parse_passkey_export_request(input.as_bytes()).unwrap_err();

        assert!(matches!(err, PasskeyRequestError::NotPasskeyRequest));
        assert!(err.is_not_passkey_request());
    }

    #[test]
    fn unrelated_json_and_text_are_not_passkey_requests() {
        for input in [br#"{"hello":"world"}"#.as_slice(), b"not json"] {
            assert!(matches!(
                parse_passkey_export_request(input),
                Err(PasskeyRequestError::NotPasskeyRequest)
            ));
        }
    }

    #[test]
    fn malformed_recognizable_request_is_reported() {
        assert!(matches!(
            parse_passkey_export_request(br#"{"version":0,"hpke":["#),
            Err(PasskeyRequestError::MalformedJson(_))
        ));
        assert!(matches!(
            parse_passkey_export_request(
                br#"{"version":0,"archive":["deflate"],"mode":"indirect","importer":"example"}"#
            ),
            Err(PasskeyRequestError::Malformed(_))
        ));
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let mut value: Value = serde_json::from_str(&valid_request_json("")).unwrap();
        value["surprise"] = Value::Bool(true);

        assert!(matches!(
            parse_passkey_export_request(&serde_json::to_vec(&value).unwrap()),
            Err(PasskeyRequestError::Malformed(_))
        ));
    }

    #[test]
    fn unsupported_version_mode_archive_and_hpke_are_distinct() {
        let mut value: Value = serde_json::from_str(&valid_request_json("")).unwrap();
        value["version"] = Value::from(1);
        assert!(matches!(
            parse_passkey_export_request(&serde_json::to_vec(&value).unwrap()),
            Err(PasskeyRequestError::UnsupportedVersion(1))
        ));

        value["version"] = Value::from(0);
        value["mode"] = Value::from("direct");
        assert!(matches!(
            parse_passkey_export_request(&serde_json::to_vec(&value).unwrap()),
            Err(PasskeyRequestError::UnsupportedMode(mode)) if mode == "direct"
        ));

        value["mode"] = Value::from("indirect");
        value["archive"] = serde_json::json!(["future-archive"]);
        assert!(matches!(
            parse_passkey_export_request(&serde_json::to_vec(&value).unwrap()),
            Err(PasskeyRequestError::UnsupportedArchive)
        ));

        value["archive"] = serde_json::json!(["deflate"]);
        value["hpke"][0]["mode"] = Value::from("future-hpke-mode");
        assert!(matches!(
            parse_passkey_export_request(&serde_json::to_vec(&value).unwrap()),
            Err(PasskeyRequestError::UnsupportedHpke)
        ));
    }

    #[test]
    fn required_lists_and_jwk_are_validated() {
        let mut value: Value = serde_json::from_str(&valid_request_json("")).unwrap();
        value["archive"] = serde_json::json!([]);
        assert!(matches!(
            parse_passkey_export_request(&serde_json::to_vec(&value).unwrap()),
            Err(PasskeyRequestError::Malformed(_))
        ));

        value["archive"] = serde_json::json!(["deflate"]);
        value["hpke"] = serde_json::json!([]);
        assert!(matches!(
            parse_passkey_export_request(&serde_json::to_vec(&value).unwrap()),
            Err(PasskeyRequestError::Malformed(_))
        ));

        value["hpke"] = serde_json::json!([{
            "mode": "base",
            "kem": 32,
            "kdf": 1,
            "aead": 1,
            "key": {"crv": "X25519", "x": "AA"}
        }]);
        assert!(matches!(
            parse_passkey_export_request(&serde_json::to_vec(&value).unwrap()),
            Err(PasskeyRequestError::Malformed(_))
        ));
    }

    #[test]
    fn importer_rp_id_and_hpke_public_key_are_structurally_checked() {
        let mut value: Value = serde_json::from_str(&valid_request_json("")).unwrap();
        value["importer"] = Value::from("https://importer.example");
        assert!(matches!(
            parse_passkey_export_request(&serde_json::to_vec(&value).unwrap()),
            Err(PasskeyRequestError::Malformed(_))
        ));

        value["importer"] = Value::from("importer.example");
        value["hpke"][0]["key"]["crv"] = Value::from("P-256");
        assert!(matches!(
            parse_passkey_export_request(&serde_json::to_vec(&value).unwrap()),
            Err(PasskeyRequestError::Malformed(_))
        ));

        value["hpke"][0]["key"]["crv"] = Value::from("X25519");
        value["hpke"][0]["key"]["d"] = Value::from("private-material");
        assert!(matches!(
            parse_passkey_export_request(&serde_json::to_vec(&value).unwrap()),
            Err(PasskeyRequestError::Malformed(_))
        ));
    }

    #[test]
    fn unknown_hpke_candidates_are_ignored_when_a_known_candidate_exists() {
        let mut value: Value = serde_json::from_str(&valid_request_json("")).unwrap();
        value["hpke"].as_array_mut().unwrap().insert(
            0,
            serde_json::json!({
                "mode": "future-hpke-mode",
                "kem": 65_535,
                "kdf": 65_535,
                "aead": 65_535
            }),
        );

        let request = parse_passkey_export_request(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(request.hpke.len(), 2);
    }

    #[test]
    fn future_hpke_suite_is_ignored_when_a_known_candidate_exists() {
        let mut value: Value = serde_json::from_str(&valid_request_json("")).unwrap();
        value["hpke"].as_array_mut().unwrap().insert(
            0,
            serde_json::json!({
                "mode": "base",
                "kem": 65_535,
                "kdf": 65_535,
                "aead": 65_535,
                "key": {"kty": "future-key-type"}
            }),
        );

        let request = parse_passkey_export_request(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(request.hpke.len(), 2);
    }

    #[test]
    fn reads_bounded_regular_local_file() {
        let path = unique_temp_path();
        fs::write(&path, valid_request_json("")).unwrap();

        let opened = read_passkey_export_request(&path).unwrap();
        assert_eq!(opened.source_path, path);
        assert_eq!(
            opened.suggested_response_path,
            path.with_file_name(format!(
                "{}.response.json",
                path.file_stem().unwrap().to_string_lossy()
            ))
        );

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn reads_standard_cxf_passkey_files_for_import() {
        use crate::password::passkey::{export_cxf_passkey_json, generate_passkey_credential};

        let credential =
            generate_passkey_credential("example.com", "alice", "Alice").expect("generate passkey");
        let path = unique_temp_path();
        fs::write(
            &path,
            export_cxf_passkey_json(&credential).expect("export CXF passkey"),
        )
        .unwrap();

        let opened = read_opened_passkey_file(&path).expect("open CXF passkey");
        let OpenedPasskeyFile::Credential(imported) = opened else {
            panic!("expected a CXF credential");
        };
        assert_eq!(imported.credential_id, credential.credential_id);
        assert_eq!(imported.rp_id, "example.com");

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn oversized_file_is_rejected_before_json_parsing() {
        let path = unique_temp_path();
        fs::write(&path, vec![b' '; MAX_PASSKEY_REQUEST_BYTES + 1]).unwrap();

        assert!(matches!(
            read_passkey_export_request(&path),
            Err(PasskeyRequestError::TooLarge {
                limit: MAX_PASSKEY_REQUEST_BYTES
            })
        ));

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn sibling_response_path_never_uses_importer_input() {
        let path = Path::new("/tmp/request.cxp.json");
        assert_eq!(
            sibling_response_path(path).unwrap(),
            Path::new("/tmp/request.cxp.response.json")
        );
        assert!(matches!(
            sibling_response_path(Path::new("/")),
            Err(PasskeyRequestError::InvalidSource { .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_link_source_is_rejected() {
        use std::os::unix::fs::symlink;

        let target = unique_temp_path();
        let link = target.with_extension("link.json");
        fs::write(&target, valid_request_json("")).unwrap();
        symlink(&target, &link).unwrap();

        assert!(matches!(
            read_passkey_export_request(&link),
            Err(PasskeyRequestError::InvalidSource { .. })
        ));

        fs::remove_file(link).unwrap();
        fs::remove_file(target).unwrap();
    }
}
