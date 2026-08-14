use adw::gtk::Button;
use adw::{NavigationPage, NavigationView, WindowTitle};
use keycord_entries::ui::widgets::EntryWindowWidgets;
use keycord_keys::ui::KeyWindowWidgets;
use keycord_shell::navigation::{HasWindowChrome, WindowChrome};

#[derive(Clone)]
pub struct WindowNavigationState {
    pub nav: NavigationView,
    pub entries: EntryWindowWidgets,
    pub keys: KeyWindowWidgets,
    pub settings_page: NavigationPage,
    pub tools_page: NavigationPage,
    pub docs_page: NavigationPage,
    pub docs_detail_page: NavigationPage,
    pub tools_audit_page: NavigationPage,
    pub store_import_page: NavigationPage,
    pub log_page: NavigationPage,
    pub back: Button,
    pub add: Button,
    pub find: Button,
    pub primary_action: Button,
    pub secondary_action: Button,
    pub save: Button,
    pub raw: Button,
    pub title: WindowTitle,
}

macro_rules! impl_has_window_chrome {
    ($($state:ty),+ $(,)?) => {
        $(
            impl HasWindowChrome for $state {
                fn window_chrome(&self) -> WindowChrome<'_> {
                    WindowChrome {
                        back: &self.back,
                        add: &self.add,
                        find: &self.find,
                        primary_action: &self.primary_action,
                        secondary_action: &self.secondary_action,
                        save: &self.save,
                        raw: &self.raw,
                        title: &self.title,
                    }
                }
            }
        )+
    };
}

impl_has_window_chrome!(WindowNavigationState,);
