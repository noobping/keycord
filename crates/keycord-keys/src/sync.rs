#[cfg(target_os = "linux")]
use crate::{
    armored_ripasso_private_key, list_ripasso_private_keys, remove_ripasso_private_key,
    ripasso_private_key_requires_passphrase, store_ripasso_private_key_bytes,
    ManagedRipassoPrivateKeyProtection,
};

#[cfg(target_os = "linux")]
use keycord_runtime::diagnostics::log_info;

#[cfg(target_os = "linux")]
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivateKeySyncDirection {
    HostToApp,
    AppToHost,
}

/// Host-GPG operations supplied by the composing application.
///
/// The Keys subject exchanges only fingerprints and armored private-key bytes,
/// so it does not depend on the application's backend models or commands.
pub trait HostPrivateKeySyncPort {
    fn list_private_key_fingerprints(&self) -> Result<Vec<String>, String>;
    fn export_private_key(&self, fingerprint: &str) -> Result<String, String>;
    fn import_private_key(&self, bytes: &[u8]) -> Result<(), String>;
    fn delete_private_key(&self, fingerprint: &str) -> Result<(), String>;
}

pub fn preflight_host_to_app_private_key_sync(
    host: &dyn HostPrivateKeySyncPort,
) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let app_fingerprints = app_private_key_fingerprints()?;
        for fingerprint in host_private_key_fingerprints(host)? {
            if app_fingerprints.contains(&normalized_fingerprint(&fingerprint)) {
                continue;
            }

            let _ = syncable_host_private_key_export(host, &fingerprint, false)?;
        }

        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = host;
        Err("Private-key sync with the host is only available on Linux.".to_string())
    }
}

pub fn sync_private_keys_with_host(
    host: &dyn HostPrivateKeySyncPort,
    direction: PrivateKeySyncDirection,
) -> Result<(), String> {
    match direction {
        PrivateKeySyncDirection::HostToApp => sync_host_private_keys_to_app(host),
        PrivateKeySyncDirection::AppToHost => sync_app_private_keys_to_host(host),
    }
}

#[cfg(target_os = "linux")]
fn app_private_key_fingerprints() -> Result<HashSet<String>, String> {
    Ok(list_ripasso_private_keys()?
        .into_iter()
        .map(|key| normalized_fingerprint(&key.fingerprint))
        .collect())
}

#[cfg(target_os = "linux")]
fn host_private_key_fingerprints(
    host: &dyn HostPrivateKeySyncPort,
) -> Result<HashSet<String>, String> {
    Ok(host
        .list_private_key_fingerprints()?
        .into_iter()
        .map(|fingerprint| normalized_fingerprint(&fingerprint))
        .collect())
}

#[cfg(target_os = "linux")]
fn normalized_fingerprint(fingerprint: &str) -> String {
    fingerprint.trim().to_ascii_lowercase()
}

#[cfg(target_os = "linux")]
fn syncable_host_private_key_export(
    host: &dyn HostPrivateKeySyncPort,
    fingerprint: &str,
    log_skip: bool,
) -> Result<Option<String>, String> {
    let armored = host.export_private_key(fingerprint)?;
    if ripasso_private_key_requires_passphrase(armored.as_bytes()).map_err(|err| err.to_string())? {
        return Ok(Some(armored));
    }

    if log_skip {
        log_info(format!(
            "Skipping host GPG private key without a passphrase during sync: {fingerprint}"
        ));
    }
    Ok(None)
}

fn sync_host_private_keys_to_app(host: &dyn HostPrivateKeySyncPort) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let host_keys = host.list_private_key_fingerprints()?;
        let app_keys = list_ripasso_private_keys()?;
        let host_fingerprints = host_keys
            .iter()
            .map(|fingerprint| normalized_fingerprint(fingerprint))
            .collect::<HashSet<_>>();
        let app_fingerprints = app_keys
            .iter()
            .map(|key| normalized_fingerprint(&key.fingerprint))
            .collect::<HashSet<_>>();

        let mut host_exports = Vec::new();
        for fingerprint in host_keys
            .into_iter()
            .filter(|fingerprint| !app_fingerprints.contains(&normalized_fingerprint(fingerprint)))
        {
            let Some(armored) = syncable_host_private_key_export(host, &fingerprint, true)? else {
                continue;
            };
            host_exports.push((fingerprint, armored));
        }

        for (_, armored) in host_exports {
            store_ripasso_private_key_bytes(armored.as_bytes()).map_err(|err| err.to_string())?;
        }

        for key in app_keys {
            if !matches!(key.protection, ManagedRipassoPrivateKeyProtection::Password) {
                continue;
            }
            if !host_fingerprints.contains(&normalized_fingerprint(&key.fingerprint)) {
                remove_ripasso_private_key(&key.fingerprint)?;
            }
        }

        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = host;
        Err("Private-key sync with the host is only available on Linux.".to_string())
    }
}

fn sync_app_private_keys_to_host(host: &dyn HostPrivateKeySyncPort) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let app_keys = list_ripasso_private_keys()?;
        let host_keys = host.list_private_key_fingerprints()?;
        let app_fingerprints = app_keys
            .iter()
            .map(|key| normalized_fingerprint(&key.fingerprint))
            .collect::<HashSet<_>>();
        let host_fingerprints = host_keys
            .iter()
            .map(|fingerprint| normalized_fingerprint(fingerprint))
            .collect::<HashSet<_>>();

        let app_exports = app_keys
            .into_iter()
            .filter(|key| matches!(key.protection, ManagedRipassoPrivateKeyProtection::Password))
            .filter(|key| !host_fingerprints.contains(&normalized_fingerprint(&key.fingerprint)))
            .map(|key| {
                armored_ripasso_private_key(&key.fingerprint)
                    .map(|armored| (key.fingerprint, armored))
            })
            .collect::<Result<Vec<_>, String>>()?;

        for (_, armored) in app_exports {
            host.import_private_key(armored.as_bytes())?;
        }

        for fingerprint in host_keys {
            if !app_fingerprints.contains(&normalized_fingerprint(&fingerprint)) {
                host.delete_private_key(&fingerprint)?;
            }
        }

        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = host;
        Err("Private-key sync with the host is only available on Linux.".to_string())
    }
}
