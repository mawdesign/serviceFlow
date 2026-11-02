//! The fonts slides are drawn with, shared by every renderer in the app (thumbnails, the output
//! monitor and the output window) so each font is looked up and reported once.
//!
//! Resolution order (spec "Fonts"): an installed font from `craft-font-catalog`, a bundled font
//! with the same measurements, Noto by script. The renderer does the last two; this module only
//! supplies the installed fonts, because the renderer may not depend on `craft-font-catalog`.
//! The browser has no installed fonts, so it uses the bundled ones only.

use std::sync::OnceLock;

use serviceflow_core::command::Command;
use serviceflow_render::Fonts;

/// The app's fonts. Cheap to clone; clones share the faces and the substitution record.
pub fn shared() -> Fonts {
    static FONTS: OnceLock<Fonts> = OnceLock::new();
    FONTS.get_or_init(create).clone()
}

/// Commands for the substitutions made since the last call.
pub fn substitution_commands() -> Vec<Command> {
    shared().take_substitutions().into_iter().map(|s| Command::FontSubstituted { requested: s.requested, used: s.used }).collect()
}

#[cfg(not(target_arch = "wasm32"))]
fn create() -> Fonts {
    use std::sync::Arc;

    use craft_font_catalog::Catalog;
    use serviceflow_render::SystemFonts;

    struct Installed(Arc<Catalog>);

    impl SystemFonts for Installed {
        fn load(&self, family: &str) -> Option<(Arc<[u8]>, u32)> {
            let (face, _) = self.0.resolve(family, "Regular")?;
            let data = self.0.load(&face).ok()?;
            Some((data.bytes, data.index))
        }
    }

    let catalog = Catalog::global();
    catalog.scan_in_background();
    Fonts::with_system_fonts(Arc::new(Installed(catalog.clone())))
}

#[cfg(target_arch = "wasm32")]
fn create() -> Fonts {
    Fonts::builtin()
}
