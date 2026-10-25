//! Built-in starting points.
//!
//! These ship with the app and cover the "pick your own background" need out of
//! the box. They are ordinary [`ThemeSpec`]s, so nothing about them is special
//! compared to a theme the user built by hand — they go through exactly the same
//! validation and rendering path.
//!
//! The set is deliberately soft rather than brutalist: rounded geometry,
//! translucent glass, gentle motion and one clear accent per theme. Each preset
//! has its own hue, background construction, typeface and shape so the gallery
//! reads as thirteen different moods instead of thirteen colour swatches.

use super::color::{hsl, Color};
use super::model::*;
use crate::util::now_rfc3339;

/// The theme the launcher starts with.
///
/// "Туман": cool daylight haze behind frosted glass. Light rather than the dark
/// default most launchers ship, with generous rounding and a single muted blue
/// accent, so the window stays calm at any brightness.
pub fn default_tokens() -> Tokens {
    Tokens {
        palette: Palette {
            accent: Color::rgb(106, 147, 176),
            accent_hover: Color::rgb(90, 134, 164),
            on_accent: Color::rgb(255, 255, 255),
            surface: Color::rgba(255, 255, 255, 0.55),
            surface_raised: Color::rgba(255, 255, 255, 0.82),
            surface_sunken: Color::rgba(224, 234, 240, 0.70),
            text: Color::rgb(31, 42, 51),
            text_muted: Color::rgb(93, 107, 118),
            border: Color::rgba(31, 42, 51, 0.12),
            success: Color::rgb(47, 138, 99),
            warning: Color::rgb(168, 116, 28),
            danger: Color::rgb(184, 69, 75),
            shadow: Color::rgba(38, 56, 70, 0.14),
            elevation: 0.30,
        },
        shape: Shape {
            radius_sm: 16,
            radius: 24,
            radius_lg: 32,
            radius_pill: 999,
            border_width: 1,
            ring_width: 3,
        },
        typography: Typography {
            family: FontFamily::System,
            display_family: FontFamily::System,
            scale: 1.0,
            weight_regular: 400,
            weight_bold: 650,
            letter_spacing: 0.005,
            line_height: 1.5,
            uppercase_labels: false,
        },
        motion: Motion {
            duration_fast: 160,
            duration: 260,
            duration_slow: 540,
            easing: Easing::Standard,
            reduced: ReducedMotion::System,
            ambient: true,
            hover_intensity: 1.0,
        },
        decor: Decor {
            grain: 0.04,
            vignette: 0.12,
            glow: 0.25,
            aurora: 0.35,
            rim: 0.25,
            pattern_scale: 0.5,
            pattern: Pattern::None,
            scanlines: false,
        },
        background: Background {
            kind: BackgroundKind::Linear,
            colors: vec![
                hsl(200.0, 0.24, 0.94),
                hsl(206.0, 0.20, 0.88),
                hsl(196.0, 0.22, 0.96),
            ],
            angle: 165.0,
            image: None,
            image_fit: ImageFit::Cover,
            image_opacity: 1.0,
            image_blur: 0.0,
            dim: 0.0,
            artwork: Artwork::Contours,
            artwork_strength: 0.18,
        },
        components: Components {
            button: ComponentStyle::Soft,
            input: ComponentStyle::Soft,
            card: ComponentStyle::Glass,
            translucency: 0.60,
            density: 18,
        },
        layout: Layout::default(),
    }
}

/// The theme applied on a fresh install.
///
/// This *is* the `mist` preset, not a second copy of it. Registering a separate
/// `sifora-default` entry with the same display name is what made the gallery
/// show the same theme twice.
pub const DEFAULT_THEME_ID: &str = "mist";

