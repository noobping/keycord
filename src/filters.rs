use adw::gtk::CheckButton;
use std::collections::BTreeSet;

pub fn build_filter_toggle(label: &str, active: bool, sensitive: bool) -> CheckButton {
    CheckButton::builder()
        .label(label)
        .active(active)
        .sensitive(sensitive)
        .hexpand(true)
        .build()
}

pub const fn filter_has_multiple_options(option_count: usize) -> bool {
    option_count > 1
}

pub const fn filter_toggle_is_sensitive(active: bool, included_count: usize) -> bool {
    !active || included_count > 1
}

pub fn reconciled_included_filter_values(
    included: Option<&BTreeSet<String>>,
    available: &BTreeSet<String>,
) -> BTreeSet<String> {
    let included: BTreeSet<String> = included
        .map(|included| included.intersection(available).cloned().collect())
        .unwrap_or_default();
    if included.is_empty() {
        available.clone()
    } else {
        included
    }
}

pub fn update_included_filter_value(
    included: &mut BTreeSet<String>,
    value: &str,
    selected: bool,
) -> bool {
    if selected {
        included.insert(value.to_string());
        return true;
    }

    if included.len() == 1 && included.contains(value) {
        return false;
    }

    included.remove(value);
    true
}

#[cfg(test)]
mod tests {
    use super::{
        filter_has_multiple_options, filter_toggle_is_sensitive, reconciled_included_filter_values,
        update_included_filter_value,
    };
    use std::collections::BTreeSet;

    #[test]
    fn unset_included_values_select_every_available_filter() {
        let available = BTreeSet::from([
            "personal".to_string(),
            "shared".to_string(),
            "work".to_string(),
        ]);
        assert_eq!(
            reconciled_included_filter_values(None, &available),
            available
        );
    }

    #[test]
    fn stale_included_values_are_removed() {
        let available = BTreeSet::from(["personal".to_string(), "work".to_string()]);
        let included = BTreeSet::from(["personal".to_string(), "missing".to_string()]);

        assert_eq!(
            reconciled_included_filter_values(Some(&included), &available),
            BTreeSet::from(["personal".to_string()])
        );
    }

    #[test]
    fn explicit_empty_included_values_select_every_available_filter() {
        let available = BTreeSet::from(["personal".to_string(), "work".to_string()]);
        let included = BTreeSet::new();

        assert_eq!(
            reconciled_included_filter_values(Some(&included), &available),
            available
        );
    }

    #[test]
    fn toggling_filters_updates_included_values() {
        let mut included = BTreeSet::from(["shared".to_string()]);

        assert!(update_included_filter_value(&mut included, "work", true));
        assert!(update_included_filter_value(&mut included, "shared", false));

        assert_eq!(included, BTreeSet::from(["work".to_string()]));
    }

    #[test]
    fn last_included_filter_cannot_be_deselected() {
        let mut included = BTreeSet::from(["work".to_string()]);

        assert!(!update_included_filter_value(&mut included, "work", false));
        assert_eq!(included, BTreeSet::from(["work".to_string()]));
    }

    #[test]
    fn filters_require_multiple_options_to_be_useful() {
        assert!(!filter_has_multiple_options(0));
        assert!(!filter_has_multiple_options(1));
        assert!(filter_has_multiple_options(2));
    }

    #[test]
    fn sole_included_filter_is_disabled() {
        assert!(!filter_toggle_is_sensitive(true, 1));
        assert!(filter_toggle_is_sensitive(true, 2));
        assert!(filter_toggle_is_sensitive(false, 1));
    }
}
