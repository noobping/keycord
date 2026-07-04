#[derive(Default)]
pub struct StoreSupportCache;

impl StoreSupportCache {
    pub fn new() -> Self {
        Self
    }

    pub fn is_supported(&mut self, _store_root: &str) -> bool {
        true
    }
}
