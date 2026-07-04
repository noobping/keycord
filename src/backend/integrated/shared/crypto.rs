use super::keys::{
    borrow_unlocked_hardware_private_key, build_ripasso_crypto_from_key_ring,
    decrypt_with_hardware_session, ensure_ripasso_private_key_is_ready, fingerprint_from_string,
    load_available_standard_key_ring, load_ripasso_key_ring,
};
use super::paths::recipients_file_for_label;
use super::recipients::{
    effective_private_key_requirement, encryption_context_fingerprint_from_contents,
    private_key_requirement_from_contents, read_store_recipient_file_contents,
    resolved_recipients_from_contents, ResolvedRecipient,
};
use crate::backend::{PasswordEntryError, StoreRecipientsPrivateKeyRequirement};
use ripasso::crypto::{Crypto, Sequoia};
use ripasso::pass::{Comment, KeyRingStatus, OwnerTrustLevel, Recipient};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

const REQUIRE_ALL_PRIVATE_KEYS_LAYER_HEADER: &str = "keycord-require-all-private-keys-v1";

#[derive(Clone, Debug, PartialEq, Eq)]
enum RequiredPrivateKeyRecipient {
    Standard { fingerprint: String },
}

pub(super) struct IntegratedCryptoContext {
    crypto: Sequoia,
    recipients: Vec<Recipient>,
    fingerprint: String,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
    required_private_key_recipients: Vec<RequiredPrivateKeyRecipient>,
}

impl IntegratedCryptoContext {
    pub(super) fn load_for_fingerprint(fingerprint: &str) -> Result<Self, String> {
        let key_ring = load_ripasso_key_ring(fingerprint)?;
        let crypto = build_ripasso_crypto_from_key_ring(fingerprint, key_ring)?;
        Ok(Self {
            crypto,
            recipients: Vec::new(),
            fingerprint: fingerprint.to_string(),
            private_key_requirement: StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
            required_private_key_recipients: vec![RequiredPrivateKeyRecipient::Standard {
                fingerprint: fingerprint.to_string(),
            }],
        })
    }

    pub(super) fn load_for_label(store_root: &str, label: &str) -> Result<Self, String> {
        let recipients_file = recipients_file_for_label(store_root, label)?;
        Self::load_for_recipients_file(&recipients_file)
    }

    pub(super) fn fingerprint_for_label(store_root: &str, label: &str) -> Result<String, String> {
        let recipients_file = recipients_file_for_label(store_root, label)?;
        Self::fingerprint_for_recipients_file(&recipients_file)
    }

    fn load_for_recipients_file(recipients_file: &Path) -> Result<Self, String> {
        let contents = read_store_recipient_file_contents(recipients_file)?;
        Self::load_for_recipient_contents(&contents)
    }

    fn fingerprint_for_recipients_file(recipients_file: &Path) -> Result<String, String> {
        let contents = read_store_recipient_file_contents(recipients_file)?;
        Self::fingerprint_for_recipient_contents(&contents)
    }

    pub(super) fn load_for_recipient_contents(contents: &str) -> Result<Self, String> {
        let key_ring = load_available_standard_key_ring()?;
        let resolved = resolved_recipients_from_contents(contents, &key_ring)?;
        let recipients = standard_recipients_from_resolved(&resolved);
        let fingerprint = encryption_context_fingerprint_from_contents(contents, &key_ring)?;
        let private_key_requirement = effective_private_key_requirement(
            private_key_requirement_from_contents(contents),
            recipients.len(),
        );
        let required_private_key_recipients = required_recipients_from_resolved(&resolved);
        let crypto = build_ripasso_crypto_from_key_ring(&fingerprint, key_ring)?;
        Ok(Self {
            crypto,
            recipients,
            fingerprint,
            private_key_requirement,
            required_private_key_recipients,
        })
    }

    pub(super) fn fingerprint_for_recipient_contents(contents: &str) -> Result<String, String> {
        let key_ring = load_available_standard_key_ring()?;
        encryption_context_fingerprint_from_contents(contents, &key_ring)
    }

    pub(super) fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    pub(super) fn decrypt_entry(&self, entry_path: &Path) -> Result<String, String> {
        let ciphertext = read_entry_ciphertext(entry_path)?;
        match self.private_key_requirement {
            StoreRecipientsPrivateKeyRequirement::AnyManagedKey => {
                decrypt_ciphertext_for_fingerprint(&self.fingerprint, &self.crypto, &ciphertext)
            }
            StoreRecipientsPrivateKeyRequirement::AllManagedKeys => {
                decrypt_password_entry_requiring_all_private_keys(
                    &ciphertext,
                    &self.required_private_key_recipients,
                )
            }
        }
    }

