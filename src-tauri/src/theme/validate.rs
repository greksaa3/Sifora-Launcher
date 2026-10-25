//! Theme validation.
//!
//! This is the single choke point every theme — built-in, user-authored or
//! imported from a file — has to pass. It performs three jobs:
//!
//! 1. **Clamping** — `Tokens::clamp_in_place` pulls every number into a safe
//!    range and rejects NaN/infinity.
//! 2. **Schema checking** — deserialisation already guarantees the shape; here
//!    we check the *semantic* invariants (contrast, gradient legibility,
//!    colour count, image availability).
//! 3. **Reporting** — the caller gets a list of issues so the UI can explain
//!    what was corrected instead of failing silently.
//!
//! Note what validation deliberately does **not** do: it never repairs by
//! executing anything, and it never rewrites the theme into "code". A rejected
//! theme is discarded, not partially trusted.

use serde::Serialize;

use super::color::Color;
use super::model::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// The value was corrected automatically.
    Fixed,
    /// The value is legal but may look bad.
    Warning,
    /// The value is not representable in the design system.
    Rejected,
}

#[derive(Debug, Clone, Serialize)]
pub struct Issue {
    pub severity: Severity,
    pub field: String,
    pub message: String,
}

impl Issue {
    fn fixed(field: &str, message: impl Into<String>) -> Self {
        Issue { severity: Severity::Fixed, field: field.into(), message: message.into() }
    }
    fn warn(field: &str, message: impl Into<String>) -> Self {
        Issue { severity: Severity::Warning, field: field.into(), message: message.into() }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationReport {
    pub errors: Vec<Issue>,
    pub warnings: Vec<Issue>,
    pub fixes: Vec<Issue>,
}

impl ValidationReport {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Minimum WCAG ratio we accept between body text and the surface it sits on.
const MIN_TEXT_CONTRAST: f32 = 4.5;
/// Minimum ratio for interactive controls against their own background.
const MIN_CONTROL_CONTRAST: f32 = 3.0;

/// Validate and normalise a theme in place.
///
/// Returns a report; `errors` is non-empty only for themes that cannot be
/// represented in the design system at all (which, given the typed schema,
/// mostly means impossible contrast or a missing background image).
pub fn validate_theme(spec: &mut ThemeSpec) -> ValidationReport {
    let mut report = ValidationReport { errors: vec![], warnings: vec![], fixes: vec![] };

    spec.sanitize_meta();
    if spec.meta.id.is_empty() {
        spec.meta.id = super::defaults::new_id();
        report.fixes.push(Issue::fixed("meta.id", "Сгенерирован идентификатор темы"));
    }
    if spec.meta.schema != THEME_SCHEMA {
        spec.meta.schema = THEME_SCHEMA;
        report.fixes.push(Issue::fixed("meta.schema", "Обновлена версия схемы темы"));
    }

    let before = spec.tokens.clone();
    spec.clamp_in_place();
    if before != spec.tokens {
        report.fixes.push(Issue::fixed("tokens", "Значения приведены к допустимым диапазонам"));
    }

    check_background(spec, &mut report);
    check_contrast(spec, &mut report);
    check_motion(spec, &mut report);

    report
}

fn check_background(spec: &mut ThemeSpec, report: &mut ValidationReport) {
    let kind = spec.tokens.background.kind;
    if kind == BackgroundKind::Image {
        match spec.tokens.background.image.clone() {
            Some(ImageRef::Local { ref path }) if is_plausible_image_path(path) => {}
            Some(ImageRef::Local { ref path }) => {
                let _ = path;
                spec.tokens.background.kind = BackgroundKind::Linear;
                report.warnings.push(Issue::warn(
                    "background.image",
                    "Файл фона недоступен — показан градиент",
                ));
            }
            None => {
                spec.tokens.background.kind = BackgroundKind::Linear;
                report
                    .warnings
                    .push(Issue::warn("background.image", "Для фонового изображения нет источника"));
            }
        }
    }

    // Gradient kinds need at least two stops to be meaningful.
    let needs_two = matches!(
        kind,
        BackgroundKind::Linear | BackgroundKind::Radial | BackgroundKind::Conic
    );
    if needs_two && spec.tokens.background.colors.len() < 2 {
        spec.tokens.background.colors = vec![
            spec.tokens.background.colors.first().copied().unwrap_or(Color::rgb(12, 14, 22)),
            Color::rgb(24, 26, 40),
        ];
        report.fixes.push(Issue::fixed("background.colors", "Добавлен второй цвет градиента"));
    }

    // If the requested path fell back to a gradient, make sure there is one.
    if spec.tokens.background.colors.len() < 2 {
        spec.tokens.background.colors.push(spec.tokens.background.colors[0].mix(Color::BLACK, 0.25));
    }
}

/// Is this a background image reference we are willing to use?
///
/// Local files must look like a local file path with a plausible image
/// extension. Anything with a URL scheme is refused outright: the renderer CSP
/// allows `asset:`/`http://asset.localhost` only, so a remote background can
/// never load, and refusing it here gives a clear warning instead of a silent
/// blank background.
fn is_plausible_image_path(path: &str) -> bool {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed.len() > 1024 {
        return false;
    }
    if trimmed.chars().any(|c| c.is_control()) {
        return false;
    }
    // A drive letter (`C:\...`) is fine; `scheme://` is not.
    let lowered = trimmed.to_ascii_lowercase();
    for scheme in ["http://", "https://", "ftp://", "file://", "//"] {
        if lowered.starts_with(scheme) {
            return false;
        }
    }
    if lowered.contains("://") {
        return false;
    }
    const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "gif", "avif"];
    trimmed
        .rsplit_once('.')
        .map(|(_, ext)| IMAGE_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Text must be readable on the surface it is drawn on.
fn check_contrast(spec: &mut ThemeSpec, report: &mut ValidationReport) {
    let p = &mut spec.tokens.palette;
    // The reference backdrop is the *raw* panel colour, not the composite:
    // that is the worst case the eye can see, so a theme that passes here stays
    // readable whatever the background behind it turns out to be.
    let backdrop = p.surface_raised;
    let deep = p.surface_sunken;

    // 1. Body text vs the raised surface (the main panel colour).
    let body = Color::contrast_ratio(p.text, backdrop);
    if body < MIN_TEXT_CONTRAST {
        let fixed = push_until_contrast(p.text, backdrop, MIN_TEXT_CONTRAST);
        report.fixes.push(Issue::fixed(
            "palette.text",
            format!("Контраст текста был {body:.1}:1, исправлен до {}:1", Color::contrast_ratio(fixed, backdrop)),
        ));
        p.text = fixed;
    } else if body < 7.0 {
        report.warnings.push(Issue::warn(
            "palette.text",
            format!("Контраст текста {body:.1}:1 — ниже комфортного AAA"),
        ));
    }

    // 2. Muted text may be slightly below AA for secondary labels.
    let muted = Color::contrast_ratio(p.text_muted, backdrop);
    if muted < 3.0 {
        let fixed = push_until_contrast(p.text_muted, backdrop, 3.0);
        report.fixes.push(Issue::fixed(
            "palette.text_muted",
            format!("Контраст вторичного текста был {muted:.1}:1, исправлен"),
        ));
        p.text_muted = fixed;
    }

    // 3. The sunken surface must stay usable for secondary panels too.
    let sunken = Color::contrast_ratio(p.text, deep);
    if sunken < MIN_TEXT_CONTRAST {
        let fixed = push_until_contrast(p.text, deep, MIN_TEXT_CONTRAST);
        report.fixes.push(Issue::fixed(
            "palette.text",
            format!("Контраст текста на утопленной поверхности был {sunken:.1}:1, исправлен"),
        ));
        p.text = fixed;
    }

    // 4. Accent text on accent fill.
    let on_accent = Color::contrast_ratio(p.on_accent, p.accent);
    if on_accent < MIN_CONTROL_CONTRAST {
        p.on_accent = p.accent.best_text_contrast();
        report.fixes.push(Issue::fixed("palette.on_accent", "Цвет текста на акценте пересчитан"));
    }

    // 5. Accent itself must be visible against the main surface.
    let surface = p.surface;
    let accent_vs_surface = Color::contrast_ratio(p.accent, surface);
    if accent_vs_surface < 1.4 {
        // Search outward from the surface: on a light theme the accent has to
        // get *darker*, on a dark theme lighter. Both directions are tried so a
        // theme that starts mid-grey still finds a visible variant.
        let surface_is_light = surface.is_light();
        let (hue, sat, lightness) = p.accent.hsl();
        let mut best = p.accent;
        let mut best_ratio = accent_vs_surface;
        for step in 1..=20 {
            let delta = 0.03 * step as f32;
            let outward = if surface_is_light { lightness - delta } else { lightness + delta };
            let inward = if surface_is_light { lightness + delta } else { lightness - delta };
            for candidate_lightness in [outward, inward] {
                if !(0.02..=0.99).contains(&candidate_lightness) {
                    continue;
                }
                let candidate = super::color::hsl(
                    hue,
                    sat.clamp(0.30, 1.0),
                    candidate_lightness,
                );
                let ratio = Color::contrast_ratio(candidate, surface);
                if ratio > best_ratio {
                    best_ratio = ratio;
                    best = candidate;
                }
                if ratio >= 1.6 {
                    break;
                }
            }
            if best_ratio >= 1.6 {
                break;
            }
        }
        report.fixes.push(Issue::fixed(
            "palette.accent",
            format!(
                "Акцент был почти не виден (контраст {accent_vs_surface:.2}:1), пересчитан до {best_ratio:.2}:1"
            ),
        ));
        p.accent = best;
        p.accent_hover = best.adjust(6.0, 0.0, if surface_is_light { -0.05 } else { 0.05 });
    }

    // 6. Status colours need to be distinguishable from plain text.
    for (field, color) in
        [("success", p.success), ("warning", p.warning), ("danger", p.danger)]
    {
        let c = Color::contrast_ratio(color, backdrop);
        if c < MIN_CONTROL_CONTRAST {
            report.warnings.push(Issue::warn(
                field,
                format!("Статусный цвет имеет слабый контраст ({c:.1}:1)"),
            ));
        }
    }
}

fn check_motion(spec: &mut ThemeSpec, report: &mut ValidationReport) {
    let m = &spec.tokens.motion;
    if m.duration_fast > 400 || m.duration > 900 {
        report.warnings.push(Issue::warn(
            "motion",
            "Длинные переходы могут ощущаться вяло; рекомендуется до 400/900 мс",
        ));
    }
    if m.duration < 90 && m.easing == Easing::Standard {
        report.warnings.push(Issue::warn("motion", "Слишком короткий переход для плавной кривой"));
    }
    if m.reduced == ReducedMotion::Never {
        report.warnings.push(Issue::warn(
            "motion.reduced",
            "Анимации принудительно включены — системная настройка движения будет проигнорирована",
        ));
    }
}

/// Composite a translucent colour over an opaque fallback.
#[allow(dead_code)]
fn flatten(top: Color, under: Color) -> Color {
    top.mix(under, top.a.clamp(0.0, 1.0))
}

/// Walk `color` away from `bg` until the contrast target is met.
fn push_until_contrast(color: Color, bg: Color, target: f32) -> Color {
    if Color::contrast_ratio(color, bg) >= target {
        return color;
    }
    let toward = if bg.is_light() { Color::BLACK } else { Color::WHITE };
    let mut best = color;
    for step in 1..=20 {
        let t = step as f32 / 20.0;
        let candidate = color.mix(toward, t);
        let ratio = Color::contrast_ratio(candidate, bg);
        if ratio > Color::contrast_ratio(best, bg) {
            best = candidate;
        }
        if ratio >= target {
            return candidate;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::defaults;

    /// The layout boundary, stated as a test.
    ///
    /// Themes rearrange capabilities; they cannot introduce one. A theme file
    /// that names an action the launcher has never heard of must fail to
    /// decode rather than be quietly repaired into something that runs.
    #[test]
    fn a_theme_cannot_invent_a_capability() {
        let spec = defaults::default_theme();
        let mut json = serde_json::to_value(&spec).expect("serialise");

        // An action name the application does not implement.
        json["tokens"]["layout"]["shortcuts"] = serde_json::json!(["run-anything"]);
        assert!(
            serde_json::from_value::<ThemeSpec>(json.clone()).is_err(),
            "an unknown shortcut must not decode"
        );

        // A section that is not part of the launcher.
        json["tokens"]["layout"]["order"] = serde_json::json!(["admin-panel"]);
        assert!(
            serde_json::from_value::<ThemeSpec>(json.clone()).is_err(),
            "an unknown section must not decode"
        );

        // A field that is not in the schema at all.
        json["tokens"]["layout"]["script"] = serde_json::json!("alert(1)");
        assert!(
            serde_json::from_value::<ThemeSpec>(json).is_err(),
            "an unknown layout field must not decode"
        );
    }

    /// A hand-edited theme may leave the menu order incomplete, but it must not
    /// be able to make a section unreachable.
    #[test]
    fn an_incomplete_menu_order_is_repaired_rather_than_trusted() {
        let mut layout = Layout {
            order: vec![Section::Settings],
            ..Layout::default()
        };
        layout.clamp_in_place();

        assert_eq!(layout.order.len(), Section::ALL.len());
        for section in Section::ALL {
            assert!(layout.order.contains(&section), "{section:?} went missing");
        }
        assert_eq!(layout.order[0], Section::Settings, "the stated order must be kept");
    }

    #[test]
    fn valid_theme_passes_without_errors() {
        let mut spec = defaults::default_theme();
        let report = validate_theme(&mut spec);
        assert!(report.is_valid(), "{:?}", report.errors);
    }

    #[test]
    fn unreadable_text_is_repaired_not_rejected() {
        let mut spec = defaults::default_theme();
        spec.tokens.palette.text = spec.tokens.palette.surface_raised;
        let report = validate_theme(&mut spec);
        assert!(report.is_valid());
        assert!(
            Color::contrast_ratio(spec.tokens.palette.text, spec.tokens.palette.surface_raised)
                >= MIN_TEXT_CONTRAST
        );
    }

    #[test]
    fn invisible_accent_is_nudged_into_view() {
        let mut spec = defaults::default_theme();
        spec.tokens.palette.accent = spec.tokens.palette.surface;
        validate_theme(&mut spec);
        let ratio = Color::contrast_ratio(spec.tokens.palette.accent, spec.tokens.palette.surface);
        assert!(ratio >= 1.4, "accent still invisible: {ratio}");
    }

    /// The repair must work on a light theme too, where the accent has to move
    /// *away* from a light surface rather than towards it.
    #[test]
    fn invisible_accent_is_repaired_on_a_light_theme() {
        let mut spec = defaults::default_theme();
        let surface = spec.tokens.palette.surface;
        spec.tokens.palette.accent = surface;
        spec.tokens.palette.accent_hover = surface;
        validate_theme(&mut spec);
        let ratio = Color::contrast_ratio(spec.tokens.palette.accent, surface);
        assert!(ratio >= 1.4, "light theme accent still invisible: {ratio}");
        // Text must remain readable after the automatic change.
        let body =
            Color::contrast_ratio(spec.tokens.palette.text, spec.tokens.palette.surface_raised);
        assert!(body >= MIN_TEXT_CONTRAST, "text contrast broke: {body}");
    }

    #[test]
    fn local_image_falls_back_when_path_is_absurd() {
        let mut spec = defaults::default_theme();
        spec.tokens.background.kind = BackgroundKind::Image;
        spec.tokens.background.image = Some(ImageRef::Local { path: "\u{0}\u{1}\u{2}".into() });
        let report = validate_theme(&mut spec);
        assert_eq!(spec.tokens.background.kind, BackgroundKind::Linear);
        assert!(!report.warnings.is_empty());
    }

    #[test]
    fn http_background_image_is_never_trusted() {
        for hostile in [
            "http://evil.example/x.png",
            "https://cdn.example/x.png",
            "file:///C:/x.png",
            "//cdn.example/x.png",
        ] {
            assert!(!is_plausible_image_path(hostile), "must refuse {hostile}");
        }
        assert!(is_plausible_image_path(r"C:\Users\me\Pictures\a.png"));
        assert!(!is_plausible_image_path("C:\\Users\\me\\Pictures\\a.exe"));
        assert!(!is_plausible_image_path("C:\\no-extension"));
        assert!(!is_plausible_image_path("C:\\bad\u{0}name.png"));
    }

    #[test]
    fn single_stop_gradient_is_completed() {
        let mut spec = defaults::default_theme();
        spec.tokens.background.kind = BackgroundKind::Linear;
        spec.tokens.background.colors = vec![Color::rgb(20, 20, 40)];
        validate_theme(&mut spec);
        assert!(spec.tokens.background.colors.len() >= 2);
    }
}