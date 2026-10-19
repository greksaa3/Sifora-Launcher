//! Built-in starting points.
//!
//! These ship with the app and cover the "pick your own background" need out of
//! the box. They are ordinary [`ThemeSpec`]s, so nothing about them is special
//! compared to a generated theme — they go through exactly the same validation
//! and rendering path.

use super::color::{hsla, hsl, Color};
use super::model::*;
use crate::util::now_rfc3339;

/// The theme the launcher starts with.
///
/// "Mountains in fog": a pale, low-contrast mist palette with soft horizontal
/// depth and a frosted-glass hero control. Light-on-light rather than the
/// dark-mode default most launchers ship, and readable at a glance on both a
/// bright and a dim monitor.
pub fn default_tokens() -> Tokens {
    Tokens {
        palette: Palette {
            accent: Color::rgb(104, 148, 170),
            accent_hover: Color::rgb(88, 132, 156),
            on_accent: Color::rgb(252, 253, 255),
            surface: Color::rgba(255, 255, 255, 0.40),
            surface_raised: Color::rgba(255, 255, 255, 0.52),
            surface_sunken: Color::rgba(198, 210, 217, 0.42),
            text: Color::rgb(36, 47, 55),
            text_muted: Color::rgb(99, 113, 122),
            border: Color::rgba(255, 255, 255, 0.62),
            success: Color::rgb(76, 152, 120),
            warning: Color::rgb(186, 138, 62),
            danger: Color::rgb(196, 91, 96),
            shadow: Color::rgba(92, 108, 118, 0.26),
            elevation: 0.45,
        },
        shape: Shape {
            radius_sm: 8,
            radius: 14,
            radius_lg: 22,
            radius_pill: 999,
            border_width: 1,
            ring_width: 3,
        },
        typography: Typography {
            family: FontFamily::System,
            display_family: FontFamily::System,
            scale: 1.0,
            weight_regular: 400,
            weight_bold: 620,
            letter_spacing: 0.0,
            line_height: 1.45,
            uppercase_labels: false,
        },
        motion: Motion {
            duration_fast: 140,
            duration: 240,
            duration_slow: 520,
            easing: Easing::Standard,
            reduced: ReducedMotion::System,
            ambient: true,
            hover_intensity: 1.0,
        },
        decor: Decor {
            grain: 0.04,
            vignette: 0.22,
            glow: 0.30,
            aurora: 0.45,
            rim: 0.35,
            pattern_scale: 0.5,
            pattern: Pattern::None,
            scanlines: false,
        },
        background: Background {
            // Near-vertical bands: the stops read as ridges receding into haze,
            // which is what "mountains in fog" actually looks like.
            kind: BackgroundKind::Linear,
            colors: vec![
                Color::rgb(240, 244, 248),
                Color::rgb(216, 226, 232),
                Color::rgb(234, 240, 243),
                Color::rgb(190, 204, 212),
                Color::rgb(222, 230, 234),
            ],
            angle: 172.0,
            image: None,
            image_fit: ImageFit::Cover,
            image_opacity: 1.0,
            image_blur: 0.0,
            dim: 0.0,
            artwork: Artwork::None,
            artwork_strength: 0.55,
        },
        components: Components {
            button: ComponentStyle::Soft,
            input: ComponentStyle::Soft,
            card: ComponentStyle::Glass,
            translucency: 0.62,
            density: 16,
        },
        layout: Layout::default(),
    }
}

/// The theme applied on a fresh install.
///
/// This *is* the `mist` preset, not a second copy of it. Registering a separate
/// `sifora-default` entry with the same display name is what made the gallery
/// show "Горы в тумане" twice.
pub const DEFAULT_THEME_ID: &str = "mist";

