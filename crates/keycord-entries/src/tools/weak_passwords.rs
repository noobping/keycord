use super::EntryRequest;
use crate::strength::weak_password_reason;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeakPasswordFinding {
    pub root: String,
    pub label: String,
    pub normalized_label: String,
    pub reason: String,
    pub normalized_reason: String,
}

pub fn weak_password_findings_with<E>(
    requests: Vec<EntryRequest>,
    mut read_password: impl FnMut(&EntryRequest) -> Result<String, E>,
) -> Vec<WeakPasswordFinding> {
    requests
        .into_iter()
        .filter_map(|request| {
            let password = read_password(&request).ok()?;
            let reason = weak_password_reason(&password)?;
            Some(WeakPasswordFinding {
                root: request.root,
                normalized_label: request.label.to_lowercase(),
                label: request.label,
                normalized_reason: reason.to_lowercase(),
                reason,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{weak_password_findings_with, EntryRequest};

    #[test]
    fn scan_uses_injected_entry_reader() {
        let requests = vec![
            EntryRequest {
                root: "/store".into(),
                label: "Weak".into(),
            },
            EntryRequest {
                root: "/store".into(),
                label: "Strong".into(),
            },
        ];
        let findings = weak_password_findings_with(requests, |request| {
            Ok::<_, ()>(
                if request.label == "Weak" {
                    "password"
                } else {
                    "correct horse battery staple"
                }
                .to_string(),
            )
        });
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].normalized_label, "weak");
    }
}
