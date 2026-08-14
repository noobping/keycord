//! Strict deterministic composition for line-oriented declarative UI fragments.

use std::collections::BTreeSet;

pub fn compose_marked_fragments(
    skeleton: &str,
    marker_namespace: &str,
    fragments: &[(&str, String)],
) -> Result<String, String> {
    let marker_prefix = format!("<!-- {marker_namespace}:");
    let mut composed = skeleton.to_string();
    let mut names = BTreeSet::new();

    for (name, fragment) in fragments {
        if !names.insert(*name) {
            return Err(format!("duplicate UI fragment definition `{name}`"));
        }

        let marker = format!("{marker_prefix}{name} -->");
        let marker_count = composed.matches(&marker).count();
        match marker_count {
            0 => return Err(format!("missing UI fragment marker `{name}`")),
            1 => {}
            _ => return Err(format!("duplicate UI fragment marker `{name}`")),
        }

        let marker_start = composed
            .find(&marker)
            .expect("marker count established that the marker exists");
        let line_start = composed[..marker_start]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        let line_end = composed[marker_start..]
            .find('\n')
            .map_or(composed.len(), |offset| marker_start + offset + 1);
        let marker_line = composed[line_start..line_end].trim();
        if marker_line != marker {
            return Err(format!(
                "UI fragment marker `{name}` must be the only content on its line"
            ));
        }

        composed.replace_range(line_start..line_end, fragment);
    }

    if let Some(marker_start) = composed.find(&marker_prefix) {
        let marker_end = composed[marker_start..]
            .find(" -->")
            .map_or(composed.len(), |offset| marker_start + offset + 4);
        return Err(format!(
            "unresolved UI fragment marker `{}`",
            &composed[marker_start..marker_end]
        ));
    }

    Ok(composed)
}

#[cfg(test)]
mod tests {
    use super::compose_marked_fragments;

    const NAMESPACE: &str = "test-fragment";

    #[test]
    fn fragments_are_inserted_in_marker_order() {
        let skeleton = "before\n  <!-- test-fragment:first -->\nmiddle\n<!-- test-fragment:second -->\nafter\n";
        let fragments = [
            ("first", "first fragment\n".to_string()),
            ("second", "second fragment\n".to_string()),
        ];

        assert_eq!(
            compose_marked_fragments(skeleton, NAMESPACE, &fragments).unwrap(),
            "before\nfirst fragment\nmiddle\nsecond fragment\nafter\n"
        );
    }

    #[test]
    fn missing_markers_are_rejected() {
        let err =
            compose_marked_fragments("<interface />\n", NAMESPACE, &[("missing", String::new())])
                .expect_err("missing markers must fail composition");
        assert!(err.contains("missing UI fragment marker `missing`"));
    }

    #[test]
    fn duplicate_markers_are_rejected() {
        let skeleton = "<!-- test-fragment:duplicate -->\n<!-- test-fragment:duplicate -->\n";
        let err = compose_marked_fragments(skeleton, NAMESPACE, &[("duplicate", String::new())])
            .expect_err("duplicate markers must fail composition");
        assert!(err.contains("duplicate UI fragment marker `duplicate`"));
    }

    #[test]
    fn duplicate_fragment_definitions_are_rejected() {
        let skeleton = "<!-- test-fragment:duplicate -->\n";
        let fragments = [
            ("duplicate", "first\n".to_string()),
            ("duplicate", "second\n".to_string()),
        ];
        let err = compose_marked_fragments(skeleton, NAMESPACE, &fragments)
            .expect_err("duplicate definitions must fail composition");
        assert!(err.contains("duplicate UI fragment definition `duplicate`"));
    }

    #[test]
    fn unresolved_markers_are_rejected() {
        let skeleton = "<!-- test-fragment:known -->\n<!-- test-fragment:unknown -->\n";
        let err = compose_marked_fragments(skeleton, NAMESPACE, &[("known", String::new())])
            .expect_err("unresolved markers must fail composition");
        assert!(err.contains("unresolved UI fragment marker"));
        assert!(err.contains("unknown"));
    }
}
