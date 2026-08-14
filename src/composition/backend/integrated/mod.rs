mod entries;
mod git;
mod store;
#[cfg(test)]
mod tests;

pub use self::entries::git_commit_private_key_requiring_unlock_for_entry;
#[cfg(test)]
pub use self::entries::required_private_key_fingerprints_for_entry;
pub use self::git::git_commit_private_key_requiring_unlock_for_store_recipients;

pub use self::entries::{
    delete_password_entry, password_entry_is_readable, read_password_entry,
    read_password_entry_with_progress, read_password_line, rename_password_entry,
    save_password_entry,
};
pub(in crate::composition::backend) use self::store::try_initialize_empty_store_recipients;
pub use self::store::{
    save_store_recipients, save_store_recipients_for_relative_dir,
    store_recipients_private_key_requiring_unlock,
    store_recipients_private_key_requiring_unlock_for_relative_dir,
};
