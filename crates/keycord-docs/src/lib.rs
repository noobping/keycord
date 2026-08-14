//! Bundled documentation and presentation for Keycord.

#[cfg(feature = "ui")]
use adw::gtk::{
    Box as GtkBox, Builder, Button, DirectionType, ListBox, ScrolledWindow, SearchEntry,
};
#[cfg(feature = "ui")]
use adw::prelude::*;
#[cfg(feature = "ui")]
use adw::{ActionRow, NavigationPage, NavigationView};
#[cfg(all(feature = "docs", feature = "ui"))]
use std::rc::Rc;

pub const DOCS_PAGE_TITLE: &str = "Documentation";
pub const DOCS_PAGE_SUBTITLE: &str = "Guides and reference";

pub const fn docs_available() -> bool {
    cfg!(feature = "docs")
}

#[cfg(test)]
mod capability_tests {
    #[test]
    fn availability_matches_the_docs_feature() {
        assert_eq!(super::docs_available(), cfg!(feature = "docs"));
    }
}

#[derive(Clone)]
#[cfg(feature = "ui")]
pub struct DocumentationWindowWidgets {
    pub tool_row: ActionRow,
    pub page: NavigationPage,
    pub search_entry: SearchEntry,
    pub list: ListBox,
    pub detail_page: NavigationPage,
    pub detail_scrolled: ScrolledWindow,
    pub detail_box: GtkBox,
}

#[cfg(feature = "ui")]
impl DocumentationWindowWidgets {
    pub fn load(builder: &Builder) -> Result<Self, String> {
        Ok(Self {
            tool_row: keycord_shell::ui::required_builder_object(builder, "tools_docs_row")?,
            page: keycord_shell::ui::required_builder_object(builder, "docs_page")?,
            search_entry: keycord_shell::ui::required_builder_object(builder, "docs_search_entry")?,
            list: keycord_shell::ui::required_builder_object(builder, "docs_list")?,
            detail_page: keycord_shell::ui::required_builder_object(builder, "docs_detail_page")?,
            detail_scrolled: keycord_shell::ui::required_builder_object(
                builder,
                "docs_detail_scrolled",
            )?,
            detail_box: keycord_shell::ui::required_builder_object(builder, "docs_detail_box")?,
        })
    }

    pub fn focus_index_target(&self) -> bool {
        keycord_shell::ui::focus_first_keyboard_focusable_list_row(&self.list)
            || self.search_entry.grab_focus()
    }

    pub fn configure_search_entries(&self) {
        keycord_shell::ui::configure_touch_friendly_search_entry(&self.search_entry);
    }

    pub fn toggle_find_for_visible_page(
        &self,
        navigation: &NavigationView,
        find_button: &Button,
    ) -> bool {
        if keycord_shell::ui::visible_navigation_page_is(navigation, &self.page) {
            keycord_shell::ui::toggle_page_search_entry(find_button, &self.search_entry);
            return true;
        }
        keycord_shell::ui::visible_navigation_page_is(navigation, &self.detail_page)
    }

    pub fn focus_detail_target(&self) -> bool {
        self.detail_box.child_focus(DirectionType::Down)
    }

    pub fn focus_first_visible_page_target(&self, navigation: &NavigationView) -> Option<bool> {
        if keycord_shell::ui::visible_navigation_page_is(navigation, &self.page) {
            return Some(self.focus_index_target());
        }
        if keycord_shell::ui::visible_navigation_page_is(navigation, &self.detail_page) {
            return Some(self.focus_detail_target());
        }
        None
    }

    pub fn visible_page_contains_focus(&self, navigation: &NavigationView) -> Option<bool> {
        for page in [&self.page, &self.detail_page] {
            if keycord_shell::ui::visible_navigation_page_is(navigation, page) {
                return Some(keycord_shell::ui::widget_contains_focus(
                    &page.clone().upcast(),
                ));
            }
        }
        None
    }
}

