//! Setup, update, and desktop integration for Keycord.

pub const fn setup_available() -> bool {
    cfg!(all(target_os = "linux", feature = "setup"))
}

#[cfg(feature = "build-support")]
pub mod build_support;
pub mod desktop;
#[cfg(all(target_os = "linux", feature = "ui"))]
pub mod search_provider;
#[cfg(all(target_os = "linux", feature = "setup"))]
pub mod setup;
#[cfg(feature = "ui")]
pub mod updater;

#[cfg(test)]
mod tests {
    #[test]
    fn setup_availability_matches_the_platform_and_feature() {
        assert_eq!(
            super::setup_available(),
            cfg!(all(target_os = "linux", feature = "setup"))
        );
    }
}
