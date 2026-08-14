use serde::{Deserialize, Serialize};

/// Persisted password-generator policy.
///
/// Password generation itself belongs to the entries subject. Preferences owns
/// this value because it defines the on-disk and GSettings representation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PasswordGenerationSettings {
    pub length: u32,
    pub min_lowercase: u32,
    pub min_uppercase: u32,
    pub min_numbers: u32,
    pub min_symbols: u32,
}

impl Default for PasswordGenerationSettings {
    fn default() -> Self {
        Self {
            length: 24,
            min_lowercase: 1,
            min_uppercase: 1,
            min_numbers: 1,
            min_symbols: 1,
        }
    }
}

impl PasswordGenerationSettings {
    #[must_use]
    pub fn normalized(&self) -> Self {
        let mut normalized = self.clone();
        normalized.length = normalized.length.max(1);

        if normalized.minimum_length() == 0 {
            normalized.min_lowercase = 1;
        }

        normalized.length = normalized.length.max(normalized.minimum_length());
        normalized
    }

    #[must_use]
    pub const fn minimum_length(&self) -> u32 {
        self.min_lowercase + self.min_uppercase + self.min_numbers + self.min_symbols
    }
}

#[cfg(test)]
mod tests {
    use super::PasswordGenerationSettings;

    #[test]
    fn defaults_are_usable() {
        let settings = PasswordGenerationSettings::default();
        assert!(settings.length >= settings.minimum_length());
        assert!(settings.minimum_length() > 0);
    }

    #[test]
    fn normalization_enables_a_character_class() {
        let settings = PasswordGenerationSettings {
            length: 0,
            min_lowercase: 0,
            min_uppercase: 0,
            min_numbers: 0,
            min_symbols: 0,
        }
        .normalized();

        assert_eq!(settings.length, 1);
        assert_eq!(settings.min_lowercase, 1);
    }
}
