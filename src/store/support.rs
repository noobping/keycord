#[derive(Default)]
pub struct StoreSupportCache;

impl StoreSupportCache {
    pub fn new() -> Self {
        Self
    }

    pub fn is_supported(&mut self, _store_root: &str) -> bool {
        true
    }

    pub fn supports_password_read_tools(&mut self, _store_root: &str) -> bool {
        true
    }

    pub fn supports_advanced_search(
        &mut self,
        _store_root: &str,
        _uses_advanced_features: bool,
    ) -> bool {
        true
    }
}
