use crate::FidoError;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

pub(crate) const DEFAULT_SECRET_CACHE_IDLE_TIMEOUT: Duration = Duration::from_secs(15 * 60);

#[derive(Clone)]
struct CacheEntry<T> {
    value: T,
    last_secret_use: Instant,
}

impl<T> CacheEntry<T> {
    fn new(value: T) -> Self {
        Self {
            value,
            last_secret_use: Instant::now(),
        }
    }
}

struct SecretCache<T> {
    entries: RwLock<HashMap<String, CacheEntry<T>>>,
    idle_timeout: Duration,
}

impl<T> SecretCache<T> {
    fn new(idle_timeout: Duration) -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            idle_timeout,
        }
    }

    fn with_write<R>(
        &self,
        f: impl FnOnce(&mut HashMap<String, CacheEntry<T>>, Instant) -> R,
    ) -> R {
        let mut entries = self
            .entries
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let now = Instant::now();
        entries.retain(|_, entry| {
            now.saturating_duration_since(entry.last_secret_use) < self.idle_timeout
        });
        f(&mut entries, now)
    }

    fn insert(&self, fingerprint: String, value: T) {
        self.with_write(|entries, _| {
            entries.insert(fingerprint, CacheEntry::new(value));
        });
    }

    fn remove(&self, fingerprint: &str) {
        self.with_write(|entries, _| {
            entries.remove(fingerprint);
        });
    }

    fn clear(&self) {
        self.with_write(|entries, _| entries.clear());
    }
}

impl<T: Clone> SecretCache<T> {
    fn borrow(&self, fingerprint: &str) -> Option<T> {
        self.with_write(|entries, now| {
            let entry = entries.get_mut(fingerprint)?;
            entry.last_secret_use = now;
            Some(entry.value.clone())
        })
    }

    #[cfg(test)]
    fn peek(&self, fingerprint: &str) -> Option<T> {
        self.with_write(|entries, _| entries.get(fingerprint).map(|entry| entry.value.clone()))
    }
}

pub(crate) type CachedPin = Arc<Zeroizing<Vec<u8>>>;

#[derive(Debug)]
pub(crate) struct PendingEnrollment {
    credential_id: Vec<u8>,
    hmac_salt: Zeroizing<Vec<u8>>,
    hmac_secret: Zeroizing<Vec<u8>>,
}

impl PendingEnrollment {
    fn new(credential_id: &[u8], hmac_salt: &[u8], hmac_secret: &[u8]) -> Self {
        Self {
            credential_id: credential_id.to_vec(),
            hmac_salt: Zeroizing::new(hmac_salt.to_vec()),
            hmac_secret: Zeroizing::new(hmac_secret.to_vec()),
        }
    }

    pub(crate) fn matches_credential_id(&self, credential_id: &[u8]) -> bool {
        self.credential_id == credential_id
    }

    pub(crate) fn hmac_salt(&self) -> &[u8] {
        self.hmac_salt.as_slice()
    }

    pub(crate) fn hmac_secret(&self) -> &[u8] {
        self.hmac_secret.as_slice()
    }
}

impl Clone for PendingEnrollment {
    fn clone(&self) -> Self {
        Self::new(&self.credential_id, self.hmac_salt(), self.hmac_secret())
    }
}

pub(crate) struct FidoCaches {
    pins: SecretCache<CachedPin>,
    enrollments: SecretCache<PendingEnrollment>,
}

impl FidoCaches {
    pub(crate) fn new(idle_timeout: Duration) -> Self {
        Self {
            pins: SecretCache::new(idle_timeout),
            enrollments: SecretCache::new(idle_timeout),
        }
    }

    pub(crate) fn borrow_pin(&self, fingerprint: &str) -> Result<Option<CachedPin>, FidoError> {
        Ok(self.pins.borrow(&normalize_fingerprint(fingerprint)?))
    }

    pub(crate) fn cache_pin(&self, fingerprint: &str, pin: &[u8]) -> Result<(), FidoError> {
        self.pins.insert(
            normalize_fingerprint(fingerprint)?,
            Arc::new(Zeroizing::new(pin.to_vec())),
        );
        Ok(())
    }

    pub(crate) fn borrow_enrollment(
        &self,
        fingerprint: &str,
    ) -> Result<Option<PendingEnrollment>, FidoError> {
        Ok(self
            .enrollments
            .borrow(&normalize_fingerprint(fingerprint)?))
    }

    pub(crate) fn cache_enrollment(
        &self,
        fingerprint: &str,
        credential_id: &[u8],
        hmac_salt: &[u8],
        hmac_secret: &[u8],
    ) -> Result<(), FidoError> {
        self.enrollments.insert(
            normalize_fingerprint(fingerprint)?,
            PendingEnrollment::new(credential_id, hmac_salt, hmac_secret),
        );
        Ok(())
    }

    pub(crate) fn remove(&self, fingerprint: &str) -> Result<(), FidoError> {
        let fingerprint = normalize_fingerprint(fingerprint)?;
        self.pins.remove(&fingerprint);
        self.enrollments.remove(&fingerprint);
        Ok(())
    }

    pub(crate) fn clear(&self) {
        self.pins.clear();
        self.enrollments.clear();
    }
}

fn normalize_fingerprint(value: &str) -> Result<String, FidoError> {
    let normalized: String = value
        .chars()
        .filter(|character| !character.is_ascii_whitespace() && *character != ':')
        .collect();
    if !matches!(normalized.len(), 40 | 64)
        || !normalized.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(FidoError::invalid(format!(
            "Invalid FIDO2 binding fingerprint '{value}'."
        )));
    }
    Ok(normalized.to_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::FidoCaches;
    use std::time::Duration;

    const FINGERPRINT: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn caches_normalize_fingerprints_and_clear_secrets_together() {
        let caches = FidoCaches::new(Duration::from_secs(60));
        caches.cache_pin(FINGERPRINT, b"1234").unwrap();
        caches
            .cache_enrollment(FINGERPRINT, b"credential", b"salt", b"secret")
            .unwrap();

        assert_eq!(
            caches
                .pins
                .peek(&FINGERPRINT.to_ascii_uppercase())
                .unwrap()
                .as_slice(),
            b"1234"
        );
        assert!(caches
            .enrollments
            .peek(&FINGERPRINT.to_ascii_uppercase())
            .is_some());

        caches.clear();
        assert!(caches.pins.peek(FINGERPRINT).is_none());
        assert!(caches.enrollments.peek(FINGERPRINT).is_none());
    }

    #[test]
    fn expired_secret_entries_are_pruned() {
        let caches = FidoCaches::new(Duration::ZERO);
        caches.cache_pin(FINGERPRINT, b"1234").unwrap();
        assert!(caches.borrow_pin(FINGERPRINT).unwrap().is_none());
    }
}
