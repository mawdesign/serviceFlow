//! Crate dependency layering (ADR-0001, spec #1 "Architecture").
//!
//! Adapted from DeckCraft's `xtask/src/layers.rs` (MIT OR Apache-2.0). The rule engine works on a
//! small, metadata-independent model so it can be unit-tested; `from_metadata` builds that model
//! from `cargo metadata --no-deps`.
//!
//! The rules:
//! - Every workspace crate is registered in [`TABLE`].
//! - A workspace dependency (normal, dev or build) must point to a strictly lower layer. Concrete
//!   storage implementations sit above core, storage and opensong, so those can never use them.
//! - Below [`UI_MIN_LAYER`], no crate may use a UI toolkit or windowing crate ([`UI_CRATES`]).
//! - The testkit may only be a dev-dependency.
//!
//! Caveat: `--no-deps` lists direct dependencies only, so a UI crate pulled in transitively
//! through a third-party crate is not caught.

use serde_json::Value;

/// Where a workspace crate sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Layer(u8),
    /// Test support: may use anything below [`UI_MIN_LAYER`]; others may use it only as a
    /// dev-dependency.
    Testkit,
    /// The app and build tooling: exempt from the rules.
    Exempt,
}

impl Class {
    fn layer(self) -> Option<u8> {
        match self {
            Class::Layer(l) => Some(l),
            Class::Testkit => Some(UI_MIN_LAYER),
            Class::Exempt => None,
        }
    }
}

/// The layering table, keyed by package name without the `serviceflow-` prefix.
pub const TABLE: &[(&str, Class)] = &[
    // L0: domain model, commands and engine. No IO, no UI.
    ("core", Class::Layer(0)),
    // L1: storage traits and the OpenSong codec, over the core.
    ("storage", Class::Layer(1)),
    ("opensong", Class::Layer(1)),
    // L2: concrete storage implementations and the renderer.
    ("storage-memory", Class::Layer(2)),
    ("storage-fs", Class::Layer(2)),
    ("storage-web", Class::Layer(2)),
    ("render", Class::Layer(2)),
    ("testkit", Class::Testkit),
    // The egui shell and tooling.
    ("serviceflow", Class::Exempt),
    ("xtask", Class::Exempt),
];

/// External crates that make up a UI toolkit or windowing dependency. Entries ending in `*` are
/// prefixes. `craft-text`, `craft-i18n`, `craft-atomic` and similar have no UI and are allowed.
pub const UI_CRATES: &[&str] =
    &["egui*", "eframe", "winit", "wgpu*", "glow", "rfd", "craft-ui", "craft-window", "craft-dialogs", "craft-keymap", "craft-font-catalog"];

/// First layer allowed to use UI crates. Only the (exempt) app is above every layer.
pub const UI_MIN_LAYER: u8 = 3;

pub fn short_name(pkg: &str) -> &str {
    pkg.strip_prefix("serviceflow-").unwrap_or(pkg)
}

pub fn classify(pkg: &str) -> Option<Class> {
    let s = short_name(pkg);
    TABLE.iter().find(|(n, _)| *n == s).map(|(_, c)| *c)
}

fn is_ui_crate(name: &str) -> bool {
    UI_CRATES.iter().any(|p| match p.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => name == *p,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepKind {
    Normal,
    Dev,
    Build,
}

#[derive(Debug, Clone)]
pub struct Dep {
    pub name: String,
    pub kind: DepKind,
    /// `true` if the dependency is a workspace member.
    pub workspace: bool,
}

#[derive(Debug, Clone)]
pub struct Crate {
    pub name: String,
    pub deps: Vec<Dep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    Unregistered { krate: String },
    Upward { krate: String, dep: String, from: u8, to: u8, kind: DepKind },
    TestkitAsNormalDep { krate: String },
    UiCrate { krate: String, dep: String, layer: u8 },
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Violation::Unregistered { krate } => write!(f, "{krate}: unknown workspace crate; register it in xtask/src/layers.rs TABLE"),
            Violation::Upward { krate, dep, from, to, kind } => {
                write!(f, "{krate} (L{from}) -> {dep} (L{to}) [{kind:?}]: may only depend on strictly lower layers")
            }
            Violation::TestkitAsNormalDep { krate } => write!(f, "{krate}: serviceflow-testkit may only be a dev-dependency"),
            Violation::UiCrate { krate, dep, layer } => {
                write!(f, "{krate} (L{layer}) depends on UI crate `{dep}`; UI and windowing crates are only allowed in the app")
            }
        }
    }
}

