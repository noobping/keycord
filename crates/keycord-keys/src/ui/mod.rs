//! Specialized private-key dialogs and unlock workflows.

mod dialog;
mod key_management;
mod managed_key;
mod smartcard_access;
mod unlock;
mod widgets;

pub use dialog::{
    present_private_key_password_dialog, present_private_key_password_dialog_with_close_handler,
    present_private_key_unlock_dialog_with_close_handler, PrivateKeyDialogHandle,
};
pub use key_management::{
    BeforeRecipientKeyRows, CopyKeyText, DisableKeySync, HideKeyNotice, IsKeyNoticeHidden,
    KeyManagementUiParts, KeyManagementUiPorts, KeyManagementUiState, KeyRecipientWorkflowPorts,
    KeySyncEnabled, ListHostPrivateKeys, PromptKeyUnlock, ReadKeyText, ReadKeyTextResult,
    RecipientKeyChoiceVisibility, RecipientKeyDeleteMessage, RecipientKeyListContext,
    RecipientKeyListPolicy, RecipientKeyMatcher, RecipientKeyToggle, RecipientKeyToggleMessage,
    RefreshKeyConsumers, SyncOptionalKeyAccess, SyncPrivateKeys,
};
pub use managed_key::{managed_key_copy_tooltip, managed_key_subtitle};
pub use smartcard_access::sync_hardware_key_access;
#[cfg(feature = "flatpak")]
pub use smartcard_access::{
    flatpak_smartcard_override_args, flatpak_smartcard_override_command,
    sync_hardware_key_access_with_flatpak, SmartcardAccessPorts, SMARTCARD_ACCESS_NOTICE_ID,
    SMARTCARD_ACCESS_ROW_NAME, SMARTCARD_PERMISSION_CONTEXT,
};
pub use unlock::{prompt_private_key_unlock_for_action, PrivateKeyUnlockUiPorts};
pub use widgets::{
    hardware_key_generation_presentation, key_generation_navigation_routes,
    private_key_generation_presentation, KeyWindowWidgets, HARDWARE_KEY_GENERATION_PAGE_ID,
    PRIVATE_KEY_GENERATION_PAGE_ID,
};
