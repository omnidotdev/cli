//! Build script to capture git commit hash.

use std::process::Command;

fn main() {
    // Re-run if git HEAD changes
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads/");

    // Get the git commit hash
    let hash = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map_or_else(|| "unknown".to_string(), |s| s.trim().to_string());

    println!("cargo:rustc-env=BUILD_HASH={hash}");

    // Check if the working directory is dirty
    let dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .is_some_and(|o| !o.stdout.is_empty());

    if dirty {
        println!("cargo:rustc-env=BUILD_DIRTY=true");
    } else {
        println!("cargo:rustc-env=BUILD_DIRTY=false");
    }
}
