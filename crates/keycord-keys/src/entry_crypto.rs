//! Binary-safe OpenPGP entry encryption using the existing managed certificates.
use pgp::{
    parse::{
        stream::{DecryptionHelper, DecryptorBuilder, MessageStructure, VerificationHelper},
        Parse,
    },
    policy::StandardPolicy,
    serialize::stream::{Encryptor, LiteralWriter, Message},
    Cert, KeyHandle,
};
use ripasso::pass::Recipient;
use sequoia_openpgp as pgp;
use std::{collections::HashMap, io::Write, sync::Arc};
use zeroize::Zeroizing;

pub struct RipassoCrypto {
    fingerprint: [u8; 20],
    keys: HashMap<[u8; 20], Arc<Cert>>,
}
impl RipassoCrypto {
    pub(crate) fn new(fingerprint: [u8; 20], keys: HashMap<[u8; 20], Arc<Cert>>) -> Self {
        Self { fingerprint, keys }
    }
    pub fn decrypt_bytes(&self, ciphertext: &[u8]) -> Result<Vec<u8>, String> {
        let cert = self
            .keys
            .get(&self.fingerprint)
            .ok_or("no key for user found")?;
        let policy = StandardPolicy::new();
        let mut reader = DecryptorBuilder::from_bytes(ciphertext)
            .map_err(|e| e.to_string())?
            .with_policy(
                &policy,
                None,
                EntryDecryptor {
                    cert,
                    policy: &policy,
                },
            )
            .map_err(|e| e.to_string())?;
        let mut plaintext = Zeroizing::new(Vec::new());
        std::io::copy(&mut reader, &mut *plaintext).map_err(|e| e.to_string())?;
        Ok(std::mem::take(&mut *plaintext))
    }
    pub fn encrypt_bytes(
        &self,
        plaintext: &[u8],
        recipients: &[Recipient],
    ) -> Result<Vec<u8>, String> {
        let policy = StandardPolicy::new();
        let mut certs = Vec::new();
        for recipient in recipients {
            if let Some(fp) = recipient.fingerprint {
                certs.push(self.keys.get(&fp).ok_or_else(|| {
                    format!("Recipient with key id {} not found", recipient.key_id)
                })?);
            } else {
                let handle: KeyHandle = recipient
                    .key_id
                    .parse()
                    .map_err(|e: anyhow::Error| e.to_string())?;
                certs.extend(
                    self.keys
                        .values()
                        .filter(|c| c.key_handle().aliases(&handle)),
                );
            }
        }
        let keys: Vec<_> = certs
            .iter()
            .flat_map(|cert| {
                cert.keys()
                    .with_policy(&policy, None)
                    .supported()
                    .alive()
                    .revoked(false)
                    .for_transport_encryption()
            })
            .collect();
        let mut ciphertext = Vec::new();
        let message = Encryptor::for_recipients(Message::new(&mut ciphertext), keys)
            .build()
            .map_err(|e| e.to_string())?;
        let mut writer = LiteralWriter::new(message)
            .build()
            .map_err(|e| e.to_string())?;
        writer.write_all(plaintext).map_err(|e| e.to_string())?;
        writer.finalize().map_err(|e| e.to_string())?;
        Ok(ciphertext)
    }
}
struct EntryDecryptor<'a> {
    cert: &'a Cert,
    policy: &'a StandardPolicy<'a>,
}
impl VerificationHelper for EntryDecryptor<'_> {
    fn get_certs(&mut self, _: &[KeyHandle]) -> pgp::Result<Vec<Cert>> {
        Ok(Vec::new())
    }
    fn check(&mut self, _: MessageStructure) -> pgp::Result<()> {
        Ok(())
    }
}
impl DecryptionHelper for EntryDecryptor<'_> {
    fn decrypt(
        &mut self,
        pkesks: &[pgp::packet::PKESK],
        _: &[pgp::packet::SKESK],
        algorithm: Option<pgp::types::SymmetricAlgorithm>,
        decrypt: &mut dyn FnMut(
            Option<pgp::types::SymmetricAlgorithm>,
            &pgp::crypto::SessionKey,
        ) -> bool,
    ) -> pgp::Result<Option<Cert>> {
        for key in self
            .cert
            .keys()
            .unencrypted_secret()
            .with_policy(self.policy, None)
            .for_transport_encryption()
        {
            let mut pair = key.key().clone().into_keypair()?;
            for pkesk in pkesks {
                if pkesk
                    .decrypt(&mut pair, algorithm)
                    .is_some_and(|(alg, session)| decrypt(alg, &session))
                {
                    return Ok(Some(self.cert.clone()));
                }
            }
        }
        Err(anyhow::anyhow!(
            "no pkesks managed to decrypt the ciphertext"
        ))
    }
}