/// Check all rules. Returns violations sorted for stable output.
pub fn check(crates: &[Crate]) -> Vec<Violation> {
    let mut out = Vec::new();
    for c in crates {
        let Some(class) = classify(&c.name) else {
            out.push(Violation::Unregistered { krate: c.name.clone() });
            continue;
        };
        let Some(layer) = class.layer() else { continue };
        for d in &c.deps {
            // Self dev-dependencies (e.g. to enable features in tests) are fine.
            if d.name == c.name {
                continue;
            }
            if !d.workspace {
                if layer < UI_MIN_LAYER && is_ui_crate(&d.name) {
                    out.push(Violation::UiCrate { krate: c.name.clone(), dep: d.name.clone(), layer });
                }
                continue;
            }
            match classify(&d.name) {
                // Unregistered deps are reported on their own entry.
                None => {}
                Some(Class::Testkit) if d.kind != DepKind::Dev => out.push(Violation::TestkitAsNormalDep { krate: c.name.clone() }),
                Some(Class::Testkit) => {}
                Some(dc) => {
                    let to = dc.layer().unwrap_or(u8::MAX);
                    if to >= layer {
                        out.push(Violation::Upward { krate: c.name.clone(), dep: d.name.clone(), from: layer, to, kind: d.kind });
                    }
                }
            }
        }
    }
    out.sort_by_key(ToString::to_string);
    out.dedup();
    out
}