pub fn default_theme() -> ThemeSpec {
    let mut tokens = default_tokens();
    preset_mist(&mut tokens);
    let mut spec = ThemeSpec {
        meta: ThemeMeta {
            id: DEFAULT_THEME_ID.into(),
            name: "Горы в тумане".into(),
            author: "sifora".into(),
            description: "Светлая дымчатая тема: мягкие слои тумана и матовое стекло".into(),
            prompt: None,
            created_at: "1970-01-01T00:00:00Z".into(),
            builtin: true,
            schema: THEME_SCHEMA,
        },
        tokens,
    };
    spec.clamp_in_place();
    spec.sanitize_meta();
    spec
}

struct Preset {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    apply: fn(&mut Tokens),
}

const PRESETS: &[Preset] = &[
    Preset {
        id: "mist",
        name: "Горы в тумане",
        description: "Светлая дымка, мягкие слои тумана и матовое стекло",
        apply: preset_mist,
    },
    Preset {
        id: "sunset",
        name: "Закат",
        description: "Тёплый сумеречный градиент: слива, коралл и янтарь",
        apply: preset_sunset,
    },
    Preset {
        id: "deep-sea",
        name: "Океан",
        description: "Конический вихрь сине-зелёных тонов, узкие радиусы, серифный заголовок",
        apply: preset_deep_sea,
    },
    Preset {
        id: "forest",
        name: "Лес",
        description: "Тёмная зелень с янтарным акцентом, крупные формы и крапинка",
        apply: preset_forest,
    },
    Preset {
        id: "neon",
        name: "Неон",
        description: "Почти чёрный фон, сетка, бирюзовое свечение и прямые углы",
        apply: preset_neon,
    },
    Preset {
        id: "terminal",
        name: "Терминал",
        description: "Чёрно-зелёный фосфор, моноширинный шрифт, никакого свечения",
        apply: preset_terminal,
    },
];

pub fn builtin_themes() -> Vec<ThemeSpec> {
    let mut out = Vec::with_capacity(PRESETS.len());
    for preset in PRESETS {
        let mut tokens = default_tokens();
        (preset.apply)(&mut tokens);
        let mut spec = ThemeSpec {
            meta: ThemeMeta {
                id: preset.id.into(),
                name: preset.name.into(),
                author: "sifora".into(),
                description: preset.description.into(),
                prompt: None,
                created_at: "1970-01-01T00:00:00Z".into(),
                builtin: true,
                schema: THEME_SCHEMA,
            },
            tokens,
        };
        spec.clamp_in_place();
        spec.sanitize_meta();
        out.push(spec);
    }
    out
}

/// Metadata-only list used by the UI (avoids shipping every token twice).
#[derive(serde::Serialize)]
pub struct ThemeSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub builtin: bool,
    pub created_at: String,
    pub accent: String,
    pub background: String,
}

pub fn summarize(spec: &ThemeSpec) -> ThemeSummary {
    let bg = spec
        .tokens
        .background
        .colors
        .first()
        .map(|c| c.to_css())
        .unwrap_or_else(|| "#0b0d16".into());
    ThemeSummary {
        id: spec.meta.id.clone(),
        name: spec.meta.name.clone(),
        description: spec.meta.description.clone(),
        author: spec.meta.author.clone(),
        builtin: spec.meta.builtin,
        created_at: spec.meta.created_at.clone(),
        accent: spec.tokens.palette.accent.to_css(),
        background: bg,
    }
}

pub fn new_id() -> String {
    let ms = crate::util::now_ms();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut x = ms ^ (std::process::id() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    for _ in 0..3 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        h = h.wrapping_mul(0x100_0000_01B3).wrapping_add(x);
    }
    format!("t-{:016x}", h & 0xffff_ffff_ffff)
}

pub fn touch_meta(spec: &mut ThemeSpec, author: &str, prompt: Option<String>) {
    spec.meta.id = new_id();
    spec.meta.author = author.to_string();
    spec.meta.created_at = now_rfc3339();
    spec.meta.builtin = false;
    spec.meta.prompt = prompt;
}

// ---------------------------------------------------------------------------

