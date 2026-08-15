use super::search::search_controller_for_list;
use super::EntryListUiPorts;
use adw::gtk::{Box as GtkBox, ListBox, MenuButton, Popover};
use adw::prelude::*;
use adw::{NavigationView, Toast, ToastOverlay};
use keycord_runtime::i18n::gettext;
use keycord_runtime::log_error;
use keycord_shell::filters::{
    build_filter_toggle, filter_has_multiple_options, filter_toggle_is_sensitive,
    reconciled_included_filter_values, update_included_filter_value,
};
use keycord_shell::ui::{clear_box_children, navigation_stack_is_root};
use keycord_stores::labels::{shortened_store_label_for_path, shortened_store_label_map};
use std::collections::BTreeSet;

fn included_password_list_store_roots(
    store_roots: &[String],
    stored_included: Option<&BTreeSet<String>>,
) -> BTreeSet<String> {
    let available = store_roots.iter().cloned().collect::<BTreeSet<_>>();
    reconciled_included_filter_values(stored_included, &available)
}

pub(super) fn reconcile_password_list_store_filter(
    list: &ListBox,
    overlay: &ToastOverlay,
    ports: &EntryListUiPorts,
    store_roots: &[String],
) -> BTreeSet<String> {
    let stored_included = (ports.preferences.included_store_roots)()
        .map(|roots| roots.into_iter().collect::<BTreeSet<_>>());
    let included = included_password_list_store_roots(store_roots, stored_included.as_ref());
    if stored_included
        .as_ref()
        .is_some_and(|stored| stored != &included)
    {
        if let Err(err) =
            (ports.preferences.set_included_store_roots)(included.iter().cloned().collect())
        {
            log_filter_save_error(overlay, "clean stale store filter values", &err);
        }
    }
    if let Some(controller) = search_controller_for_list(list) {
        controller.set_included_store_roots(list, included.clone());
    }

    included
}

fn render_password_list_store_filter(
    filter_store_box: &GtkBox,
    list: &ListBox,
    overlay: &ToastOverlay,
    ports: &EntryListUiPorts,
) {
    clear_box_children(filter_store_box);

    let store_roots = (ports.preferences.store_roots)();
    let available = store_roots.iter().cloned().collect::<BTreeSet<_>>();
    let store_labels = shortened_store_label_map(&store_roots);
    let included = reconcile_password_list_store_filter(list, overlay, ports, &store_roots);

    for store_root in store_roots {
        let label = shortened_store_label_for_path(&store_root, &store_labels);
        let active = included.contains(&store_root);
        let toggle = build_filter_toggle(
            &label,
            active,
            filter_toggle_is_sensitive(active, included.len()),
        );
        let filter_store_box_for_toggle = filter_store_box.clone();
        let list = list.clone();
        let overlay = overlay.clone();
        let available = available.clone();
        let ports = ports.clone();
        toggle.connect_toggled(move |toggle| {
            let stored_included = (ports.preferences.included_store_roots)()
                .map(|roots| roots.into_iter().collect::<BTreeSet<_>>());
            let mut included =
                reconciled_included_filter_values(stored_included.as_ref(), &available);
            if !update_included_filter_value(&mut included, &store_root, toggle.is_active()) {
                toggle.set_active(true);
                return;
            }
            if let Err(err) =
                (ports.preferences.set_included_store_roots)(included.iter().cloned().collect())
            {
                log_filter_save_error(&overlay, "save store filter selection", &err);
                render_password_list_store_filter(
                    &filter_store_box_for_toggle,
                    &list,
                    &overlay,
                    &ports,
                );
                return;
            }

            render_password_list_store_filter(
                &filter_store_box_for_toggle,
                &list,
                &overlay,
                &ports,
            );
        });
        filter_store_box.append(&toggle);
    }
}

fn log_filter_save_error(overlay: &ToastOverlay, action: &str, err: &str) {
    log_error(format!("Failed to {action}: {err}"));
    overlay.add_toast(Toast::new(&gettext("Couldn't save the filter selection.")));
}

fn sync_password_list_filter_button(
    filter_button: &MenuButton,
    filter_popover: &Popover,
    navigation: &NavigationView,
    ports: &EntryListUiPorts,
) {
    let visible = navigation_stack_is_root(navigation)
        && filter_has_multiple_options((ports.preferences.store_roots)().len());
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
    ports: &EntryListUiPorts,
) {
    render_password_list_store_filter(filter_store_box, list, overlay, ports);
    sync_password_list_filter_button(filter_button, filter_popover, navigation, ports);

    {
        let filter_store_box = filter_store_box.clone();
        let list = list.clone();
        let overlay = overlay.clone();
        let ports = ports.clone();
        filter_popover.connect_notify_local(Some("visible"), move |popover, _| {
            if popover.is_visible() {
                render_password_list_store_filter(&filter_store_box, &list, &overlay, &ports);
            }
        });
    }

    {
        let filter_button = filter_button.clone();
        let filter_popover = filter_popover.clone();
        let navigation = navigation.clone();
        let ports = ports.clone();
        navigation
            .clone()
            .connect_notify_local(Some("visible-page"), move |_, _| {
                sync_password_list_filter_button(
                    &filter_button,
                    &filter_popover,
                    &navigation,
                    &ports,
                );
            });
    }
}

#[cfg(test)]
mod tests {
    use super::included_password_list_store_roots;
    use std::collections::BTreeSet;

    #[test]
    fn first_store_is_included_after_empty_initial_configuration() {
        let first_store = "/tmp/personal".to_string();
        let store_roots = vec![first_store.clone()];

        assert_eq!(
            included_password_list_store_roots(&store_roots, None),
            BTreeSet::from([first_store.clone()])
        );
        assert_eq!(
            included_password_list_store_roots(&store_roots, Some(&BTreeSet::new())),
            BTreeSet::from([first_store])
        );
    }
}
