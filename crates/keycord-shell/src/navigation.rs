//! Feature-neutral navigation, page restoration, and window chrome.

use adw::gtk::Button;
use adw::prelude::*;
use adw::{ApplicationWindow, NavigationPage, NavigationView, WindowTitle};
use keycord_runtime::i18n::gettext;
use std::rc::Rc;

pub const APP_WINDOW_TITLE: &str = "Keycord";

#[derive(Clone)]
pub struct WindowPageState {
    pub window: ApplicationWindow,
    pub nav: NavigationView,
    pub page: NavigationPage,
    pub back: Button,
    pub add: Button,
    pub find: Button,
    pub primary_action: Button,
    pub secondary_action: Button,
    pub save: Button,
    pub raw: Button,
    pub title: WindowTitle,
}

#[derive(Clone, Copy)]
pub struct WindowChrome<'a> {
    pub back: &'a Button,
    pub add: &'a Button,
    pub find: &'a Button,
    pub primary_action: &'a Button,
    pub secondary_action: &'a Button,
    pub save: &'a Button,
    pub raw: &'a Button,
    pub title: &'a WindowTitle,
}

pub trait HasWindowChrome {
    fn window_chrome(&self) -> WindowChrome<'_>;
}

impl HasWindowChrome for WindowPageState {
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

/// Visibility supplied by the application for actions shown on its root page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrimaryPageActionVisibility {
    pub add: bool,
    pub find: bool,
    pub primary: bool,
    pub secondary: bool,
}

pub fn show_primary_page_chrome(
    chrome: &WindowChrome<'_>,
    actions: PrimaryPageActionVisibility,
    title: &str,
    subtitle: &str,
) {
    chrome.back.set_visible(false);
    chrome.save.set_visible(false);
    chrome.add.set_visible(actions.add);
    chrome.find.set_visible(actions.find);
    chrome.primary_action.set_visible(actions.primary);
    chrome.secondary_action.set_visible(actions.secondary);
    chrome.title.set_title(&gettext(title));
    chrome.title.set_subtitle(&gettext(subtitle));
    chrome.raw.set_visible(false);
}

pub fn show_secondary_page_chrome(
    chrome: &WindowChrome<'_>,
    title: &str,
    subtitle: &str,
    save_visible: bool,
) {
    chrome.back.set_visible(true);
    chrome.add.set_visible(false);
    chrome.find.set_visible(false);
    chrome.primary_action.set_visible(false);
    chrome.secondary_action.set_visible(false);
    chrome.save.set_visible(save_visible);
    chrome.raw.set_visible(false);
    chrome.title.set_title(&gettext(title));
    chrome.title.set_subtitle(&gettext(subtitle));
}

/// Stable identity assigned by the composing application to a navigation page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NavigationPageId(&'static str);

impl NavigationPageId {
    pub const fn new(value: &'static str) -> Self {
        Self(value)
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PagePresentation {
    pub title: String,
    pub subtitle: String,
    pub save_visible: bool,
    pub find_visible: bool,
    pub raw_visible: bool,
}

impl PagePresentation {
    pub fn secondary(
        title: impl Into<String>,
        subtitle: impl Into<String>,
        save_visible: bool,
    ) -> Self {
        Self {
            title: title.into(),
            subtitle: subtitle.into(),
            save_visible,
            find_visible: false,
            raw_visible: false,
        }
    }

    pub const fn with_find_visible(mut self, visible: bool) -> Self {
        self.find_visible = visible;
        self
    }

    pub const fn with_raw_visible(mut self, visible: bool) -> Self {
        self.raw_visible = visible;
        self
    }
}

pub type NavigationRestoreCallback = Rc<dyn Fn()>;
pub type WindowChromeCallback = Rc<dyn for<'a> Fn(&WindowChrome<'a>)>;

#[derive(Clone)]
enum NavigationRestoreBehavior {
    Secondary(PagePresentation),
    Callback(NavigationRestoreCallback),
}

#[derive(Clone)]
pub struct NavigationPageRoute {
    id: NavigationPageId,
    page: NavigationPage,
    behavior: NavigationRestoreBehavior,
    after_restore: Option<NavigationRestoreCallback>,
}

impl NavigationPageRoute {
    pub fn secondary(
        id: NavigationPageId,
        page: &NavigationPage,
        presentation: PagePresentation,
    ) -> Self {
        Self {
            id,
            page: page.clone(),
            behavior: NavigationRestoreBehavior::Secondary(presentation),
            after_restore: None,
        }
    }