fn preset_mist(t: &mut Tokens) {
    let p = &mut t.palette;
    p.accent = hsl(198.0, 0.32, 0.54);
    p.accent_hover = hsl(202.0, 0.36, 0.48);
    p.on_accent = Color::rgb(252, 253, 255);
    p.surface = Color::rgba(255, 255, 255, 0.40);
    p.surface_raised = Color::rgba(255, 255, 255, 0.52);
    p.surface_sunken = Color::rgba(198, 210, 217, 0.42);
    p.text = hsl(200.0, 0.18, 0.18);
    p.text_muted = hsl(200.0, 0.10, 0.44);
    p.border = Color::rgba(255, 255, 255, 0.62);
    p.shadow = hsl(200.0, 0.12, 0.35).with_alpha(0.24);
    p.elevation = 0.45;

    t.background.kind = BackgroundKind::Linear;
    t.background.colors = vec![
        hsl(205.0, 0.30, 0.96),
        hsl(202.0, 0.22, 0.88),
        hsl(200.0, 0.18, 0.94),
        hsl(198.0, 0.16, 0.79),
        hsl(203.0, 0.20, 0.89),
    ];
    t.background.angle = 172.0;

    t.background.artwork = Artwork::Contours;

    t.background.artwork_strength = 0.30;
    
    t.decor.vignette = 0.22;
    t.decor.glow = 0.30;
    t.decor.rim = 0.35;
    t.components.card = ComponentStyle::Glass;
    t.components.translucency = 0.62;
}

/// Warm twilight: plum into coral into amber. Dark, so it is the clear
/// counterpart to the light `mist` default rather than another blue gradient.
fn preset_sunset(t: &mut Tokens) {
    let p = &mut t.palette;
    p.accent = hsl(30.0, 0.86, 0.66);
    p.accent_hover = hsl(38.0, 0.90, 0.72);
    p.on_accent = Color::rgb(42, 24, 18);
    p.surface = hsla(322.0, 0.30, 0.13, 0.58);
    p.surface_raised = hsla(338.0, 0.30, 0.19, 0.66);
    p.surface_sunken = hsla(310.0, 0.34, 0.09, 0.54);
    p.text = hsl(28.0, 0.45, 0.95);
    p.text_muted = hsl(18.0, 0.30, 0.72);
    p.border = Color::rgba(255, 236, 220, 0.14);
    p.success = hsl(150.0, 0.52, 0.66);
    p.warning = hsl(44.0, 0.88, 0.70);
    p.danger = hsl(6.0, 0.72, 0.68);
    p.shadow = Color::rgba(18, 5, 12, 0.55);
    p.elevation = 0.55;

    t.shape.radius_sm = 10;
    t.shape.radius = 16;
    t.shape.radius_lg = 26;

    t.background.kind = BackgroundKind::Linear;
    t.background.colors = vec![
        hsl(300.0, 0.42, 0.13),
        hsl(348.0, 0.52, 0.26),
        hsl(12.0, 0.66, 0.44),
        hsl(34.0, 0.78, 0.58),
    ];
    t.background.angle = 158.0;

    t.components.button = ComponentStyle::Solid;
    t.components.card = ComponentStyle::Soft;
    t.components.translucency = 0.60;

    t.background.artwork = Artwork::Waves;

    t.background.artwork_strength = 0.55;
    
    t.decor.rim = 0.28;
    t.decor.grain = 0.06;
    t.decor.vignette = 0.42;
    t.decor.pattern = Pattern::None;
    t.decor.scanlines = false;
    t.motion.easing = Easing::Standard;
}

