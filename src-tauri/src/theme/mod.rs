//! Theme subsystem: closed schema, validation, storage and the built-in
//! presets.
//!
//! The module boundary *is* the security boundary. Nothing outside it can
//! create a `ThemeSpec` that has not passed [`validate::validate_theme`], and a
//! `ThemeSpec` can only ever carry colours, numbers and enums.
//!
//! That holds for [`model::Layout`] as well: it reorders, hides and promotes
//! capabilities that already exist. It has no field for anything new — no
//! action name a theme could invent, no markup, no script.
//!
//! Themes are authored by hand, in the application, and by editing an exported
//! file. There is deliberately no generator: the interesting part of a theme is
//! a design decision, and a model asked to guess one produced palettes that all
//! looked alike.

pub mod color;
pub mod defaults;
pub mod model;
pub mod store;
pub mod validate;

pub use color::Color;
pub use defaults::{builtin_themes, default_theme, ThemeSummary};
pub use model::*;
pub use store::ThemeStore;
pub use validate::{Severity, ValidationReport};