/// Build the model from `cargo metadata --format-version 1 --no-deps`.
pub fn from_metadata(meta: &Value) -> Result<Vec<Crate>, String> {
    let pkgs = meta["packages"].as_array().ok_or("metadata: no packages array")?;
    let members: Vec<&str> = pkgs.iter().filter_map(|p| p["name"].as_str()).collect();
    let mut out = Vec::new();
    for p in pkgs {
        let name = p["name"].as_str().ok_or("package without name")?.to_owned();
        let mut deps = Vec::new();
        for d in p["dependencies"].as_array().into_iter().flatten() {
            let dname = d["name"].as_str().unwrap_or_default().to_owned();
            let kind = match d["kind"].as_str() {
                Some("dev") => DepKind::Dev,
                Some("build") => DepKind::Build,
                _ => DepKind::Normal,
            };
            let workspace = members.contains(&dname.as_str()) || d["path"].is_string();
            deps.push(Dep { name: dname, kind, workspace });
        }
        out.push(Crate { name, deps });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

pub fn describe(class: Option<Class>) -> String {
    match class {
        Some(Class::Layer(l)) => format!("L{l}"),
        Some(Class::Testkit) => "testkit".into(),
        Some(Class::Exempt) => "exempt".into(),
        None => "UNREGISTERED".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use DepKind::{Dev, Normal};

    fn c(name: &str, deps: &[(&str, DepKind, bool)]) -> Crate {
        Crate { name: name.into(), deps: deps.iter().map(|(n, k, w)| Dep { name: (*n).into(), kind: *k, workspace: *w }).collect() }
    }

    #[test]
    fn the_planned_graph_passes() {
        let g = [
            c("serviceflow-core", &[("serde", Normal, false)]),
            c("serviceflow-storage", &[("serviceflow-core", Normal, true)]),
            c("serviceflow-opensong", &[("serviceflow-core", Normal, true), ("serviceflow-testkit", Dev, true)]),
            c("serviceflow-storage-memory", &[("serviceflow-storage", Normal, true), ("serviceflow-opensong", Normal, true)]),
            c("serviceflow-storage-fs", &[("serviceflow-storage", Normal, true), ("craft-atomic", Normal, false)]),
            c("serviceflow-render", &[("serviceflow-core", Normal, true), ("craft-text", Normal, false)]),
            c("serviceflow-testkit", &[("serviceflow-storage-memory", Normal, true)]),
            c("serviceflow", &[("serviceflow-storage-fs", Normal, true), ("egui", Normal, false), ("craft-ui", Normal, false)]),
            c("xtask", &[("serde_json", Normal, false)]),
        ];
        assert_eq!(check(&g), vec![]);
    }

    #[test]
    fn core_storage_and_opensong_may_not_use_ui_crates() {
        let ui =
            ["egui", "eframe", "winit", "egui_kittest", "egui-winit", "wgpu", "rfd", "craft-ui", "craft-window", "craft-dialogs", "craft-keymap"];
        for krate in ["serviceflow-core", "serviceflow-storage", "serviceflow-opensong"] {
            for dep in ui {
                let v = check(&[c(krate, &[(dep, Normal, false)])]);
                assert!(matches!(&v[..], [Violation::UiCrate { .. }]), "{krate} -> {dep}: {v:?}");
            }
        }
    }

    #[test]
    fn core_storage_and_opensong_may_not_use_a_concrete_storage_implementation() {
        for krate in ["serviceflow-core", "serviceflow-storage", "serviceflow-opensong"] {
            for imp in ["serviceflow-storage-memory", "serviceflow-storage-fs", "serviceflow-storage-web"] {
                let v = check(&[c(krate, &[(imp, Normal, true)])]);
                assert!(matches!(&v[..], [Violation::Upward { .. }]), "{krate} -> {imp}: {v:?}");
            }
        }
    }

    #[test]
    fn a_concrete_storage_implementation_is_banned_even_as_a_dev_dependency() {
        let v = check(&[c("serviceflow-core", &[("serviceflow-storage-memory", Dev, true)])]);
        assert!(matches!(&v[..], [Violation::Upward { kind: Dev, .. }]), "{v:?}");
    }

    #[test]
    fn the_core_does_not_depend_on_the_codec_or_the_traits() {
        assert!(!check(&[c("serviceflow-core", &[("serviceflow-opensong", Normal, true)])]).is_empty());
        assert!(!check(&[c("serviceflow-core", &[("serviceflow-storage", Normal, true)])]).is_empty());
    }

    #[test]
    fn non_ui_craft_crates_are_allowed_low_down() {
        for dep in ["craft-text", "craft-i18n", "craft-atomic"] {
            assert_eq!(check(&[c("serviceflow-core", &[(dep, Normal, false)])]), vec![], "{dep}");
        }
    }

    #[test]
    fn the_app_and_xtask_are_exempt() {
        for app in ["serviceflow", "xtask"] {
            assert_eq!(check(&[c(app, &[("egui", Normal, false), ("serviceflow-storage-fs", Normal, true)])]), vec![]);
        }
    }

    #[test]
    fn an_unregistered_crate_is_an_error() {
        let v = check(&[c("serviceflow-mystery", &[])]);
        assert!(matches!(&v[..], [Violation::Unregistered { krate }] if krate == "serviceflow-mystery"));
        assert!(v[0].to_string().contains("register"));
    }

    #[test]
    fn the_testkit_is_only_a_dev_dependency() {
        let v = check(&[c("serviceflow-render", &[("serviceflow-testkit", Normal, true)])]);
        assert!(matches!(&v[..], [Violation::TestkitAsNormalDep { .. }]), "{v:?}");
    }

    #[test]
    fn metadata_is_parsed_into_the_model() {
        let meta: Value = serde_json::from_str(
            r#"{"packages":[
                {"name":"serviceflow-opensong","dependencies":[
                    {"name":"serviceflow-core","kind":null,"path":"/x/crates/core"},
                    {"name":"quick-xml","kind":null},
                    {"name":"serviceflow-testkit","kind":"dev","path":"/x/crates/testkit"}]},
                {"name":"serviceflow-core","dependencies":[]}
            ]}"#,
        )
        .unwrap();
        let g = from_metadata(&meta).unwrap();
        assert_eq!(g.len(), 2);
        let codec = g.iter().find(|c| c.name == "serviceflow-opensong").unwrap();
        assert!(codec.deps[0].workspace && !codec.deps[1].workspace);
        assert_eq!(codec.deps[2].kind, Dev);
        assert_eq!(check(&g), vec![]);
    }
}