/// A slow conic swirl of blue-green. Narrow radii and a serif display face
/// make it read as a different interface, not just a different hue.
fn preset_deep_sea(t: &mut Tokens) {
    let p = &mut t.palette;
    p.accent = hsl(172.0, 0.66, 0.56);
    p.accent_hover = hsl(178.0, 0.72, 0.64);
    p.on_accent = Color::rgb(4, 33, 29);
    p.surface = hsla(195.0, 0.55, 0.13, 0.62);
    p.surface_raised = hsla(190.0, 0.52, 0.19, 0.70);
    p.surface_sunken = hsla(205.0, 0.60, 0.08, 0.60);
    p.text = hsl(185.0, 0.40, 0.95);
    p.text_muted = hsl(192.0, 0.26, 0.68);
    p.border = Color::rgba(190, 240, 244, 0.14);
    p.success = hsl(150.0, 0.55, 0.62);
    p.warning = hsl(40.0, 0.80, 0.66);
    p.danger = hsl(356.0, 0.68, 0.66);
    p.shadow = Color::rgba(2, 14, 20, 0.58);
    p.elevation = 0.6;

    t.shape.radius_sm = 6;
    t.shape.radius = 9;
    t.shape.radius_lg = 16;

    t.typography.display_family = FontFamily::Georgia;
    t.typography.letter_spacing = 0.01;

    t.background.kind = BackgroundKind::Conic;
    t.background.colors = vec![
        hsl(196.0, 0.62, 0.10),
        hsl(186.0, 0.58, 0.17),
        hsl(172.0, 0.52, 0.22),
        hsl(205.0, 0.66, 0.12),
    ];
    t.background.angle = 205.0;

    t.components.button = ComponentStyle::Solid;
    t.components.card = ComponentStyle::Soft;
    t.components.translucency = 0.66;

    t.background.artwork = Artwork::Bubbles;

    t.background.artwork_strength = 0.55;
    
    t.decor.rim = 0.20;
    t.decor.grain = 0.05;
    t.decor.vignette = 0.52;
    t.decor.pattern = Pattern::None;
    t.motion.easing = Easing::Decelerate;
}

/// Deep green with an amber accent, generous radii and a dotted texture.
fn preset_forest(t: &mut Tokens) {
    let p = &mut t.palette;
    p.accent = hsl(42.0, 0.72, 0.60);
    p.accent_hover = hsl(48.0, 0.80, 0.68);
    p.on_accent = Color::rgb(30, 20, 6);
    p.surface = hsla(150.0, 0.38, 0.12, 0.56);
    p.surface_raised = hsla(146.0, 0.36, 0.18, 0.64);
    p.surface_sunken = hsla(160.0, 0.44, 0.07, 0.54);
    p.text = hsl(130.0, 0.28, 0.95);
    p.text_muted = hsl(140.0, 0.20, 0.68);
    p.border = Color::rgba(214, 240, 220, 0.13);
    p.success = hsl(96.0, 0.55, 0.60);
    p.warning = hsl(42.0, 0.80, 0.62);
    p.danger = hsl(2.0, 0.66, 0.62);
    p.shadow = Color::rgba(4, 16, 10, 0.55);
    p.elevation = 0.5;

    t.shape.radius_sm = 12;
    t.shape.radius = 18;
    t.shape.radius_lg = 30;

    t.background.kind = BackgroundKind::Mesh;
    t.background.colors = vec![
        hsl(158.0, 0.50, 0.07),
        hsl(146.0, 0.42, 0.15),
        hsl(120.0, 0.36, 0.11),
        hsl(168.0, 0.52, 0.05),
    ];
    t.background.angle = 118.0;

    t.components.button = ComponentStyle::Soft;
    t.components.card = ComponentStyle::Glass;
    t.components.translucency = 0.58;

    t.background.artwork = Artwork::Blocks;

    t.background.artwork_strength = 0.42;
    
    t.decor.aurora = 0.45;
    t.decor.glow = 0.32;
    t.decor.rim = 0.18;
    t.decor.grain = 0.07;
    t.decor.vignette = 0.44;
    t.decor.scanlines = false;
    t.motion.easing = Easing::Standard;
}