    pub fn callback(
        id: NavigationPageId,
        page: &NavigationPage,
        restore: impl Fn() + 'static,
    ) -> Self {
        Self {
            id,
            page: page.clone(),
            behavior: NavigationRestoreBehavior::Callback(Rc::new(restore)),
            after_restore: None,
        }
    }

    pub fn with_after_restore(mut self, after_restore: impl Fn() + 'static) -> Self {
        self.after_restore = Some(Rc::new(after_restore));
        self
    }

    pub const fn id(&self) -> NavigationPageId {
        self.id
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationRestoreTarget {
    Root,
    Page(NavigationPageId),
    Other,
}

impl NavigationRestoreTarget {
    pub const fn is_root(self) -> bool {
        matches!(self, Self::Root)
    }
}

const fn restore_target(
    before_root_page: Option<NavigationPageId>,
    at_root: bool,
    current_page: Option<NavigationPageId>,
) -> NavigationRestoreTarget {
    if let Some(page) = before_root_page {
        return NavigationRestoreTarget::Page(page);
    }
    if at_root {
        NavigationRestoreTarget::Root
    } else {
        match current_page {
            Some(page) => NavigationRestoreTarget::Page(page),
            None => NavigationRestoreTarget::Other,
        }
    }
}

fn visible_route<'a>(
    nav: &NavigationView,
    routes: &'a [NavigationPageRoute],
) -> Option<&'a NavigationPageRoute> {
    routes
        .iter()
        .find(|route| visible_navigation_page_is(nav, &route.page))
}

fn apply_route(chrome: &WindowChrome<'_>, route: &NavigationPageRoute) {
    match &route.behavior {
        NavigationRestoreBehavior::Secondary(presentation) => {
            show_page_presentation(chrome, presentation)
        }
        NavigationRestoreBehavior::Callback(restore) => restore(),
    }

    if let Some(after_restore) = route.after_restore.as_ref() {
        after_restore();
    }
}

/// Restores chrome and subject callbacks for the currently visible page.
///
/// `before_root` preserves exceptional pages that must win before root-stack
/// detection. `routes` are matched in their supplied, stable priority order.
pub fn restore_navigation_for_current_page(
    nav: &NavigationView,
    chrome: &WindowChrome<'_>,
    restore_root: &WindowChromeCallback,
    before_root: &[NavigationPageRoute],
    routes: &[NavigationPageRoute],
) -> NavigationRestoreTarget {
    let before_root_route = visible_route(nav, before_root);
    let current_route = visible_route(nav, routes);
    let target = restore_target(
        before_root_route.map(NavigationPageRoute::id),
        navigation_stack_is_root(nav),
        current_route.map(NavigationPageRoute::id),
    );

    if let Some(route) = before_root_route {
        apply_route(chrome, route);
        return target;
    }

    match target {
        NavigationRestoreTarget::Root => restore_root(chrome),
        NavigationRestoreTarget::Page(id) => {
            chrome.save.set_visible(false);
            if let Some(route) = current_route.filter(|route| route.id == id) {
                apply_route(chrome, route);
            }
        }
        NavigationRestoreTarget::Other => chrome.save.set_visible(false),
    }
    target
}

pub fn show_navigation_page(
    nav: &NavigationView,
    page: &NavigationPage,
    chrome: &WindowChrome<'_>,
    presentation: &PagePresentation,
) -> bool {
    show_page_presentation(chrome, presentation);
    push_navigation_page_if_needed(nav, page)
}

pub fn show_page_presentation(chrome: &WindowChrome<'_>, presentation: &PagePresentation) {
    show_secondary_page_chrome(
        chrome,
        &presentation.title,
        &presentation.subtitle,
        presentation.save_visible,
    );
    chrome.find.set_visible(presentation.find_visible);
    chrome.raw.set_visible(presentation.raw_visible);
}

/// Removes a transient page while preserving whichever page is currently on top.
pub fn finish_transient_navigation_page(nav: &NavigationView, transient_page: &NavigationPage) {
    let current_page = nav.visible_page();
    let transient_visible = visible_navigation_page_is(nav, transient_page);
    let transient_in_stack = navigation_stack_contains_page(nav, transient_page);

    if transient_visible {
        nav.pop();
    } else if transient_in_stack {
        if let Some(current_page) = current_page.filter(|page| page != transient_page) {
            let _ = nav.pop_to_page(transient_page);
            let _ = nav.pop();
            nav.push(&current_page);
        }
    }
}

pub fn navigation_stack_contains_page(nav: &NavigationView, page: &NavigationPage) -> bool {
    let stack = nav.navigation_stack();
    let mut index = 0;
    let len = stack.n_items();
    while index < len {
        if let Some(item) = stack.item(index) {
            if let Ok(stack_page) = item.downcast::<NavigationPage>() {
                if stack_page == *page {
                    return true;
                }
            }
        }
        index += 1;
    }

    false
}

pub fn visible_navigation_page_is(nav: &NavigationView, page: &NavigationPage) -> bool {
    nav.visible_page()
        .as_ref()
        .is_some_and(|visible| visible == page)
}

pub fn push_navigation_page_if_needed(nav: &NavigationView, page: &NavigationPage) -> bool {
    if visible_navigation_page_is(nav, page) {
        return false;
    }

    nav.push(page);
    true
}

pub fn reveal_navigation_page(nav: &NavigationView, page: &NavigationPage) -> bool {
    if visible_navigation_page_is(nav, page) {
        return false;
    }

    if navigation_stack_contains_page(nav, page) {
        let _ = nav.pop_to_page(page);
    } else {
        nav.push(page);
    }

    true
}

pub fn navigation_stack_is_root(nav: &NavigationView) -> bool {
    nav.navigation_stack().n_items() <= 1
}

pub fn pop_navigation_to_root(nav: &NavigationView) {
    while !navigation_stack_is_root(nav) {
        nav.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::{restore_target, NavigationPageId, NavigationRestoreTarget, PagePresentation};

    const EXAMPLE_PAGE: NavigationPageId = NavigationPageId::new("example");

    #[test]
    fn root_wins_before_a_regular_visible_page() {
        assert_eq!(
            restore_target(None, true, Some(EXAMPLE_PAGE)),
            NavigationRestoreTarget::Root
        );
    }

    #[test]
    fn before_root_page_wins_over_root_stack_detection() {
        let modal_page = NavigationPageId::new("modal");
        assert_eq!(
            restore_target(Some(modal_page), true, Some(EXAMPLE_PAGE)),
            NavigationRestoreTarget::Page(modal_page)
        );
    }

    #[test]
    fn visible_page_identity_is_preserved() {
        assert_eq!(
            restore_target(None, false, Some(EXAMPLE_PAGE)),
            NavigationRestoreTarget::Page(EXAMPLE_PAGE)
        );
        assert_eq!(EXAMPLE_PAGE.as_str(), "example");
    }

    #[test]
    fn missing_page_mapping_falls_back_to_other() {
        assert_eq!(
            restore_target(None, false, None),
            NavigationRestoreTarget::Other
        );
    }

    #[test]
    fn secondary_presentation_defaults_to_hidden_optional_actions() {
        assert_eq!(
            PagePresentation::secondary("Title", "Subtitle", true),
            PagePresentation {
                title: "Title".to_string(),
                subtitle: "Subtitle".to_string(),
                save_visible: true,
                find_visible: false,
                raw_visible: false,
            }
        );
    }
}
