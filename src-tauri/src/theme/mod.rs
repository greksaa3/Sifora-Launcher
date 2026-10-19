//! Theme subsystem: closed schema, validation, offline composer and the
//! optional remote designer.
//!
//! The module boundary *is* the security boundary. Nothing outside it can
//! create a `ThemeSpec` that has not passed [`validate::validate_theme`], and a
//! `ThemeSpec` can only ever carry colours, numbers and enums.
//!
//! That holds for [`model::Layout`] as well: it reorders, hides and promotes
//! capabilities that already exist. It has no field for anything new — no
//! action name a theme could invent, no markup, no script.

pub mod color;
pub mod defaults;
pub mod generate;
pub mod layout;
pub mod lexicon;
pub mod model;
pub mod provider;
pub mod store;
pub mod validate;

pub use color::Color;
pub use defaults::{builtin_themes, default_theme, ThemeSummary};
pub use model::*;
pub use provider::{AiConfig, AiConfigView, GenerationResult, ProviderKind};
pub use store::ThemeStore;
pub use validate::{Severity, ValidationReport};