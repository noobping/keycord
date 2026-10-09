use std::fmt;
use zeroize::Zeroize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportRp {
    pub id: String,
    pub name: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportUser {
    pub id: Vec<u8>,
    pub name: String,
    pub display_name: Option<String>,
}

/// Import data is independent of either on-disk schema.
#[derive(Clone, PartialEq, Eq)]
pub struct ImportedCredential {
    pub id: Vec<u8>,
    pub rp: ImportRp,
    pub user: ImportUser,
    pub alg: i32,
    pub private_key: Vec<u8>,
    pub created: i64,
}
impl Drop for ImportedCredential {
    fn drop(&mut self) {
        self.private_key.zeroize();
    }
}
impl fmt::Debug for ImportedCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImportedCredential")
            .field("rp", &self.rp)
            .field("private_key", &"[redacted]")
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PasskeyFormat {
    #[cfg(feature = "passkey")]
    #[default]
    Android,
    #[cfg(feature = "passless")]
    #[cfg_attr(not(feature = "passkey"), default)]
    Passless,
}
pub struct PreparedPasskey {
    pub label: String,
    pub contents: zeroize::Zeroizing<Vec<u8>>,
}
impl ImportedCredential {
    #[cfg(feature = "passkey")]
    pub(crate) fn android(&self) -> Result<crate::PasskeyCredential, String> {
        let value = crate::PasskeyCredential {
            id: self.id.clone(),
            rp: crate::RelyingParty {
                id: self.rp.id.clone(),
                name: self.rp.name.clone(),
            },
            user: crate::UserInfo {
                id: self.user.id.clone(),
                name: self.user.name.clone(),
                display_name: self.user.display_name.clone(),
                reveal_name: false,
            },
            sign_count: 0,
            alg: self.alg,
            private_key: self.private_key.clone(),
            created: self.created,
            zone: "UTC".into(),
        };
        value.validate()?;
        Ok(value)
    }
    pub fn prepare(&self, format: PasskeyFormat) -> Result<PreparedPasskey, String> {
        match format {
            #[cfg(feature = "passkey")]
            PasskeyFormat::Android => {
                let entry = crate::build_passkey_storage_entry(&self.android()?)?;
                Ok(PreparedPasskey {
                    label: entry.label,
                    contents: zeroize::Zeroizing::new(entry.contents.into_bytes()),
                })
            }
            #[cfg(feature = "passless")]
            PasskeyFormat::Passless => crate::passless::prepare_import(self),
        }
    }
}
