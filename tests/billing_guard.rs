//! Guard: the CLI never meters or bills AI usage itself
//!
//! A client must never hold an Aether service key, and only the Synapse
//! gateway turns usage into credits. Any cost the CLI shows is a display-only
//! estimate

use std::fs;
use std::path::{Path, PathBuf};

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_client_side_usage_recording() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_sources(&root.join("src"), &mut files);
    rust_sources(&root.join("crates"), &mut files);

    let forbidden = [
        "AETHER_URL",
        "AETHER_SERVICE_API_KEY",
        "AETHER_APP_ID",
        "synapse_billing",
        "UsageRecorder",
        "AetherClient",
    ];
    for path in files {
        let text = fs::read_to_string(&path).unwrap();
        for needle in forbidden {
            assert!(
                !text.contains(needle),
                "{} contains `{needle}`: the Synapse gateway meters and bills usage, never the client",
                path.display()
            );
        }
    }
}

#[test]
fn no_billing_crate_dependency() {
    let manifest =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    assert!(
        !manifest.contains("synapse-billing"),
        "Cargo.toml depends on synapse-billing: billing belongs to the gateway"
    );
}
