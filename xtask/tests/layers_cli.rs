//! `cargo xtask layers` end to end: it passes on this workspace and fails the build on a
//! deliberately broken one.

use std::path::Path;
use std::process::Command;

fn layers(manifest: &Path) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_xtask")).args(["layers", "--manifest-path"]).arg(manifest).output().expect("run xtask");
    (out.status.success(), format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

#[test]
fn a_core_that_depends_on_ui_and_concrete_storage_fails_the_check() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/broken-workspace/Cargo.toml");
    let (ok, output) = layers(&manifest);
    assert!(!ok, "the broken workspace passed:\n{output}");
    assert!(output.contains("serviceflow-core (L0) depends on UI crate `egui`"), "{output}");
    assert!(output.contains("serviceflow-core (L0) depends on UI crate `craft-ui`"), "{output}");
    assert!(output.contains("serviceflow-core (L0) -> serviceflow-storage-fs (L2)"), "{output}");
}

#[test]
fn this_workspace_passes_the_check() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../Cargo.toml");
    let (ok, output) = layers(&manifest);
    assert!(ok, "{output}");
}
