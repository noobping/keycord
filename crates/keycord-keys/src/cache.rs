use super::cert::normalized_fingerprint;
use super::hardware::HardwareSessionPolicy;
use sequoia_openpgp::Cert;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant};

const SECRET_CACHE_IDLE_TIMEOUT: Duration = Duration::from_secs(15 * 60);

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

    fn is_expired_at(&self, now: Instant) -> bool {
        now.duration_since(self.last_secret_use) >= SECRET_CACHE_IDLE_TIMEOUT
    }
}

struct SecretCache<T> {
    entries: RwLock<HashMap<String, CacheEntry<T>>>,
}

impl<T> SecretCache<T> {
    fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }

    fn with_write<R>(
        &self,
        f: impl FnOnce(&mut HashMap<String, CacheEntry<T>>, Instant) -> R,
    ) -> R {
        match self.entries.write() {
            Ok(mut entries) => {
                let now = Instant::now();
                Self::prune_expired_entries(&mut entries, now);
                f(&mut entries, now)
            }
            Err(poisoned) => {
                let mut entries = poisoned.into_inner();
                let now = Instant::now();
                Self::prune_expired_entries(&mut entries, now);
                f(&mut entries, now)
            }
        }
    }

    fn prune_expired_entries(entries: &mut HashMap<String, CacheEntry<T>>, now: Instant) {
        entries.retain(|_, entry| !entry.is_expired_at(now));
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

    #[cfg(test)]
    fn expire_for_tests(&self, fingerprint: &str) {
        self.with_write(|entries, _| {
            let entry = entries
                .get_mut(fingerprint)
                .expect("cache entry should exist");
            entry.last_secret_use -= SECRET_CACHE_IDLE_TIMEOUT + Duration::from_secs(1);
        });
    }
}

impl<T: Clone> SecretCache<T> {
    fn peek(&self, fingerprint: &str) -> Option<T> {
        self.with_write(|entries, _| entries.get(fingerprint).map(|entry| entry.value.clone()))
    }

    fn borrow(&self, fingerprint: &str) -> Option<T> {
        self.with_write(|entries, now| {
            let entry = entries.get_mut(fingerprint)?;
            entry.last_secret_use = now;
            Some(entry.value.clone())
        })
    }
}

fn unlocked_ripasso_private_keys() -> &'static SecretCache<Arc<Cert>> {
    static UNLOCKED_KEYS: OnceLock<SecretCache<Arc<Cert>>> = OnceLock::new();
    UNLOCKED_KEYS.get_or_init(SecretCache::new)
}

fn unlocked_hardware_private_keys() -> &'static SecretCache<HardwareSessionPolicy> {
    static UNLOCKED_KEYS: OnceLock<SecretCache<HardwareSessionPolicy>> = OnceLock::new();
    UNLOCKED_KEYS.get_or_init(SecretCache::new)
}

pub(crate) fn peek_unlocked_ripasso_private_key(
    fingerprint: &str,
) -> Result<Option<Arc<Cert>>, String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    Ok(unlocked_ripasso_private_keys().peek(&fingerprint))
}

pub fn borrow_unlocked_ripasso_private_key(fingerprint: &str) -> Result<Option<Arc<Cert>>, String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    Ok(unlocked_ripasso_private_keys().borrow(&fingerprint))
}

pub(crate) fn cache_unlocked_ripasso_private_key(cert: Cert) {
    unlocked_ripasso_private_keys().insert(cert.fingerprint().to_hex(), Arc::new(cert));
}

pub(crate) fn peek_unlocked_hardware_private_key(
    fingerprint: &str,
) -> Result<Option<HardwareSessionPolicy>, String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    Ok(unlocked_hardware_private_keys().peek(&fingerprint))
}

pub fn borrow_unlocked_hardware_private_key(
    fingerprint: &str,
) -> Result<Option<HardwareSessionPolicy>, String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    Ok(unlocked_hardware_private_keys().borrow(&fingerprint))
}

