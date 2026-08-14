//! Connects Keys' synchronization workflow to the configured host-GPG adapter.

use keycord_keys::PrivateKeySyncDirection;

pub fn private_key_sync_enabled() -> bool {
    keycord_keys::host_private_key_sync_available()
        && keycord_preferences::Preferences::new().sync_private_keys_with_host()
}

pub fn preflight_host_to_app_private_key_sync() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        keycord_keys::preflight_host_to_app_private_key_sync(
            &crate::composition::backend::host_gpg_backend(),
        )
    }

    #[cfg(not(target_os = "linux"))]
    Err("Private-key sync with the host is only available on Linux.".to_string())
}

pub fn sync_private_keys_with_host(direction: PrivateKeySyncDirection) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        keycord_keys::sync_private_keys_with_host(
            &crate::composition::backend::host_gpg_backend(),
            direction,
        )
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = direction;
        Err("Private-key sync with the host is only available on Linux.".to_string())
    }
}
