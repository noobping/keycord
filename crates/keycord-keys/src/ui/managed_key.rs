//! Presentation metadata for Keys-owned managed-key protection kinds.

use crate::{ManagedRipassoPrivateKey, ManagedRipassoPrivateKeyProtection};
use keycord_runtime::i18n::gettext;

pub fn managed_key_subtitle(key: &ManagedRipassoPrivateKey) -> String {
    let template = match key.protection {
        ManagedRipassoPrivateKeyProtection::Password => "{fingerprint} - Password protected",
        ManagedRipassoPrivateKeyProtection::HardwareOpenPgpCard => "{fingerprint} - Hardware key",
    };
    gettext(template).replace("{fingerprint}", &key.fingerprint)
}

pub const fn managed_key_copy_tooltip(key: &ManagedRipassoPrivateKey) -> &'static str {
    match key.protection {
        ManagedRipassoPrivateKeyProtection::Password => "Copy armored private key",
        ManagedRipassoPrivateKeyProtection::HardwareOpenPgpCard => "Copy armored public key",
    }
}