#[cfg(feature = "ui")]
pub fn documentation_index_presentation() -> keycord_shell::navigation::PagePresentation {
    keycord_shell::navigation::PagePresentation::secondary(
        DOCS_PAGE_TITLE,
        DOCS_PAGE_SUBTITLE,
        false,
    )
    .with_find_visible(true)
}

#[cfg(feature = "ui")]
pub fn documentation_detail_presentation(
    title: impl Into<String>,
) -> keycord_shell::navigation::PagePresentation {
    keycord_shell::navigation::PagePresentation::secondary(title, DOCS_PAGE_TITLE, false)
}

#[cfg(feature = "ui")]
pub fn configure_documentation_shortcuts(app: &adw::Application) {
    #[cfg(feature = "docs")]
    app.set_accels_for_action("win.open-docs", &["<primary><shift>d"]);

    #[cfg(not(feature = "docs"))]
    let _ = app;
}

#[cfg(all(feature = "docs", feature = "ui"))]
type ShowChrome = dyn Fn(&str, &str, bool);

/// The subject-neutral window controls needed by documentation navigation.
///
/// The composing application decides how its chrome is rendered; this crate
/// owns when the documentation index and detail chrome are selected.
#[derive(Clone)]
#[cfg(all(feature = "docs", feature = "ui"))]
pub struct DocumentationNavigation {
    pub(crate) nav: NavigationView,
    index_page: NavigationPage,
    show_chrome: Rc<ShowChrome>,
}

#[cfg(all(feature = "docs", feature = "ui"))]
impl DocumentationNavigation {
    pub fn new(
        nav: &NavigationView,
        index_page: &NavigationPage,
        show_chrome: impl Fn(&str, &str, bool) + 'static,
    ) -> Self {
        Self {
            nav: nav.clone(),
            index_page: index_page.clone(),
            show_chrome: Rc::new(show_chrome),
        }
    }

    pub(crate) fn show_index(&self) {
        (self.show_chrome)(DOCS_PAGE_TITLE, DOCS_PAGE_SUBTITLE, true);
        keycord_shell::ui::push_navigation_page_if_needed(&self.nav, &self.index_page);
    }

    pub(crate) fn show_detail(&self, title: &str) {
        (self.show_chrome)(title, DOCS_PAGE_TITLE, false);
    }
}

/// No-op navigation input retained when bundled documentation is disabled.
#[derive(Clone, Default)]
#[cfg(all(not(feature = "docs"), feature = "ui"))]
pub struct DocumentationNavigation;

#[cfg(all(not(feature = "docs"), feature = "ui"))]
impl DocumentationNavigation {
    pub fn new(
        _nav: &NavigationView,
        _index_page: &NavigationPage,
        _show_chrome: impl Fn(&str, &str, bool) + 'static,
    ) -> Self {
        Self
    }
}

#[cfg(all(feature = "docs", feature = "ui"))]
mod enabled;
#[cfg(all(feature = "docs", feature = "ui"))]
use enabled::DocumentationPageWidgets;
#[cfg(all(feature = "docs", feature = "ui"))]
pub use enabled::{register_open_docs_action, DocumentationPageState};

#[cfg(all(not(feature = "docs"), feature = "ui"))]
mod disabled;
#[cfg(all(not(feature = "docs"), feature = "ui"))]
use disabled::DocumentationPageWidgets;
#[cfg(all(not(feature = "docs"), feature = "ui"))]
pub use disabled::{register_open_docs_action, DocumentationPageState};

#[cfg(feature = "ui")]
impl DocumentationWindowWidgets {
    /// Build Docs state from its owner bundle and application-supplied navigation.
    pub fn page_state(&self, navigation: DocumentationNavigation) -> DocumentationPageState {
        DocumentationPageState::new(DocumentationPageWidgets::new(
            navigation,
            &self.search_entry,
            &self.list,
            &self.detail_page,
            &self.detail_scrolled,
            &self.detail_box,
        ))
    }
}
