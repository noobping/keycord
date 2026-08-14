//! Presentation metadata for Keys-owned managed-key protection kinds.

use crate::{ManagedRipassoPrivateKey, ManagedRipassoPrivateKeyProtection};
use keycord_runtime::i18n::gettext;

pub fn managed_key_subtitle(key: &ManagedRipassoPrivateKey) -> String {
    let template = match key.protection {
        ManagedRipassoPrivateKeyProtection::Password => "{fingerprint} - Password protected",
        ManagedRipassoPrivateKeyProtection::HardwareOpenPgpCard => "{fingerprint} - Hardware key",
        #[cfg(feature = "fido")]
        ManagedRipassoPrivateKeyProtection::Fido2HmacSecret => {
            "{fingerprint} - Security key protected"
        }
    };
    gettext(template).replace("{fingerprint}", &key.fingerprint)
}

pub const fn managed_key_copy_tooltip(key: &ManagedRipassoPrivateKey) -> &'static str {
    match key.protection {
        ManagedRipassoPrivateKeyProtection::Password => "Copy armored private key",
        ManagedRipassoPrivateKeyProtection::HardwareOpenPgpCard => "Copy armored public key",
        #[cfg(feature = "fido")]
        ManagedRipassoPrivateKeyProtection::Fido2HmacSecret => {
            "Copy experimental FIDO2-protected private key"
        }
    }
}
