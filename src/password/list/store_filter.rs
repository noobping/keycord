use super::search::search_controller_for_list;
use crate::filters::{
    build_filter_toggle, reconciled_included_filter_values, update_included_filter_value,
};
use crate::i18n::gettext;
use crate::logging::log_error;
use crate::preferences::Preferences;
use crate::store::labels::{shortened_store_label_for_path, shortened_store_label_map};
use crate::support::ui::{clear_box_children, navigation_stack_is_root};
use adw::gtk::{Box as GtkBox, ListBox, MenuButton, Popover};
use adw::prelude::*;
use adw::{NavigationView, Toast, ToastOverlay};
use std::collections::BTreeSet;

fn render_password_list_store_filter(
    filter_store_box: &GtkBox,
    list: &ListBox,
    overlay: &ToastOverlay,
) {
    clear_box_children(filter_store_box);

    let preferences = Preferences::new();
    let store_roots = preferences.store_roots();
    let available = store_roots.iter().cloned().collect::<BTreeSet<_>>();
    let store_labels = shortened_store_label_map(&store_roots);
    let stored_included = preferences
        .filter_included_store_roots()
        .map(|roots| roots.into_iter().collect::<BTreeSet<_>>());
    let included = reconciled_included_filter_values(stored_included.as_ref(), &available);
    if stored_included
        .as_ref()
        .is_some_and(|stored| stored != &included)
    {
        if let Err(err) =
            preferences.set_filter_included_store_roots(included.iter().cloned().collect())
        {
            log_filter_save_error(overlay, "clean stale store filter values", &err);
        }
    }
    if let Some(controller) = search_controller_for_list(list) {
        controller.set_included_store_roots(list, included.clone());
    }

    for store_root in store_roots {
        let label = shortened_store_label_for_path(&store_root, &store_labels);
        let toggle = build_filter_toggle(&label, included.contains(&store_root));
        let list = list.clone();
        let overlay = overlay.clone();
        let available = available.clone();
        toggle.connect_toggled(move |toggle| {
            let preferences = Preferences::new();
            let stored_included = preferences
                .filter_included_store_roots()
                .map(|roots| roots.into_iter().collect::<BTreeSet<_>>());
            let mut included =
                reconciled_included_filter_values(stored_included.as_ref(), &available);
            if !update_included_filter_value(&mut included, &store_root, toggle.is_active()) {
                toggle.set_active(true);
                return;
            }
            if let Err(err) =
                preferences.set_filter_included_store_roots(included.iter().cloned().collect())
            {
                log_filter_save_error(&overlay, "save store filter selection", &err);
                return;
            }

            if let Some(controller) = search_controller_for_list(&list) {
                controller.set_included_store_roots(&list, included);
            }
        });
        filter_store_box.append(&toggle);
    }
}

fn log_filter_save_error(overlay: &ToastOverlay, action: &str, err: &adw::glib::BoolError) {
    log_error(format!("Failed to {action}: {err}"));
    overlay.add_toast(Toast::new(&gettext("Couldn't save the filter selection.")));
}

fn sync_password_list_filter_button(
    filter_button: &MenuButton,
    filter_popover: &Popover,
    navigation: &NavigationView,
) {
    let visible =
        navigation_stack_is_root(navigation) && !Preferences::new().store_roots().is_empty();
    filter_button.set_visible(visible);
    filter_button.set_sensitive(visible);
    if !visible {
        filter_popover.popdown();
    }
}

pub fn configure_password_list_store_filter(
    filter_button: &MenuButton,
    filter_popover: &Popover,
    filter_store_box: &GtkBox,
    list: &ListBox,
    navigation: &NavigationView,
    overlay: &ToastOverlay,
) {
    render_password_list_store_filter(filter_store_box, list, overlay);
    sync_password_list_filter_button(filter_button, filter_popover, navigation);

    {
        let filter_store_box = filter_store_box.clone();
        let list = list.clone();
        let overlay = overlay.clone();
        filter_popover.connect_notify_local(Some("visible"), move |popover, _| {
            if popover.is_visible() {
                render_password_list_store_filter(&filter_store_box, &list, &overlay);
            }
        });
    }

    {
        let filter_button = filter_button.clone();
        let filter_popover = filter_popover.clone();
        let navigation = navigation.clone();
        navigation
            .clone()
            .connect_notify_local(Some("visible-page"), move |_, _| {
                sync_password_list_filter_button(&filter_button, &filter_popover, &navigation);
            });
    }
}
