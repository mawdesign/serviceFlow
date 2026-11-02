//! Workspace tooling: `cargo xtask <command>`.
//!
//! Adapted from DeckCraft's xtask (MIT OR Apache-2.0). Pure Rust (std + serde_json); `cargo` is
//! invoked through `std::process::Command`.

mod layers;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const USAGE: &str = "\
usage: cargo xtask <command>

commands:
  layers [--manifest-path <Cargo.toml>]
                  enforce the crate dependency layering (core, storage and opensong may not use
                  UI crates or concrete storage implementations)
  wasm            cargo check --target wasm32-unknown-unknown for the layered crates and the app
  ci              fmt --check, clippy -D warnings, test, layers, wasm (stops at first failure)
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("layers") => match &args[1..] {
            [] => cmd_layers(&root().join("Cargo.toml")),
            [flag, path] if flag == "--manifest-path" => cmd_layers(Path::new(path)),
            _ => Err(format!("bad arguments to `layers`\n\n{USAGE}")),
        },
        Some("wasm") => cmd_wasm(),
        Some("ci") => cmd_ci(),
        Some("-h" | "--help" | "help") | None => {
            print!("{USAGE}");
            Ok(())
        }
        Some(other) => Err(format!("unknown command `{other}`\n\n{USAGE}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Workspace root (parent of the xtask crate).
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("xtask has a parent dir").to_path_buf()
}

fn cargo() -> Command {
    let mut c = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    c.current_dir(root());
    c
}

fn run(mut cmd: Command, what: &str) -> Result<(), String> {
    eprintln!("$ {what}");
    let status = cmd.status().map_err(|e| format!("{what}: failed to spawn: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("{what}: exited with {status}")) }
}

fn metadata(manifest: &Path) -> Result<serde_json::Value, String> {
    let out = cargo()
        .args(["metadata", "--format-version", "1", "--no-deps", "--manifest-path"])
        .arg(manifest)
        .output()
        .map_err(|e| format!("cargo metadata: {e}"))?;
    if !out.status.success() {
        return Err(format!("cargo metadata failed:\n{}", String::from_utf8_lossy(&out.stderr)));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("cargo metadata: bad JSON: {e}"))
}

fn cmd_layers(manifest: &Path) -> Result<(), String> {
    let crates = layers::from_metadata(&metadata(manifest)?)?;
    println!("Dependency layering\n");
    println!("{:<28} {:<14} workspace deps", "crate", "layer");
    for c in &crates {
        let ws: Vec<String> = c
            .deps
            .iter()
            .filter(|d| d.workspace)
            .map(|d| {
                let k = match d.kind {
                    layers::DepKind::Normal => "",
                    layers::DepKind::Dev => " (dev)",
                    layers::DepKind::Build => " (build)",
                };
                format!("{}{k}", layers::short_name(&d.name))
            })
            .collect();
        println!("{:<28} {:<14} {}", c.name, layers::describe(layers::classify(&c.name)), ws.join(", "));
    }
    let violations = layers::check(&crates);
    println!();
    if violations.is_empty() {
        println!("OK: {} crates, no layering violations.", crates.len());
        Ok(())
    } else {
        println!("{} violation(s):", violations.len());
        for v in &violations {
            println!("  - {v}");
        }
        Err(format!("{} layering violation(s)", violations.len()))
    }
}

/// Packages that must build for wasm32: every layered crate except the desktop filesystem
/// storage, plus the app.
fn wasm_set() -> Result<Vec<String>, String> {
    let crates = layers::from_metadata(&metadata(&root().join("Cargo.toml"))?)?;
    Ok(crates
        .into_iter()
        .filter(|c| match layers::classify(&c.name) {
            Some(layers::Class::Layer(_)) => layers::short_name(&c.name) != "storage-fs",
            Some(layers::Class::Exempt) => c.name == "serviceflow",
            _ => false,
        })
        .map(|c| c.name)
        .collect())
}

fn cmd_wasm() -> Result<(), String> {
    let set = wasm_set()?;
    let mut c = cargo();
    c.args(["check", "--target", "wasm32-unknown-unknown", "--keep-going"]);
    for pkg in &set {
        c.args(["-p", pkg]);
    }
    run(c, &format!("cargo check --target wasm32-unknown-unknown --keep-going -p {}", set.join(" -p ")))
}

fn cargo_step(args: &'static [&'static str]) -> impl Fn() -> Result<(), String> {
    move || {
        let mut c = cargo();
        c.args(args);
        run(c, &format!("cargo {}", args.join(" ")))
    }
}

fn cmd_ci() -> Result<(), String> {
    type Step = (&'static str, Box<dyn Fn() -> Result<(), String>>);
    let steps: Vec<Step> = vec![
        ("fmt", Box::new(cargo_step(&["fmt", "--all", "--", "--check"]))),
        ("clippy", Box::new(cargo_step(&["clippy", "--workspace", "--all-targets", "--keep-going", "--", "-D", "warnings"]))),
        ("test", Box::new(cargo_step(&["test", "--workspace"]))),
        ("layers", Box::new(|| cmd_layers(&root().join("Cargo.toml")))),
        ("wasm", Box::new(cmd_wasm)),
    ];
    for (i, (name, f)) in steps.iter().enumerate() {
        eprintln!("\n=== ci: {name} ===");
        if let Err(e) = f() {
            let skipped: Vec<&str> = steps[i + 1..].iter().map(|(n, _)| *n).collect();
            return Err(format!("ci failed at `{name}`: {e} (skipped: {})", skipped.join(", ")));
        }
    }
    println!("\nci: all {} steps passed", steps.len());
    Ok(())
}
