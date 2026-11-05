//! User-facing strings go through craft-i18n: write the English source text at the call site with
//! [`tl!`](crate::tl). Phase 1 is English only, so no catalogs are embedded and lookups return the
//! source text.

use std::sync::LazyLock;

pub static I18N: LazyLock<craft_i18n::I18n> = LazyLock::new(|| {
    static NO_CATALOGS: &craft_i18n::Embedded = &[];
    craft_i18n::I18n::new(NO_CATALOGS, &["en"]).0
});

/// The current UI language. Always English for now.
pub fn lang() -> craft_i18n::Lang {
    craft_i18n::Lang::EN
}

/// Translate an English source string: `tl!("Library")`. A sentence with values is one string with
/// `{name}` placeholders, so a translator can reorder it: `tl!("Could not open “{id}”", id = id)`.
#[macro_export]
macro_rules! tl {
    ($source:expr, $($name:ident = $value:expr),+ $(,)?) => {
        craft_i18n::fmt($crate::tl!($source), &[$((stringify!($name), $value.to_string().as_str())),+])
    };
    ($source:expr) => {
        $crate::i18n::I18N.t($crate::i18n::lang(), $source)
    };
}
