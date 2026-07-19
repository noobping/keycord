use adw::gtk::CheckButton;
use std::collections::BTreeSet;

pub fn build_filter_toggle(label: &str, active: bool) -> CheckButton {
    CheckButton::builder()
        .label(label)
        .active(active)
        .hexpand(true)
        .build()
}

pub fn reconciled_included_filter_values(
    included: Option<&BTreeSet<String>>,
    available: &BTreeSet<String>,
) -> BTreeSet<String> {
    included.map_or_else(
        || available.clone(),
        |included| included.intersection(available).cloned().collect(),
    )
}

pub fn update_included_filter_value(included: &mut BTreeSet<String>, value: &str, selected: bool) {
    if selected {
        included.insert(value.to_string());
    } else {
        included.remove(value);
    }
}

#[cfg(test)]
mod tests {
    use super::{reconciled_included_filter_values, update_included_filter_value};
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
    fn explicit_empty_included_values_keep_every_filter_deselected() {
        let available = BTreeSet::from(["personal".to_string(), "work".to_string()]);
        let included = BTreeSet::new();

        assert_eq!(
            reconciled_included_filter_values(Some(&included), &available),
            BTreeSet::new()
        );
    }

    #[test]
    fn toggling_filters_updates_included_values() {
        let mut included = BTreeSet::from(["shared".to_string()]);

        update_included_filter_value(&mut included, "work", true);
        update_included_filter_value(&mut included, "shared", false);

        assert_eq!(included, BTreeSet::from(["work".to_string()]));
    }
}
