use crate::FidoError;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use hmac::{digest::KeyInit, Hmac, Mac};
use openssl::symm::{Cipher, Crypter, Mode};
use rand::random;
use sha2::Sha256;

const AES_GCM_TAG_LEN: usize = 16;
const FIDO_KEK_INFO: &[u8] = b"keycord/fido2-hmac-secret/kek/v1";
const FIDO_DEK_LEN: usize = 32;

pub(crate) fn derive_kek(
    hmac_secret: &[u8],
    fingerprint: &str,
    hmac_salt: &[u8],
) -> Result<Vec<u8>, FidoError> {
    hkdf_sha256(
        hmac_secret,
        fingerprint.as_bytes(),
        hmac_salt,
        FIDO_KEK_INFO,
        FIDO_DEK_LEN,
    )
}

fn hkdf_sha256(
    ikm: &[u8],
    salt: &[u8],
    hmac_salt: &[u8],
    info: &[u8],
    len: usize,
) -> Result<Vec<u8>, FidoError> {
    type HmacSha256 = Hmac<Sha256>;

    let mut extract =
        HmacSha256::new_from_slice(salt).map_err(|error| FidoError::crypto(error.to_string()))?;
    extract.update(ikm);
    extract.update(hmac_salt);
    let prk = extract.finalize().into_bytes();

    let mut output = Vec::with_capacity(len);
    let mut previous = Vec::new();
    let mut counter: u8 = 1;
    while output.len() < len {
        let mut expand = HmacSha256::new_from_slice(&prk)
            .map_err(|error| FidoError::crypto(error.to_string()))?;
        if !previous.is_empty() {
            expand.update(&previous);
        }
        expand.update(info);
        expand.update(&[counter]);
        previous = expand.finalize().into_bytes().to_vec();
        output.extend_from_slice(&previous);
        counter = counter
            .checked_add(1)
            .ok_or_else(|| FidoError::crypto("Failed to derive enough HKDF output."))?;
    }
    output.truncate(len);
    Ok(output)
}

pub(crate) fn encrypt_aes_256_gcm(
    key: &[u8],
    nonce: &[u8],
    aad: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, FidoError> {
    let cipher = Cipher::aes_256_gcm();
    let mut crypter = Crypter::new(cipher, Mode::Encrypt, key, Some(nonce))
        .map_err(|error| FidoError::crypto(error.to_string()))?;
    crypter.pad(false);
    crypter
        .aad_update(aad)
        .map_err(|error| FidoError::crypto(error.to_string()))?;
    let mut ciphertext = vec![0u8; plaintext.len() + cipher.block_size()];
    let mut count = crypter
        .update(plaintext, &mut ciphertext)
        .map_err(|error| FidoError::crypto(error.to_string()))?;
    count += crypter
        .finalize(&mut ciphertext[count..])
        .map_err(|error| FidoError::crypto(error.to_string()))?;
    ciphertext.truncate(count);

    let mut tag = [0u8; AES_GCM_TAG_LEN];
    crypter
        .get_tag(&mut tag)
        .map_err(|error| FidoError::crypto(error.to_string()))?;
    ciphertext.extend_from_slice(&tag);
    Ok(ciphertext)
}

pub(crate) fn decrypt_aes_256_gcm(
    key: &[u8],
    nonce: &[u8],
    aad: &[u8],
    ciphertext_and_tag: &[u8],
) -> Result<Vec<u8>, FidoError> {
    if ciphertext_and_tag.len() < AES_GCM_TAG_LEN {
        return Err(FidoError::invalid("Invalid FIDO2 encrypted data."));
    }
    let (ciphertext, tag) = ciphertext_and_tag.split_at(ciphertext_and_tag.len() - AES_GCM_TAG_LEN);
    let cipher = Cipher::aes_256_gcm();
    let mut crypter = Crypter::new(cipher, Mode::Decrypt, key, Some(nonce))
        .map_err(|error| FidoError::crypto(error.to_string()))?;
    crypter.pad(false);
    crypter
        .aad_update(aad)
        .map_err(|error| FidoError::crypto(error.to_string()))?;
    crypter
        .set_tag(tag)
        .map_err(|error| FidoError::crypto(error.to_string()))?;
    let mut plaintext = vec![0u8; ciphertext.len() + cipher.block_size()];
    let mut count = crypter
        .update(ciphertext, &mut plaintext)
        .map_err(|_| FidoError::crypto("Couldn't decrypt the FIDO2-encrypted data."))?;
    count += crypter
        .finalize(&mut plaintext[count..])
        .map_err(|_| FidoError::crypto("Couldn't decrypt the FIDO2-encrypted data."))?;
    plaintext.truncate(count);
    Ok(plaintext)
}

pub(crate) fn encode_base64(bytes: &[u8]) -> String {
    BASE64.encode(bytes)
}

pub(crate) fn decode_base64(value: &str) -> Result<Vec<u8>, FidoError> {
    BASE64
        .decode(value)
        .map_err(|error| FidoError::invalid(error.to_string()))
}

pub(crate) fn random_bytes<const N: usize>() -> [u8; N] {
    random::<[u8; N]>()
}

#[cfg(test)]
mod tests {
    use super::{decrypt_aes_256_gcm, derive_kek, encode_base64, encrypt_aes_256_gcm};

    #[test]
    fn key_derivation_is_stable_and_matches_the_existing_format() {
        let derived = derive_kek(
            b"existing-hmac-secret",
            "0123456789ABCDEF0123456789ABCDEF01234567",
            b"existing-hmac-salt",
        )
        .unwrap();
        assert_eq!(derived.len(), 32);
        assert_eq!(
            derived,
            [
                97, 94, 125, 159, 61, 2, 131, 174, 125, 176, 209, 177, 151, 163, 225, 162, 190,
                192, 71, 15, 171, 206, 98, 21, 78, 138, 245, 44, 180, 243, 14, 218,
            ]
        );
    }

    #[test]
    fn aes_gcm_round_trips_with_the_existing_tag_layout() {
        let key = [7u8; 32];
        let nonce = [9u8; 12];
        let encrypted = encrypt_aes_256_gcm(&key, &nonce, b"aad", b"private key").unwrap();
        assert_eq!(
            encode_base64(&encrypted),
            "V/ft4t+EpEHLB7IDXs8xKGrMWoak/Du4Ddpy"
        );
        assert_eq!(
            decrypt_aes_256_gcm(&key, &nonce, b"aad", &encrypted).unwrap(),
            b"private key"
        );
        assert_eq!(encrypted.len(), b"private key".len() + 16);
    }
}