pub(crate) fn cache_unlocked_hardware_private_key(
    fingerprint: &str,
    session: HardwareSessionPolicy,
) -> Result<(), String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    unlocked_hardware_private_keys().insert(fingerprint, session);
    Ok(())
}

#[cfg(feature = "fido")]
pub(crate) fn clear_cached_fido2_pin(fingerprint: &str) -> Result<(), String> {
    super::fido2::remove_cached_fido2_secrets(fingerprint)
}

pub(crate) fn remove_cached_unlocked_ripasso_private_key(fingerprint: &str) -> Result<(), String> {
    let fingerprint = normalized_fingerprint(fingerprint)?;
    unlocked_ripasso_private_keys().remove(&fingerprint);
    unlocked_hardware_private_keys().remove(&fingerprint);
    #[cfg(feature = "fido")]
    super::fido2::remove_cached_fido2_secrets(&fingerprint)?;
    Ok(())
}

pub fn clear_integrated_runtime_secret_state() {
    unlocked_ripasso_private_keys().clear();
    unlocked_hardware_private_keys().clear();
    #[cfg(feature = "fido")]
    super::fido2::clear_cached_fido2_secrets();
}

#[cfg(any(test, feature = "test-support"))]
pub fn clear_cached_unlocked_ripasso_private_keys() {
    clear_integrated_runtime_secret_state();
}

#[cfg(test)]
mod tests {
    use super::{
        borrow_unlocked_ripasso_private_key, cache_unlocked_ripasso_private_key,
        clear_integrated_runtime_secret_state, peek_unlocked_ripasso_private_key,
        unlocked_ripasso_private_keys,
    };
    use sequoia_openpgp::Cert;

    fn test_cert() -> Cert {
        let (cert, _) = sequoia_openpgp::cert::CertBuilder::general_purpose(Some("Cache Test"))
            .generate()
            .expect("generate test cert");
        cert
    }

    fn expire_ripasso_entry(fingerprint: &str) {
        unlocked_ripasso_private_keys().expire_for_tests(fingerprint);
    }

    #[test]
    fn peek_prunes_expired_ripasso_entries_without_refreshing() {
        let _guard = crate::test_support::lock();
        clear_integrated_runtime_secret_state();
        let cert = test_cert();
        let fingerprint = cert.fingerprint().to_hex();
        cache_unlocked_ripasso_private_key(cert);
        expire_ripasso_entry(&fingerprint);

        assert!(peek_unlocked_ripasso_private_key(&fingerprint)
            .expect("peek cache")
            .is_none());
        assert!(borrow_unlocked_ripasso_private_key(&fingerprint)
            .expect("borrow cache")
            .is_none());
    }

    #[test]
    fn borrow_refreshes_secret_use_for_ripasso_entries() {
        let _guard = crate::test_support::lock();
        clear_integrated_runtime_secret_state();
        let cert = test_cert();
        let fingerprint = cert.fingerprint().to_hex();
        cache_unlocked_ripasso_private_key(cert.clone());
        expire_ripasso_entry(&fingerprint);

        // Reinsert with a fresh timestamp, then make sure borrow keeps it alive.
        cache_unlocked_ripasso_private_key(cert);
        let borrowed = borrow_unlocked_ripasso_private_key(&fingerprint)
            .expect("borrow cache")
            .expect("entry should exist");
        assert_eq!(borrowed.fingerprint().to_hex(), fingerprint);
        assert!(peek_unlocked_ripasso_private_key(&fingerprint)
            .expect("peek cache")
            .is_some());
    }

    #[test]
    fn shutdown_cleanup_clears_runtime_secret_state() {
        let _guard = crate::test_support::lock();
        clear_integrated_runtime_secret_state();
        let cert = test_cert();
        let fingerprint = cert.fingerprint().to_hex();
        cache_unlocked_ripasso_private_key(cert);

        clear_integrated_runtime_secret_state();

        assert!(peek_unlocked_ripasso_private_key(&fingerprint)
            .expect("peek cache")
            .is_none());
    }
}