/// Near-black, a fine grid, scanlines and a hard cyan glow. The most
/// "designed" of the set: uppercase labels, squared corners, outline buttons.
fn preset_neon(t: &mut Tokens) {
    let p = &mut t.palette;
    p.accent = hsl(168.0, 0.88, 0.56);
    p.accent_hover = hsl(172.0, 0.94, 0.68);
    p.on_accent = Color::rgb(2, 26, 22);
    p.surface = hsla(226.0, 0.52, 0.09, 0.60);
    p.surface_raised = hsla(224.0, 0.48, 0.14, 0.70);
    p.surface_sunken = hsla(230.0, 0.58, 0.05, 0.62);
    p.text = hsl(190.0, 0.60, 0.97);
    p.text_muted = hsl(214.0, 0.36, 0.72);
    p.border = hsla(168.0, 0.70, 0.60, 0.20);
    p.success = hsl(150.0, 0.80, 0.58);
    p.warning = hsl(48.0, 0.95, 0.60);
    p.danger = hsl(330.0, 0.90, 0.66);
    p.shadow = Color::rgba(0, 0, 6, 0.65);
    p.elevation = 0.65;

    t.shape.radius_sm = 4;
    t.shape.radius = 7;
    t.shape.radius_lg = 12;

    t.typography.uppercase_labels = true;
    t.typography.letter_spacing = 0.08;

    t.background.kind = BackgroundKind::Radial;
    t.background.colors = vec![
        hsl(168.0, 0.60, 0.16),
        hsl(226.0, 0.60, 0.10),
        hsl(300.0, 0.45, 0.08),
        hsl(228.0, 0.62, 0.04),
    ];
    t.background.angle = 135.0;

    t.components.button = ComponentStyle::Outline;
    t.components.card = ComponentStyle::Soft;
    t.components.translucency = 0.74;

    t.background.artwork = Artwork::Circuit;

    t.background.artwork_strength = 0.62;
    
    t.decor.scanlines = true;
    t.decor.glow = 0.85;
    t.decor.rim = 0.50;
    t.decor.aurora = 0.22;
    t.decor.grain = 0.06;
    t.decor.vignette = 0.60;
    t.motion.easing = Easing::Spring;
    t.motion.duration = 200;
}