pub fn default_theme() -> ThemeSpec {
    let mut tokens = default_tokens();
    preset_mist(&mut tokens);
    let mut spec = ThemeSpec {
        meta: ThemeMeta {
            id: DEFAULT_THEME_ID.into(),
            name: "Туман".into(),
            author: "sifora".into(),
            description: "Прохладный дневной свет: матовое стекло, мягкие углы, спокойный синий акцент".into(),
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

/// Gallery order: airy light themes first, deep ones at the end.
const PRESETS: &[Preset] = &[
    Preset {
        id: "mist",
        name: "Туман",
        description: "Прохладный дневной свет: матовое стекло, мягкие углы, спокойный синий акцент",
        apply: preset_mist,
    },
    Preset {
        id: "sakura",
        name: "Сакура",
        description: "Нежный розовый рассвет, округлые формы и воздушные пузыри",
        apply: preset_sakura,
    },
    Preset {
        id: "orchid",
        name: "Орхидея",
        description: "Сиреневая дымка, глубокий шрифт с засечками, плавные переходы",
        apply: preset_orchid,
    },
    Preset {
        id: "sunset",
        name: "Закат",
        description: "Тёплый персиково-медовый градиент и мягкое солнечное свечение",
        apply: preset_sunset,
    },
    Preset {
        id: "honey",
        name: "Мёд",
        description: "Кремовая бумага, золотой акцент, тонкие контурные линии",
        apply: preset_honey,
    },
    Preset {
        id: "citrus",
        name: "Цитрус",
        description: "Свежий лаймовый свет, лёгкая точка и пружинистые анимации",
        apply: preset_citrus,
    },
    Preset {
        id: "sage",
        name: "Шалфей",
        description: "Спокойная зелень, топографический узор и тихая типографика",
        apply: preset_sage,
    },
    Preset {
        id: "ocean",
        name: "Океан",
        description: "Глубокий бирюзовый сумрак, волны и холодное стекло",
        apply: preset_ocean,
    },
    Preset {
        id: "midnight",
        name: "Полночь",
        description: "Тёмно-синий бархат, звёзды и элегантный шрифт с засечками",
        apply: preset_midnight,
    },
    Preset {
        id: "cosmos",
        name: "Космос",
        description: "Фиолетовая туманность, звёздная пыль и моноширинный набор",
        apply: preset_cosmos,
    },
    Preset {
        id: "forest",
        name: "Лес",
        description: "Хвойная глубина, топографические линии и тёплый каменный текст",
        apply: preset_forest,
    },
    Preset {
        id: "ember",
        name: "Янтарь",
        description: "Тёмное дерево и тлеющий янтарный свет, плотные тёплые панели",
        apply: preset_ember,
    },
    Preset {
        id: "rose",
        name: "Роза",
        description: "Винный бархат, пудровый акцент и строгая низкая геометрия",
        apply: preset_rose,
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
// Light themes

/// Cool daylight haze: the shipped default. Soft white glass, blue accent and
/// the largest rounding of the set.
fn preset_mist(t: &mut Tokens) {
    let p = &mut t.palette;
    p.accent = Color::rgb(106, 147, 176);
    p.accent_hover = Color::rgb(90, 134, 164);
    p.on_accent = Color::rgb(255, 255, 255);
    p.surface = Color::rgba(255, 255, 255, 0.55);
    p.surface_raised = Color::rgba(255, 255, 255, 0.82);
    p.surface_sunken = Color::rgba(224, 234, 240, 0.70);
    p.text = Color::rgb(31, 42, 51);
    p.text_muted = Color::rgb(93, 107, 118);
    p.border = Color::rgba(31, 42, 51, 0.12);
    p.success = Color::rgb(47, 138, 99);
    p.warning = Color::rgb(168, 116, 28);
    p.danger = Color::rgb(184, 69, 75);
    p.shadow = Color::rgba(38, 56, 70, 0.14);
    p.elevation = 0.30;

    t.shape.radius_sm = 16;
    t.shape.radius = 24;
    t.shape.radius_lg = 32;
    t.shape.radius_pill = 999;
    t.shape.border_width = 1;
    t.shape.ring_width = 3;

    t.typography.family = FontFamily::System;
    t.typography.display_family = FontFamily::System;
    t.typography.scale = 1.0;
    t.typography.weight_regular = 400;
    t.typography.weight_bold = 650;
    t.typography.letter_spacing = 0.005;
    t.typography.line_height = 1.5;
    t.typography.uppercase_labels = false;

    t.motion.duration_fast = 160;
    t.motion.duration = 260;
    t.motion.duration_slow = 540;
    t.motion.easing = Easing::Standard;
    t.motion.ambient = true;
    t.motion.hover_intensity = 1.0;

    t.background.kind = BackgroundKind::Linear;
    t.background.colors = vec![
        hsl(200.0, 0.24, 0.94),
        hsl(206.0, 0.20, 0.88),
        hsl(196.0, 0.22, 0.96),
    ];
    t.background.angle = 165.0;
    t.background.artwork = Artwork::Contours;
    t.background.artwork_strength = 0.18;

    t.decor.grain = 0.04;
    t.decor.vignette = 0.12;
    t.decor.glow = 0.25;
    t.decor.aurora = 0.35;
    t.decor.rim = 0.25;
    t.decor.pattern = Pattern::None;

    t.components.button = ComponentStyle::Soft;
    t.components.input = ComponentStyle::Soft;
    t.components.card = ComponentStyle::Glass;
    t.components.translucency = 0.60;
    t.components.density = 18;

    t.layout.corners = Corners::Squircle;
    t.layout.motion = MotionStyle::Smooth;
    t.layout.label_case = LabelCase::Title;
}

/// Blush dawn. Rounder than mist, with bubbles and a deeper rose accent.
fn preset_sakura(t: &mut Tokens) {
    let p = &mut t.palette;
    p.accent = Color::rgb(207, 79, 125);
    p.accent_hover = Color::rgb(190, 62, 108);
    p.on_accent = Color::rgb(255, 255, 255);
    p.surface = Color::rgba(255, 255, 255, 0.60);
    p.surface_raised = Color::rgba(255, 251, 253, 0.86);
    p.surface_sunken = Color::rgba(250, 232, 240, 0.74);
    p.text = Color::rgb(51, 33, 42);
    p.text_muted = Color::rgb(125, 100, 112);
    p.border = Color::rgba(92, 44, 64, 0.12);
    p.success = Color::rgb(47, 138, 99);
    p.warning = Color::rgb(168, 116, 28);
    p.danger = Color::rgb(184, 69, 75);
    p.shadow = Color::rgba(92, 44, 64, 0.14);
    p.elevation = 0.30;

    t.shape.radius_sm = 22;
    t.shape.radius = 30;
    t.shape.radius_lg = 40;
    t.shape.radius_pill = 999;
    t.shape.border_width = 1;
    t.shape.ring_width = 3;

    t.typography.family = FontFamily::Palatino;
    t.typography.display_family = FontFamily::Palatino;
    t.typography.scale = 1.0;
    t.typography.letter_spacing = 0.005;
    t.typography.line_height = 1.58;
    t.typography.uppercase_labels = false;

    t.motion.duration_fast = 180;
    t.motion.duration = 300;
    t.motion.duration_slow = 640;
    t.motion.easing = Easing::Spring;
    t.motion.ambient = true;

    t.background.kind = BackgroundKind::Linear;
    t.background.colors = vec![
        hsl(308.0, 0.42, 0.95),
        hsl(322.0, 0.36, 0.90),
        hsl(296.0, 0.34, 0.95),
    ];
    t.background.angle = 155.0;
    t.background.artwork = Artwork::Bubbles;
    t.background.artwork_strength = 0.25;

    t.decor.grain = 0.03;
    t.decor.vignette = 0.08;
    t.decor.glow = 0.28;
    t.decor.aurora = 0.40;
    t.decor.rim = 0.25;
    t.decor.pattern = Pattern::None;

    t.components.button = ComponentStyle::Soft;
    t.components.input = ComponentStyle::Soft;
    t.components.card = ComponentStyle::Glass;
    t.components.translucency = 0.60;
    t.components.density = 18;

    t.layout.corners = Corners::Pill;
    t.layout.motion = MotionStyle::Springy;
    t.layout.label_case = LabelCase::Title;
}

/// Lilac mesh wash with a serif voice and very soft contours.
fn preset_orchid(t: &mut Tokens) {
    let p = &mut t.palette;
    p.accent = Color::rgb(131, 88, 196);
    p.accent_hover = Color::rgb(115, 74, 178);
    p.on_accent = Color::rgb(255, 255, 255);
    p.surface = Color::rgba(255, 255, 255, 0.62);
    p.surface_raised = Color::rgba(255, 255, 255, 0.86);
    p.surface_sunken = Color::rgba(236, 230, 246, 0.74);
    p.text = Color::rgb(43, 35, 56);
    p.text_muted = Color::rgb(108, 96, 130);
    p.border = Color::rgba(60, 44, 86, 0.12);
    p.success = Color::rgb(47, 138, 99);
    p.warning = Color::rgb(168, 116, 28);
    p.danger = Color::rgb(184, 69, 75);
    p.shadow = Color::rgba(60, 44, 86, 0.14);
    p.elevation = 0.30;

    t.shape.radius_sm = 18;
    t.shape.radius = 26;
    t.shape.radius_lg = 34;
    t.shape.radius_pill = 999;
    t.shape.border_width = 1;
    t.shape.ring_width = 3;

    t.typography.family = FontFamily::Georgia;
    t.typography.display_family = FontFamily::Georgia;
    t.typography.scale = 1.0;
    t.typography.letter_spacing = 0.0;
    t.typography.line_height = 1.58;
    t.typography.uppercase_labels = false;

    t.motion.duration_fast = 170;
    t.motion.duration = 280;
    t.motion.duration_slow = 580;
    t.motion.easing = Easing::Decelerate;
    t.motion.ambient = true;

    t.background.kind = BackgroundKind::Mesh;
    t.background.colors = vec![
        hsl(281.0, 0.34, 0.94),
        hsl(292.0, 0.30, 0.89),
        hsl(268.0, 0.32, 0.93),
        hsl(302.0, 0.26, 0.87),
    ];
    t.background.angle = 140.0;
    t.background.artwork = Artwork::Contours;
    t.background.artwork_strength = 0.20;

    t.decor.grain = 0.04;
    t.decor.vignette = 0.10;
    t.decor.glow = 0.30;
    t.decor.aurora = 0.45;
    t.decor.rim = 0.30;
    t.decor.pattern = Pattern::None;

    t.components.button = ComponentStyle::Soft;
    t.components.input = ComponentStyle::Soft;
    t.components.card = ComponentStyle::Glass;
    t.components.translucency = 0.55;
    t.components.density = 18;

    t.layout.corners = Corners::Squircle;
    t.layout.motion = MotionStyle::Smooth;
    t.layout.label_case = LabelCase::Title;
}

/// Warm dusk light: peach into honey, with a sun low on the horizon.
fn preset_sunset(t: &mut Tokens) {
    let p = &mut t.palette;
    p.accent = Color::rgb(194, 85, 56);
    p.accent_hover = Color::rgb(176, 70, 44);
    p.on_accent = Color::rgb(255, 246, 241);
    p.surface = Color::rgba(255, 252, 248, 0.64);
    p.surface_raised = Color::rgba(255, 252, 248, 0.88);
    p.surface_sunken = Color::rgba(250, 232, 220, 0.74);
    p.text = Color::rgb(53, 35, 28);
    p.text_muted = Color::rgb(125, 100, 89);
    p.border = Color::rgba(96, 54, 40, 0.12);
    p.success = Color::rgb(47, 138, 99);
    p.warning = Color::rgb(168, 116, 28);
    p.danger = Color::rgb(184, 69, 75);
    p.shadow = Color::rgba(96, 54, 40, 0.15);
    p.elevation = 0.32;

    t.shape.radius_sm = 18;
    t.shape.radius = 26;
    t.shape.radius_lg = 34;
    t.shape.radius_pill = 999;
    t.shape.border_width = 1;
    t.shape.ring_width = 3;

    t.typography.family = FontFamily::Times;
    t.typography.display_family = FontFamily::Times;
    t.typography.scale = 1.02;
    t.typography.letter_spacing = 0.0;
    t.typography.line_height = 1.58;
    t.typography.uppercase_labels = false;

    t.motion.duration_fast = 180;
    t.motion.duration = 300;
    t.motion.duration_slow = 600;
    t.motion.easing = Easing::Decelerate;
    t.motion.ambient = true;

    t.background.kind = BackgroundKind::Mesh;
    t.background.colors = vec![
        hsl(29.0, 0.55, 0.92),
        hsl(15.0, 0.50, 0.86),
        hsl(43.0, 0.52, 0.94),
        hsl(345.0, 0.40, 0.90),
    ];
    t.background.angle = 135.0;
    t.background.artwork = Artwork::Sunburst;
    t.background.artwork_strength = 0.24;

    t.decor.grain = 0.05;
    t.decor.vignette = 0.12;
    t.decor.glow = 0.35;
    t.decor.aurora = 0.45;
    t.decor.rim = 0.30;
    t.decor.pattern = Pattern::None;

    t.components.button = ComponentStyle::Soft;
    t.components.input = ComponentStyle::Soft;
    t.components.card = ComponentStyle::Glass;
    t.components.translucency = 0.58;
    t.components.density = 18;

    t.layout.corners = Corners::Squircle;
    t.layout.motion = MotionStyle::Smooth;
    t.layout.label_case = LabelCase::Title;
}

/// Cream and gold, printed rather than lit: warm paper, amber accent, fine
/// contour engraving.
fn preset_honey(t: &mut Tokens) {
    let p = &mut t.palette;
    p.accent = Color::rgb(201, 138, 30);
    p.accent_hover = Color::rgb(180, 120, 20);
    p.on_accent = Color::rgb(42, 26, 0);
    p.surface = Color::rgba(255, 253, 247, 0.70);
    p.surface_raised = Color::rgba(255, 255, 251, 0.90);
    p.surface_sunken = Color::rgba(247, 238, 219, 0.80);
    p.text = Color::rgb(51, 40, 15);
    p.text_muted = Color::rgb(122, 106, 74);
    p.border = Color::rgba(90, 70, 20, 0.14);
    p.success = Color::rgb(47, 127, 82);
    p.warning = Color::rgb(168, 116, 28);
    p.danger = Color::rgb(184, 69, 75);
    p.shadow = Color::rgba(90, 70, 20, 0.16);
    p.elevation = 0.30;

    t.shape.radius_sm = 16;
    t.shape.radius = 22;
    t.shape.radius_lg = 30;
    t.shape.radius_pill = 999;
    t.shape.border_width = 1;
    t.shape.ring_width = 3;

    t.typography.family = FontFamily::Trebuchet;
    t.typography.display_family = FontFamily::Trebuchet;
    t.typography.scale = 1.0;
    t.typography.letter_spacing = 0.005;
    t.typography.line_height = 1.52;
    t.typography.uppercase_labels = false;

    t.motion.duration_fast = 170;
    t.motion.duration = 280;
    t.motion.duration_slow = 560;
    t.motion.easing = Easing::Decelerate;
    t.motion.ambient = true;

    t.background.kind = BackgroundKind::Linear;
    t.background.colors = vec![
        hsl(56.0, 0.55, 0.92),
        hsl(44.0, 0.50, 0.86),
        hsl(66.0, 0.45, 0.94),
    ];
    t.background.angle = 150.0;
    t.background.artwork = Artwork::Contours;
    t.background.artwork_strength = 0.20;

    t.decor.grain = 0.05;
    t.decor.vignette = 0.12;
    t.decor.glow = 0.30;
    t.decor.aurora = 0.35;
    t.decor.rim = 0.30;
    t.decor.pattern = Pattern::None;

    t.components.button = ComponentStyle::Soft;
    t.components.input = ComponentStyle::Soft;
    t.components.card = ComponentStyle::Glass;
    t.components.translucency = 0.62;
    t.components.density = 18;

    t.layout.corners = Corners::Squircle;
    t.layout.motion = MotionStyle::Smooth;
    t.layout.label_case = LabelCase::Title;
}

/// Fresh lime: the brightest preset, with a dot screen and springy motion.
fn preset_citrus(t: &mut Tokens) {
    let p = &mut t.palette;
    p.accent = Color::rgb(79, 138, 16);
    p.accent_hover = Color::rgb(64, 116, 10);
    p.on_accent = Color::rgb(255, 255, 255);
    p.surface = Color::rgba(252, 255, 246, 0.66);
    p.surface_raised = Color::rgba(255, 255, 250, 0.90);
    p.surface_sunken = Color::rgba(232, 242, 214, 0.78);
    p.text = Color::rgb(31, 42, 18);
    p.text_muted = Color::rgb(95, 107, 76);
    p.border = Color::rgba(60, 86, 20, 0.14);
    p.success = Color::rgb(47, 127, 82);
    p.warning = Color::rgb(168, 116, 28);
    p.danger = Color::rgb(184, 69, 75);
    p.shadow = Color::rgba(60, 86, 20, 0.15);
    p.elevation = 0.30;

    t.shape.radius_sm = 14;
    t.shape.radius = 18;
    t.shape.radius_lg = 26;
    t.shape.radius_pill = 999;
    t.shape.border_width = 1;
    t.shape.ring_width = 3;

    t.typography.family = FontFamily::Segoe;
    t.typography.display_family = FontFamily::Segoe;
    t.typography.scale = 1.0;
    t.typography.weight_bold = 700;
    t.typography.letter_spacing = 0.005;
    t.typography.line_height = 1.5;
    t.typography.uppercase_labels = false;

    t.motion.duration_fast = 150;
    t.motion.duration = 240;
    t.motion.duration_slow = 480;
    t.motion.easing = Easing::Spring;
    t.motion.ambient = true;

    t.background.kind = BackgroundKind::Aurora;
    t.background.colors = vec![
        hsl(83.0, 0.55, 0.91),
        hsl(95.0, 0.48, 0.85),
        hsl(72.0, 0.45, 0.94),
        hsl(105.0, 0.40, 0.88),
    ];
    t.background.angle = 120.0;
    t.background.artwork = Artwork::Confetti;
    t.background.artwork_strength = 0.22;

    t.decor.grain = 0.05;
    t.decor.vignette = 0.10;
    t.decor.glow = 0.25;
    t.decor.aurora = 0.55;
    t.decor.rim = 0.25;
    t.decor.pattern = Pattern::Dots;
    t.decor.pattern_scale = 0.30;

    t.components.button = ComponentStyle::Soft;
    t.components.input = ComponentStyle::Outline;
    t.components.card = ComponentStyle::Soft;
    t.components.translucency = 0.60;
    t.components.density = 17;

    t.layout.corners = Corners::Rounded;
    t.layout.motion = MotionStyle::Springy;
    t.layout.label_case = LabelCase::Title;
}

/// Herbal calm: a radial sage wash with topographic contour lines.
fn preset_sage(t: &mut Tokens) {
    let p = &mut t.palette;
    p.accent = Color::rgb(63, 122, 92);
    p.accent_hover = Color::rgb(50, 104, 77);
    p.on_accent = Color::rgb(255, 255, 255);
    p.surface = Color::rgba(250, 253, 250, 0.68);
    p.surface_raised = Color::rgba(255, 255, 255, 0.90);
    p.surface_sunken = Color::rgba(226, 238, 228, 0.78);
    p.text = Color::rgb(30, 42, 34);
    p.text_muted = Color::rgb(91, 107, 96);
    p.border = Color::rgba(40, 70, 52, 0.13);
    p.success = Color::rgb(47, 127, 82);
    p.warning = Color::rgb(168, 116, 28);
    p.danger = Color::rgb(184, 69, 75);
    p.shadow = Color::rgba(40, 70, 52, 0.14);
    p.elevation = 0.28;

    t.shape.radius_sm = 18;
    t.shape.radius = 26;
    t.shape.radius_lg = 34;
    t.shape.radius_pill = 999;
    t.shape.border_width = 1;
    t.shape.ring_width = 3;

    t.typography.family = FontFamily::Tahoma;
    t.typography.display_family = FontFamily::Tahoma;
    t.typography.scale = 0.98;
    t.typography.letter_spacing = 0.0;
    t.typography.line_height = 1.55;
    t.typography.uppercase_labels = false;

    t.motion.duration_fast = 190;
    t.motion.duration = 320;
    t.motion.duration_slow = 660;
    t.motion.easing = Easing::Standard;
    t.motion.ambient = true;

    t.background.kind = BackgroundKind::Radial;
    t.background.colors = vec![
        hsl(110.0, 0.30, 0.92),
        hsl(120.0, 0.26, 0.86),
        hsl(100.0, 0.24, 0.95),
    ];
    t.background.angle = 0.0;
    t.background.artwork = Artwork::Contours;
    t.background.artwork_strength = 0.24;

    t.decor.grain = 0.06;
    t.decor.vignette = 0.12;
    t.decor.glow = 0.20;
    t.decor.aurora = 0.30;
    t.decor.rim = 0.25;
    t.decor.pattern = Pattern::Topography;
    t.decor.pattern_scale = 0.40;

    t.components.button = ComponentStyle::Soft;
    t.components.input = ComponentStyle::Soft;
    t.components.card = ComponentStyle::Soft;
    t.components.translucency = 0.60;
    t.components.density = 18;

    t.layout.corners = Corners::Squircle;
    t.layout.motion = MotionStyle::Smooth;
    t.layout.label_case = LabelCase::AsWritten;
}

// ---------------------------------------------------------------------------
// Dark themes

/// Deep teal water: waves under a cold glass panel.
fn preset_ocean(t: &mut Tokens) {
    let p = &mut t.palette;
    p.accent = Color::rgb(95, 215, 207);
    p.accent_hover = Color::rgb(120, 228, 220);
    p.on_accent = Color::rgb(4, 32, 31);
    p.surface = Color::rgba(14, 35, 39, 0.74);
    p.surface_raised = Color::rgba(20, 50, 56, 0.88);
    p.surface_sunken = Color::rgba(7, 22, 26, 0.92);
    p.text = Color::rgb(227, 248, 247);
    p.text_muted = Color::rgb(158, 196, 198);
    p.border = Color::rgba(95, 215, 207, 0.16);
    p.success = Color::rgb(111, 224, 174);
    p.warning = Color::rgb(255, 208, 122);
    p.danger = Color::rgb(255, 143, 155);
    p.shadow = Color::rgba(0, 0, 0, 0.60);
    p.elevation = 0.40;

    t.shape.radius_sm = 16;
    t.shape.radius = 24;
    t.shape.radius_lg = 32;
    t.shape.radius_pill = 999;
    t.shape.border_width = 1;
    t.shape.ring_width = 3;

    t.typography.family = FontFamily::System;
    t.typography.display_family = FontFamily::System;
    t.typography.scale = 1.0;
    t.typography.letter_spacing = 0.01;
    t.typography.line_height = 1.55;
    t.typography.uppercase_labels = false;

    t.motion.duration_fast = 170;
    t.motion.duration = 290;
    t.motion.duration_slow = 580;
    t.motion.easing = Easing::Standard;
    t.motion.ambient = true;

    t.background.kind = BackgroundKind::Radial;
    t.background.colors = vec![
        hsl(164.0, 0.50, 0.09),
        hsl(174.0, 0.45, 0.15),
        hsl(154.0, 0.42, 0.05),
    ];
    t.background.angle = 0.0;
    t.background.artwork = Artwork::Waves;
    t.background.artwork_strength = 0.30;

    t.decor.grain = 0.05;
    t.decor.vignette = 0.35;
    t.decor.glow = 0.25;
    t.decor.aurora = 0.38;
    t.decor.rim = 0.30;
    t.decor.pattern = Pattern::None;

    t.components.button = ComponentStyle::Glass;
    t.components.input = ComponentStyle::Soft;
    t.components.card = ComponentStyle::Glass;
    t.components.translucency = 0.55;
    t.components.density = 18;

    t.layout.corners = Corners::Squircle;
    t.layout.motion = MotionStyle::Smooth;
    t.layout.label_case = LabelCase::Title;
}

/// Navy velvet with a star field and a serif voice.
fn preset_midnight(t: &mut Tokens) {
    let p = &mut t.palette;
    p.accent = Color::rgb(157, 196, 255);
    p.accent_hover = Color::rgb(186, 214, 255);
    p.on_accent = Color::rgb(10, 16, 32);
    p.surface = Color::rgba(22, 28, 46, 0.74);
    p.surface_raised = Color::rgba(31, 39, 62, 0.88);
    p.surface_sunken = Color::rgba(13, 17, 30, 0.92);
    p.text = Color::rgb(233, 238, 252);
    p.text_muted = Color::rgb(166, 178, 205);
    p.border = Color::rgba(157, 196, 255, 0.16);
    p.success = Color::rgb(111, 214, 160);
    p.warning = Color::rgb(255, 199, 102);
    p.danger = Color::rgb(255, 138, 148);
    p.shadow = Color::rgba(0, 0, 0, 0.55);
    p.elevation = 0.40;

    t.shape.radius_sm = 18;
    t.shape.radius = 26;
    t.shape.radius_lg = 36;
    t.shape.radius_pill = 999;
    t.shape.border_width = 1;
    t.shape.ring_width = 3;

    t.typography.family = FontFamily::Palatino;
    t.typography.display_family = FontFamily::Palatino;
    t.typography.scale = 1.02;
    t.typography.letter_spacing = 0.01;
    t.typography.line_height = 1.6;
    t.typography.uppercase_labels = false;

    t.motion.duration_fast = 180;
    t.motion.duration = 300;
    t.motion.duration_slow = 620;
    t.motion.easing = Easing::Standard;
    t.motion.ambient = true;

    t.background.kind = BackgroundKind::Radial;
    t.background.colors = vec![
        hsl(227.0, 0.45, 0.11),
        hsl(232.0, 0.40, 0.17),
        hsl(222.0, 0.38, 0.07),
    ];
    t.background.angle = 0.0;
    t.background.artwork = Artwork::Sunburst;
    t.background.artwork_strength = 0.16;

    t.decor.grain = 0.06;
    t.decor.vignette = 0.42;
    t.decor.glow = 0.18;
    t.decor.aurora = 0.15;
    t.decor.rim = 0.30;
    t.decor.pattern = Pattern::Stars;
    t.decor.pattern_scale = 0.50;

    t.components.button = ComponentStyle::Soft;
    t.components.input = ComponentStyle::Soft;
    t.components.card = ComponentStyle::Glass;
    t.components.translucency = 0.55;
    t.components.density = 18;

    t.layout.corners = Corners::Squircle;
    t.layout.motion = MotionStyle::Smooth;
    t.layout.label_case = LabelCase::Title;
}

/// Violet nebula with a star field, set in monospace.
fn preset_cosmos(t: &mut Tokens) {
    let p = &mut t.palette;
    p.accent = Color::rgb(201, 167, 255);
    p.accent_hover = Color::rgb(219, 194, 255);
    p.on_accent = Color::rgb(23, 10, 40);
    p.surface = Color::rgba(28, 22, 46, 0.72);
    p.surface_raised = Color::rgba(39, 31, 62, 0.86);
    p.surface_sunken = Color::rgba(16, 12, 28, 0.92);
    p.text = Color::rgb(240, 234, 255);
    p.text_muted = Color::rgb(182, 171, 205);
    p.border = Color::rgba(201, 167, 255, 0.18);
    p.success = Color::rgb(134, 227, 180);
    p.warning = Color::rgb(255, 210, 122);
    p.danger = Color::rgb(255, 143, 166);
    p.shadow = Color::rgba(0, 0, 0, 0.60);
    p.elevation = 0.45;

    t.shape.radius_sm = 14;
    t.shape.radius = 20;
    t.shape.radius_lg = 28;
    t.shape.radius_pill = 999;
    t.shape.border_width = 1;
    t.shape.ring_width = 3;

    t.typography.family = FontFamily::Cascadia;
    t.typography.display_family = FontFamily::Cascadia;
    t.typography.scale = 0.98;
    t.typography.letter_spacing = 0.02;
    t.typography.line_height = 1.5;
    t.typography.uppercase_labels = false;

    t.motion.duration_fast = 200;
    t.motion.duration = 340;
    t.motion.duration_slow = 720;
    t.motion.easing = Easing::Emphasized;
    t.motion.ambient = true;

    t.background.kind = BackgroundKind::Aurora;
    t.background.colors = vec![
        hsl(254.0, 0.50, 0.10),
        hsl(268.0, 0.44, 0.16),
        hsl(240.0, 0.42, 0.06),
        hsl(288.0, 0.40, 0.13),
    ];
    t.background.angle = 110.0;
    t.background.artwork = Artwork::Bubbles;
    t.background.artwork_strength = 0.22;

    t.decor.grain = 0.08;
    t.decor.vignette = 0.45;
    t.decor.glow = 0.35;
    t.decor.aurora = 0.80;
    t.decor.rim = 0.40;
    t.decor.pattern = Pattern::Stars;
    t.decor.pattern_scale = 0.35;

    t.components.button = ComponentStyle::Glass;
    t.components.input = ComponentStyle::Soft;
    t.components.card = ComponentStyle::Glass;
    t.components.translucency = 0.50;
    t.components.density = 17;

    t.layout.corners = Corners::Rounded;
    t.layout.motion = MotionStyle::Springy;
    t.layout.label_case = LabelCase::Title;
}

/// Pine depth: dark green conic light, topographic lines and a classical face.
fn preset_forest(t: &mut Tokens) {
    let p = &mut t.palette;
    p.accent = Color::rgb(127, 214, 164);
    p.accent_hover = Color::rgb(150, 228, 184);
    p.on_accent = Color::rgb(6, 24, 13);
    p.surface = Color::rgba(18, 33, 24, 0.74);
    p.surface_raised = Color::rgba(26, 46, 34, 0.88);
    p.surface_sunken = Color::rgba(10, 20, 15, 0.92);
    p.text = Color::rgb(231, 246, 236);
    p.text_muted = Color::rgb(170, 197, 180);
    p.border = Color::rgba(127, 214, 164, 0.16);
    p.success = Color::rgb(134, 224, 174);
    p.warning = Color::rgb(255, 207, 125);
    p.danger = Color::rgb(255, 145, 153);
    p.shadow = Color::rgba(0, 0, 0, 0.60);
    p.elevation = 0.40;

    t.shape.radius_sm = 16;
    t.shape.radius = 22;
    t.shape.radius_lg = 30;
    t.shape.radius_pill = 999;
    t.shape.border_width = 1;
    t.shape.ring_width = 3;

    t.typography.family = FontFamily::Times;
    t.typography.display_family = FontFamily::Times;
    t.typography.scale = 1.0;
    t.typography.letter_spacing = 0.0;
    t.typography.line_height = 1.55;
    t.typography.uppercase_labels = false;

    t.motion.duration_fast = 200;
    t.motion.duration = 340;
    t.motion.duration_slow = 700;
    t.motion.easing = Easing::Standard;
    t.motion.ambient = true;

    t.background.kind = BackgroundKind::Conic;
    t.background.colors = vec![
        hsl(137.0, 0.38, 0.09),
        hsl(147.0, 0.34, 0.15),
        hsl(127.0, 0.32, 0.06),
    ];
    t.background.angle = 240.0;
    t.background.artwork = Artwork::Contours;
    t.background.artwork_strength = 0.26;

    t.decor.grain = 0.10;
    t.decor.vignette = 0.40;
    t.decor.glow = 0.15;
    t.decor.aurora = 0.30;
    t.decor.rim = 0.30;
    t.decor.pattern = Pattern::Topography;
    t.decor.pattern_scale = 0.45;

    t.components.button = ComponentStyle::Glass;
    t.components.input = ComponentStyle::Soft;
    t.components.card = ComponentStyle::Glass;
    t.components.translucency = 0.56;
    t.components.density = 18;

    t.layout.corners = Corners::Rounded;
    t.layout.motion = MotionStyle::Smooth;
    t.layout.label_case = LabelCase::Title;
    t.layout.placement = Placement::Left;
}

/// Dark wood and a low amber fire: the warmest of the dark presets.
fn preset_ember(t: &mut Tokens) {
    let p = &mut t.palette;
    p.accent = Color::rgb(255, 178, 107);
    p.accent_hover = Color::rgb(255, 197, 138);
    p.on_accent = Color::rgb(36, 16, 0);
    p.surface = Color::rgba(42, 25, 20, 0.76);
    p.surface_raised = Color::rgba(58, 35, 27, 0.88);
    p.surface_sunken = Color::rgba(26, 15, 12, 0.92);
    p.text = Color::rgb(255, 238, 227);
    p.text_muted = Color::rgb(204, 168, 150);
    p.border = Color::rgba(255, 178, 107, 0.18);
    p.success = Color::rgb(126, 214, 163);
    p.warning = Color::rgb(255, 199, 107);
    p.danger = Color::rgb(255, 143, 143);
    p.shadow = Color::rgba(0, 0, 0, 0.60);
    p.elevation = 0.45;

    t.shape.radius_sm = 10;
    t.shape.radius = 14;
    t.shape.radius_lg = 20;
    t.shape.radius_pill = 999;
    t.shape.border_width = 1;
    t.shape.ring_width = 3;

    t.typography.family = FontFamily::Verdana;
    t.typography.display_family = FontFamily::Verdana;
    t.typography.scale = 0.99;
    t.typography.letter_spacing = 0.0;
    t.typography.line_height = 1.52;
    t.typography.uppercase_labels = false;

    t.motion.duration_fast = 140;
    t.motion.duration = 220;
    t.motion.duration_slow = 420;
    t.motion.easing = Easing::Decelerate;
    t.motion.ambient = true;

    t.background.kind = BackgroundKind::Conic;
    t.background.colors = vec![
        hsl(2.0, 0.55, 0.10),
        hsl(20.0, 0.50, 0.16),
        hsl(350.0, 0.44, 0.07),
    ];
    t.background.angle = 35.0;
    t.background.artwork = Artwork::Halftone;
    t.background.artwork_strength = 0.22;

    t.decor.grain = 0.09;
    t.decor.vignette = 0.42;
    t.decor.glow = 0.25;
    t.decor.aurora = 0.20;
    t.decor.rim = 0.35;
    t.decor.pattern = Pattern::None;

    t.components.button = ComponentStyle::Soft;
    t.components.input = ComponentStyle::Solid;
    t.components.card = ComponentStyle::Soft;
    t.components.translucency = 0.62;
    t.components.density = 18;

    t.layout.corners = Corners::Rounded;
    t.layout.motion = MotionStyle::Brisk;
    t.layout.label_case = LabelCase::Title;
    t.layout.placement = Placement::Left;
}

/// Wine velvet: a solid, near-square dark preset for contrast with the rest.
fn preset_rose(t: &mut Tokens) {
    let p = &mut t.palette;
    p.accent = Color::rgb(255, 157, 181);
    p.accent_hover = Color::rgb(255, 184, 203);
    p.on_accent = Color::rgb(42, 7, 18);
    p.surface = Color::rgba(44, 19, 29, 0.76);
    p.surface_raised = Color::rgba(60, 27, 40, 0.88);
    p.surface_sunken = Color::rgba(28, 11, 18, 0.92);
    p.text = Color::rgb(255, 238, 243);
    p.text_muted = Color::rgb(214, 170, 185);
    p.border = Color::rgba(255, 157, 181, 0.16);
    p.success = Color::rgb(121, 214, 165);
    p.warning = Color::rgb(255, 203, 122);
    p.danger = Color::rgb(255, 143, 157);
    p.shadow = Color::rgba(0, 0, 0, 0.55);
    p.elevation = 0.40;

    t.shape.radius_sm = 12;
    t.shape.radius = 16;
    t.shape.radius_lg = 22;
    t.shape.radius_pill = 999;
    t.shape.border_width = 1;
    t.shape.ring_width = 3;

    t.typography.family = FontFamily::Georgia;
    t.typography.display_family = FontFamily::Georgia;
    t.typography.scale = 1.02;
    t.typography.letter_spacing = 0.0;
    t.typography.line_height = 1.55;
    t.typography.uppercase_labels = false;

    t.motion.duration_fast = 160;
    t.motion.duration = 260;
    t.motion.duration_slow = 500;
    t.motion.easing = Easing::Standard;
    t.motion.ambient = true;

    t.background.kind = BackgroundKind::Solid;
    t.background.colors = vec![hsl(335.0, 0.46, 0.09), hsl(345.0, 0.40, 0.05)];
    t.background.angle = 0.0;
    t.background.artwork = Artwork::Sunburst;
    t.background.artwork_strength = 0.20;

    t.decor.grain = 0.07;
    t.decor.vignette = 0.40;
    t.decor.glow = 0.20;
    t.decor.aurora = 0.18;
    t.decor.rim = 0.35;
    t.decor.pattern = Pattern::None;

    t.components.button = ComponentStyle::Glass;
    t.components.input = ComponentStyle::Soft;
    t.components.card = ComponentStyle::Glass;
    t.components.translucency = 0.58;
    t.components.density = 17;

    t.layout.corners = Corners::Rounded;
    t.layout.motion = MotionStyle::Smooth;
    t.layout.label_case = LabelCase::Title;
    t.layout.placement = Placement::Banner;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::validate::validate_theme;

    #[test]
    fn every_builtin_theme_passes_validation() {
        let mut all = builtin_themes();
        assert!(all.len() >= 12, "the gallery should offer a real choice");
        for theme in &mut all {
            let report = validate_theme(theme);
            assert!(
                report.errors.is_empty(),
                "builtin {:?} has errors: {:?}",
                theme.meta.id,
                report.errors
            );
            // Built-ins are authored by us; they should not need repair.
            assert!(
                report.fixes.is_empty(),
                "builtin {:?} needed fixes: {:?}",
                theme.meta.id,
                report.fixes
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
        assert!(all.len() >= 12, "the preset set must not shrink into near-duplicates");

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
