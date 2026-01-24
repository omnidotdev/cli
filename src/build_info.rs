//! Build information captured at compile time.

/// Package version from Cargo.toml.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Short git commit hash (7 chars).
pub const BUILD_HASH: &str = env!("BUILD_HASH");

/// Whether the build was from a dirty working directory (as string).
const BUILD_DIRTY_STR: &str = env!("BUILD_DIRTY");

/// Check if the build was from a dirty working directory.
fn is_dirty() -> bool {
    BUILD_DIRTY_STR == "true"
}

/// Full version string including hash and dirty indicator.
///
/// Format: `0.1.0 (abc1234)` or `0.1.0 (abc1234*)` if dirty.
#[must_use]
pub fn version_string() -> String {
    if is_dirty() {
        format!("{VERSION} ({BUILD_HASH}*)")
    } else {
        format!("{VERSION} ({BUILD_HASH})")
    }
}

/// Short version for display in constrained spaces.
///
/// Format: `build abc1234` or `build abc1234*` if dirty.
#[must_use]
pub fn short_version() -> String {
    if is_dirty() {
        format!("build {BUILD_HASH}*")
    } else {
        format!("build {BUILD_HASH}")
    }
}