    pub(super) fn encrypt_contents_with_existing(
        &self,
        contents: &str,
        _existing_ciphertext: Option<&[u8]>,
    ) -> Result<Vec<u8>, String> {
        match self.private_key_requirement {
            StoreRecipientsPrivateKeyRequirement::AnyManagedKey => {
                encrypt_password_entry_with_crypto(&self.crypto, &self.recipients, contents)
            }
            StoreRecipientsPrivateKeyRequirement::AllManagedKeys => {
                encrypt_password_entry_requiring_all_private_keys(
                    contents,
                    &self.required_private_key_recipients,
                )
            }
        }
    }
}

pub(super) fn decrypt_entry_for_fingerprint(
    fingerprint: &str,
    entry_path: &Path,
) -> Result<String, String> {
    let ciphertext = read_entry_ciphertext(entry_path)?;
    ensure_ripasso_private_key_is_ready(fingerprint).map_err(password_entry_error_to_string)?;
    let context = IntegratedCryptoContext::load_for_fingerprint(fingerprint)?;
    decrypt_ciphertext_for_fingerprint(fingerprint, &context.crypto, &ciphertext)
}

fn standard_recipients_from_resolved(resolved: &[ResolvedRecipient<'_>]) -> Vec<Recipient> {
    let mut recipients = Vec::new();

    for recipient in resolved {
        let ResolvedRecipient::Standard {
            fingerprint,
            cert,
            requested_id,
        } = recipient;

        let name = cert
            .userids()
            .map(|user_id| user_id.userid().to_string())
            .find(|value| !value.trim().is_empty())
            .unwrap_or_else(|| requested_id.clone());

        recipients.push(Recipient {
            name,
            comment: Comment {
                pre_comment: None,
                post_comment: None,
            },
            key_id: cert.fingerprint().to_hex(),
            fingerprint: Some(*fingerprint),
            key_ring_status: KeyRingStatus::InKeyRing,
            trust_level: OwnerTrustLevel::Ultimate,
            not_usable: false,
        });
    }

    recipients
}

fn required_recipients_from_resolved(
    resolved: &[ResolvedRecipient<'_>],
) -> Vec<RequiredPrivateKeyRecipient> {
    resolved
        .iter()
        .map(|recipient| match recipient {
            ResolvedRecipient::Standard { cert, .. } => RequiredPrivateKeyRecipient::Standard {
                fingerprint: cert.fingerprint().to_hex(),
            },
        })
        .collect()
}

fn read_entry_ciphertext(entry_path: &Path) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(entry_path).map_err(|err| err.to_string())?;
    if metadata.len() == 0 {
        return Err("empty password file".to_string());
    }
    fs::read(entry_path).map_err(|err| err.to_string())
}

fn decrypt_ciphertext_with_crypto(crypto: &Sequoia, ciphertext: &[u8]) -> Result<String, String> {
    crypto
        .decrypt_string(ciphertext)
        .map_err(|err| err.to_string())
}

fn decrypt_ciphertext_for_fingerprint(
    fingerprint: &str,
    crypto: &Sequoia,
    ciphertext: &[u8],
) -> Result<String, String> {
    if let Some(session) = borrow_unlocked_hardware_private_key(fingerprint)? {
        return decrypt_with_hardware_session(&session, ciphertext).map_err(|err| err.to_string());
    }

    decrypt_ciphertext_with_crypto(crypto, ciphertext)
}

fn decrypt_password_entry_requiring_all_private_keys(
    ciphertext: &[u8],
    required_private_key_recipients: &[RequiredPrivateKeyRecipient],
) -> Result<String, String> {
    let mut current = ciphertext.to_vec();

    for (index, recipient) in required_private_key_recipients.iter().enumerate() {
        let decrypted = decrypt_required_private_key_layer(recipient, &current)?;
        let is_final_layer = index + 1 == required_private_key_recipients.len();
        if is_final_layer {
            return String::from_utf8(decrypted).map_err(|err| err.to_string());
        }

        current = unwrap_required_private_key_layer(&decrypted)?;
    }

    Err("No recipients were found for this password entry.".to_string())
}

fn decrypt_required_private_key_layer(
    recipient: &RequiredPrivateKeyRecipient,
    ciphertext: &[u8],
) -> Result<Vec<u8>, String> {
    match recipient {
        RequiredPrivateKeyRecipient::Standard { fingerprint } => {
            let context = IntegratedCryptoContext::load_for_fingerprint(fingerprint)?;
            let decrypted =
                decrypt_ciphertext_for_fingerprint(fingerprint, &context.crypto, ciphertext)?;
            Ok(decrypted.into_bytes())
        }
    }
}

fn encrypt_password_entry_requiring_all_private_keys(
    contents: &str,
    required_private_key_recipients: &[RequiredPrivateKeyRecipient],
) -> Result<Vec<u8>, String> {
    let Some((last_recipient, outer_recipients)) = required_private_key_recipients.split_last()
    else {
        return Err("No recipients were found for this password entry.".to_string());
    };

    let mut current =
        encrypt_for_required_private_key_recipient(last_recipient, contents.as_bytes())?;

    for recipient in outer_recipients.iter().rev() {
        let wrapped = wrap_required_private_key_layer(&current);
        current = encrypt_for_required_private_key_recipient(recipient, wrapped.as_bytes())?;
    }

    Ok(current)
}

fn encrypt_for_required_private_key_recipient(
    recipient: &RequiredPrivateKeyRecipient,
    payload: &[u8],
) -> Result<Vec<u8>, String> {
    match recipient {
        RequiredPrivateKeyRecipient::Standard { fingerprint } => {
            let context = IntegratedCryptoContext::load_for_fingerprint(fingerprint)?;
            let text = String::from_utf8(payload.to_vec()).map_err(|err| err.to_string())?;
            let recipient = Recipient {
                name: fingerprint.clone(),
                comment: Comment {
                    pre_comment: None,
                    post_comment: None,
                },
                key_id: fingerprint.clone(),
                fingerprint: Some(fingerprint_from_string(fingerprint)?),
                key_ring_status: KeyRingStatus::InKeyRing,
                trust_level: OwnerTrustLevel::Ultimate,
                not_usable: false,
            };
            encrypt_password_entry_with_crypto(&context.crypto, &[recipient], &text)
        }
    }
}

fn wrap_required_private_key_layer(ciphertext: &[u8]) -> String {
    format!(
        "{REQUIRE_ALL_PRIVATE_KEYS_LAYER_HEADER}\n{}",
        encode_hex(ciphertext)
    )
}

fn unwrap_required_private_key_layer(payload: &[u8]) -> Result<Vec<u8>, String> {
    let payload = std::str::from_utf8(payload).map_err(|err| err.to_string())?;
    let (header, body) = payload
        .split_once('\n')
        .ok_or_else(|| "Invalid all-keys encrypted password entry.".to_string())?;
    if header.trim() != REQUIRE_ALL_PRIVATE_KEYS_LAYER_HEADER {
        return Err("Invalid all-keys encrypted password entry.".to_string());
    }

    decode_hex(body.trim())
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(encoded, "{byte:02x}").expect("writing hex into a string should not fail");
    }
    encoded
}

fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
    if !value.len().is_multiple_of(2) {
        return Err("Invalid all-keys encrypted password entry.".to_string());
    }

    let mut decoded = Vec::with_capacity(value.len() / 2);
    let mut index = 0;
    while index < value.len() {
        let byte = u8::from_str_radix(&value[index..index + 2], 16)
            .map_err(|_| "Invalid all-keys encrypted password entry.".to_string())?;
        decoded.push(byte);
        index += 2;
    }
    Ok(decoded)
}

fn password_entry_error_to_string(err: PasswordEntryError) -> String {
    match err {
        PasswordEntryError::EntryNotFound(message)
        | PasswordEntryError::LockedPrivateKey(message)
        | PasswordEntryError::MissingPrivateKey(message)
        | PasswordEntryError::IncompatiblePrivateKey(message)
        | PasswordEntryError::Other(message) => message,
    }
}

fn encrypt_password_entry_with_crypto(
    crypto: &Sequoia,
    recipients: &[Recipient],
    contents: &str,
) -> Result<Vec<u8>, String> {
    if recipients.is_empty() {
        return Err("No recipients were found for this password entry.".to_string());
    }
    crypto
        .encrypt_string(contents, recipients)
        .map_err(|err| err.to_string())
}