/// Black-and-green phosphor. Monospace everywhere, no glow, no blur-heavy
/// decoration — the deliberately austere option.
fn preset_terminal(t: &mut Tokens) {
    let p = &mut t.palette;
    p.accent = hsl(138.0, 0.68, 0.60);
    p.accent_hover = hsl(142.0, 0.76, 0.70);
    p.on_accent = Color::rgb(2, 20, 10);
    p.surface = hsla(146.0, 0.60, 0.07, 0.80);
    p.surface_raised = hsla(142.0, 0.55, 0.12, 0.86);
    p.surface_sunken = hsla(150.0, 0.66, 0.04, 0.78);
    p.text = hsl(140.0, 0.55, 0.76);
    p.text_muted = hsl(142.0, 0.38, 0.46);
    p.border = hsla(140.0, 0.60, 0.55, 0.22);
    p.success = hsl(138.0, 0.68, 0.60);
    p.warning = hsl(52.0, 0.80, 0.62);
    p.danger = hsl(6.0, 0.78, 0.62);
    p.shadow = Color::rgba(0, 12, 6, 0.62);
    p.elevation = 0.7;

    t.shape.radius_sm = 2;
    t.shape.radius = 4;
    t.shape.radius_lg = 7;

    t.typography.family = FontFamily::Consolas;
    t.typography.display_family = FontFamily::Consolas;
    t.typography.uppercase_labels = true;
    t.typography.letter_spacing = 0.03;

    t.background.kind = BackgroundKind::Solid;
    t.background.colors = vec![hsl(150.0, 0.62, 0.04)];
    t.background.angle = 0.0;

    t.components.button = ComponentStyle::Outline;
    t.components.card = ComponentStyle::Soft;
    t.components.translucency = 0.90;

    t.background.artwork = Artwork::Confetti;

    t.background.artwork_strength = 0.35;
    
    t.decor.glow = 0.16;
    t.decor.rim = 0.10;
    t.decor.aurora = 0.0;
    t.decor.grain = 0.14;
    t.decor.vignette = 0.68;
    t.decor.scanlines = false;
    t.motion.easing = Easing::Linear;
    t.motion.duration = 150;
    t.motion.ambient = false;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::validate::validate_theme;

    #[test]
    fn every_builtin_theme_passes_validation() {
        let mut all = builtin_themes();
        assert!(all.len() >= 6);
        for theme in &mut all {
            let report = validate_theme(theme);
            assert!(
                report.errors.is_empty(),
                "builtin {:?} has errors: {:?}",
                theme.meta.id,
                report.errors
            );
        }
    }

    #[test]
    fn builtin_ids_and_names_are_unique() {
        let all = builtin_themes();
        let mut ids: Vec<&str> = all.iter().map(|t| t.meta.id.as_str()).collect();
        let mut names: Vec<String> = all.iter().map(|t| t.meta.name.to_lowercase()).collect();
        ids.sort_unstable();
        names.sort();
        let id_count = ids.len();
        let name_count = names.len();
        ids.dedup();
        names.dedup();
        assert_eq!(ids.len(), id_count, "duplicate builtin id");
        assert_eq!(names.len(), name_count, "duplicate builtin name");
        for id in ids {
            assert!(!id.is_empty());
            assert!(id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'));
        }
    }

    /// The shipped default must be one of the presets, never a second copy of
    /// one: that is exactly how the gallery ended up with two identical entries.
    #[test]
    fn default_theme_is_exactly_one_of_the_presets() {
        let default = default_theme();
        let presets = builtin_themes();
        let matches = presets
            .iter()
            .filter(|p| p.tokens == default.tokens && p.meta.name == default.meta.name)
            .count();
        assert_eq!(matches, 1, "the default theme must appear exactly once in the preset list");
        assert_eq!(default.meta.id, DEFAULT_THEME_ID);
    }

    #[test]
    fn light_preset_has_readable_text() {
        let mist = builtin_themes().into_iter().find(|t| t.meta.id == "mist").unwrap();
        let p = &mist.tokens.palette;
        let ratio = Color::contrast_ratio(p.text, p.surface_raised);
        assert!(ratio >= 7.0, "mist text contrast too low: {ratio}");
    }

    /// Two presets that look alike are worse than one preset that looks good.
    /// Each preset therefore has to differ from every other one on at least two
    /// identity signals: background hue, background kind, shape ramp, surface
    /// pattern, button style and typography.
    #[test]
    fn presets_are_visually_distinguishable() {
        let all = builtin_themes();
        assert!(all.len() >= 5, "the preset set must not shrink into near-duplicates");

        for (i, a) in all.iter().enumerate() {
            for b in all.iter().skip(i + 1) {
                let mut signals = Vec::new();

                let hue_a = a.tokens.background.colors[0].hsl().0;
                let hue_b = b.tokens.background.colors[0].hsl().0;
                let hue_delta = (hue_a - hue_b).abs().min(360.0 - (hue_a - hue_b).abs());
                signals.push(hue_delta > 22.0);

                signals.push(a.tokens.background.kind != b.tokens.background.kind);
                signals.push(
                    (a.tokens.shape.radius as i32 - b.tokens.shape.radius as i32).abs() >= 5,
                );
                signals.push(a.tokens.decor.pattern != b.tokens.decor.pattern);
                signals.push(a.tokens.components.button != b.tokens.components.button);
                signals.push(a.tokens.typography.family != b.tokens.typography.family);
                signals.push(
                    a.tokens.palette.surface.is_light() != b.tokens.palette.surface.is_light(),
                );

                let differing = signals.iter().filter(|s| **s).count();
                assert!(
                    differing >= 2,
                    "{} and {} look too alike (only {differing} differing signals, hue Δ{hue_delta:.0}°)",
                    a.meta.id,
                    b.meta.id
                );
            }
        }
    }

    /// The shipped set must stay genuinely spread out rather than collapsing
    /// into variations of one hue.
    #[test]
    fn preset_background_hues_are_spread_out() {
        let mut hues: Vec<f32> = builtin_themes()
            .iter()
            .map(|t| t.tokens.background.colors[0].hsl().0)
            .collect();
        hues.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for pair in hues.windows(2) {
            assert!(pair[1] - pair[0] > 8.0, "two presets share nearly the same background hue");
        }
    }
}
