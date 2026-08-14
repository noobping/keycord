//! Composition-only registry for subject-owned window widget bundles.

use adw::gtk::Builder;
use keycord_docs::DocumentationWindowWidgets;
use keycord_entries::ui::widgets::EntryWindowWidgets;
#[cfg(feature = "fidokey")]
use keycord_fido::ui::FidoWindowWidgets;
use keycord_git::ui::GitWindowWidgets;
use keycord_keys::ui::KeyWindowWidgets;
use keycord_preferences::ui::PreferencesWindowWidgets;
use keycord_shell::ShellWindowWidgets;
use keycord_stores::ui::StoresWindowWidgets;

use crate::window::tool_hub::ToolHubWindowWidgets;

#[derive(Clone)]
pub(in crate::window) struct WindowWidgets {
    pub(in crate::window) shell: ShellWindowWidgets,
    pub(in crate::window) entries: EntryWindowWidgets,
    pub(in crate::window) stores: StoresWindowWidgets,
    pub(in crate::window) git: GitWindowWidgets,
    pub(in crate::window) docs: DocumentationWindowWidgets,
    pub(in crate::window) preferences: PreferencesWindowWidgets,
    pub(in crate::window) keys: KeyWindowWidgets,
    #[cfg(feature = "fidokey")]
    pub(in crate::window) fido: FidoWindowWidgets,
    pub(in crate::window) tool_hub: ToolHubWindowWidgets,
}

impl WindowWidgets {
    pub(in crate::window) fn load(builder: &Builder) -> Result<Self, String> {
        Ok(Self {
            shell: ShellWindowWidgets::load(builder)?,
            entries: EntryWindowWidgets::load(builder)?,
            stores: StoresWindowWidgets::load(builder)?,
            git: GitWindowWidgets::load(builder)?,
            docs: DocumentationWindowWidgets::load(builder)?,
            preferences: PreferencesWindowWidgets::load(builder)?,
            keys: KeyWindowWidgets::load(builder)?,
            #[cfg(feature = "fidokey")]
            fido: FidoWindowWidgets::load(builder)?,
            tool_hub: ToolHubWindowWidgets::load(builder)?,
        })
    }
}
