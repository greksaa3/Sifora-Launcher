//! Offline theme composer.
//!
//! This is the default (and free) "AI": a deterministic generator that reads
//! the user's description, extracts intent from the design-system vocabulary and
//! composes a complete, internally consistent theme. It runs with zero network
//! access, so a theme can always be produced.
//!
//! It is a *composer*, not a template gallery. Given
//! `"тёмная фиолетовая тема со стеклянными панелями, зелёным акцентом и сеткой"`
//! it will pick purple surfaces, a green accent, glass components and a grid
//! pattern — a combination that is not present in any preset.
//!
//! The optional remote model in [`super::provider`] produces themes in exactly
//! the same schema and goes through exactly the same validation, so switching
//! providers changes *how good* a theme is, never *what a theme may do*.

use super::color::{hsla, hsl, Color};
use super::defaults;
use super::lexicon::*;
use super::model::*;

/// Everything the prompt can steer.
#[derive(Debug, Clone)]
pub struct Intent {
    pub accent_hues: Vec<f32>,
    pub accent_sat: f32,
    pub lightness_shift: f32,
    pub contrast_shift: f32,
    pub radius_bias: f32,
    pub border_bias: f32,
    pub glass: bool,
    pub glass_bias: f32,
    pub bold: f32,
    pub spacing: f32,
    pub scale: f32,
    pub uppercase: bool,
    pub family: Option<FontFamily>,
    pub display_family: Option<FontFamily>,
    pub bg_kind: Option<BackgroundKind>,
    pub pattern: Option<Pattern>,
    pub aurora: Option<f32>,
    pub grain: Option<f32>,
    pub vignette: Option<f32>,
    pub glow: Option<f32>,
    pub scanlines: bool,
    pub speed: f32,
    pub easing: Option<Easing>,
    pub ambient: Option<bool>,
    pub component_style: Option<ComponentStyle>,
    /// Explicit "make it light" flag (as opposed to the general lightness shift).
    pub light_mode: Option<bool>,
    pub dark_mode: Option<bool>,
}

impl Default for Intent {
    fn default() -> Self {
        Intent {
            accent_hues: Vec::new(),
            accent_sat: 0.8,
            lightness_shift: 0.0,
            contrast_shift: 0.0,
            radius_bias: 0.0,
            border_bias: 0.0,
            glass: false,
            glass_bias: 0.0,
            bold: 0.0,
            spacing: 0.0,
            scale: 0.0,
            uppercase: false,
            family: None,
            display_family: None,
            bg_kind: None,
            pattern: None,
            aurora: None,
            grain: None,
            vignette: None,
            glow: None,
            scanlines: false,
            speed: 0.0,
            easing: None,
            ambient: None,
            component_style: None,
            light_mode: None,
            dark_mode: None,
        }
    }
}

/// Lower-cased prompt with simple morphological folding so Russian inflections
/// still match the lexicon stems.
fn normalise(input: &str) -> String {
    let mut s = input.to_lowercase();
    // Strip anything outside letters/digits/space so markup in a prompt can
    // never become meaningful (and cannot blow up matching).
    s = s
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' { c } else { ' ' })
        .collect();
    // Collapse spaces.
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        if ch == ' ' {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            prev_space = false;
            out.push(ch);
        }
    }
    out
}

fn contains(haystack: &str, needle: &str) -> bool {
    !needle.is_empty() && haystack.contains(needle)
}

/// Stable 32-bit FNV-1a hash used for deterministic variety.
fn hash32(text: &str) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in text.as_bytes() {
        h ^= *b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

pub fn parse_intent(prompt: &str) -> Intent {
    let text = normalise(prompt);
    let mut it = Intent::default();
    let mut noise = hash32(&text);

    let mut next_random = || {
        noise ^= noise << 13;
        noise ^= noise >> 17;
        noise ^= noise << 5;
        (noise as f32) / (u32::MAX as f32)
    };

    // --- colours -----------------------------------------------------------
    // Hues are recorded *in the order they appear in the sentence*, because
    // that order carries meaning: "фиолетовый фон с зелёным акцентом" names
    // the background first and the accent second.
    let mut hits: Vec<(usize, f32, f32)> = Vec::new();
    for word in HUE_WORDS {
        for needle in word.words {
            if let Some(pos) = text.find(needle) {
                if hits.iter().any(|(_, h, _)| (h - word.hue).abs() < 4.0) {
                    continue;
                }
                hits.push((pos, word.hue, word.sat));
            }
        }
    }
    hits.sort_by_key(|(pos, _, _)| *pos);
    for (_, hue, sat) in &hits {
        it.accent_hues.push(*hue);
        it.accent_sat = it.accent_sat.max(*sat);
    }

    // --- mood --------------------------------------------------------------
    for word in MOOD_WORDS {
        for needle in word.words {
            if contains(&text, needle) {
                it.lightness_shift += word.lightness;
                it.accent_sat = (it.accent_sat + word.saturation).clamp(0.05, 1.0);
                it.contrast_shift += word.contrast;
            }
        }
    }
    if contains(&text, "светл") || contains(&text, "light mode") || contains(&text, "дневн") {
        it.light_mode = Some(true);
    }
    if contains(&text, "тёмн") || contains(&text, "темн") || contains(&text, "dark") || contains(&text, "ночн") {
        it.dark_mode = Some(true);
    }
    if it.light_mode == Some(true) && it.dark_mode == Some(true) {
        // Contradictory wording: prefer the explicit "light" reading.
        it.dark_mode = None;
    }

    // --- shape -------------------------------------------------------------
    for word in SHAPE_WORDS {
        for needle in word.words {
            if contains(&text, needle) {
                it.radius_bias += word.radius;
                it.border_bias += word.border;
                if word.glass {
                    it.glass = true;
                    it.glass_bias += 0.25;
                }
            }
        }
    }
    it.radius_bias = it.radius_bias.clamp(-1.0, 1.5);
    it.border_bias = it.border_bias.clamp(-1.0, 2.0);

    // --- typography --------------------------------------------------------
    for word in TYPE_WORDS {
        for needle in word.words {
            if contains(&text, needle) {
                if let Some(f) = word.family {
                    it.family.get_or_insert(font_from_id(f));
                }
                it.bold += word.bold;
                it.spacing += word.spacing;
                it.scale += word.scale;
                it.uppercase |= word.uppercase;
            }
        }
    }
    it.bold = it.bold.clamp(-0.2, 0.6);
    it.spacing = it.spacing.clamp(-0.03, 0.16);
    it.scale = it.scale.clamp(-0.12, 0.2);

    // --- background + decor ------------------------------------------------
    for word in DECOR_WORDS {
        for needle in word.words {
            if contains(&text, needle) {
                if let Some(bg) = word.kind {
                    it.bg_kind.get_or_insert(match bg.0 {
                        1 => BackgroundKind::Linear,
                        2 => BackgroundKind::Radial,
                        3 => BackgroundKind::Conic,
                        4 => BackgroundKind::Mesh,
                        5 => BackgroundKind::Aurora,
                        _ => BackgroundKind::Image,
                    });
                }
                if let Some(p) = word.pattern {
                    it.pattern.get_or_insert(match p.0 {
                        1 => Pattern::Dots,
                        2 => Pattern::Grid,
                        3 => Pattern::Noise,
                        4 => Pattern::Diagonal,
                        5 => Pattern::Stars,
                        _ => Pattern::Topography,
                    });
                }
                if let Some(v) = word.aurora {
                    it.aurora = Some(it.aurora.unwrap_or(0.5).max(v));
                }
                if let Some(v) = word.grain {
                    it.grain = Some(it.grain.unwrap_or(0.05).max(v));
                }
                if let Some(v) = word.vignette {
                    it.vignette = Some(it.vignette.unwrap_or(0.3).max(v));
                }
                if let Some(v) = word.glow {
                    it.glow = Some(it.glow.unwrap_or(0.4).max(v));
                }
                it.scanlines |= word.scanlines;
            }
        }
    }

    // --- motion ------------------------------------------------------------
    for word in MOTION_WORDS {
        for needle in word.words {
            if contains(&text, needle) {
                if let Some(s) = word.speed {
                    it.speed = it.speed + s;
                }
                if let Some(e) = word.easing {
                    it.easing.get_or_insert(easing_from_id(e));
                }
                if let Some(a) = word.ambient {
                    it.ambient = Some(a);
                }
            }
        }
    }
    it.speed = it.speed.clamp(-0.5, 1.0);

    // --- component styling --------------------------------------------------
    for word in COMPONENT_WORDS {
        for needle in word.words {
            if contains(&text, needle) {
                if let Some(s) = word.style {
                    it.component_style.get_or_insert(component_style_from_id(s));
                }
            }
        }
    }

    // --- explicit hex colours ----------------------------------------------
    // If the user pasted `#7c5cff`, honour it as the accent.
    for token in text.split_whitespace() {
        if let Some(c) = Color::parse(token) {
            let (h, _, _) = c.hsl();
            if it.accent_hues.is_empty() {
                it.accent_hues.push(h);
                it.accent_sat = 0.85;
            }
        }
    }
    // Hex glued to punctuation is handled by the character filter above.

    // Deterministic variety for unset dimensions.
    //
    // Two different prompts must not collapse into the same theme, so anything
    // the description left open is chosen from a stable hash of the prompt
    // rather than from a single default. This is what makes the composer feel
    // like it is composing instead of filling in blanks.
    if it.accent_hues.is_empty() {
        it.accent_hues.push((next_random() * 360.0).floor());
        it.accent_sat = 0.55 + next_random() * 0.3;
    }
    if it.bg_kind.is_none() {
        const KINDS: [BackgroundKind; 5] = [
            BackgroundKind::Linear,
            BackgroundKind::Radial,
            BackgroundKind::Mesh,
            BackgroundKind::Aurora,
            BackgroundKind::Conic,
        ];
        it.bg_kind = Some(KINDS[(next_random() * KINDS.len() as f32) as usize % KINDS.len()]);
    }
    if it.pattern.is_none() {
        const PATTERNS: [Pattern; 5] =
            [Pattern::None, Pattern::Dots, Pattern::Grid, Pattern::Noise, Pattern::Topography];
        // A pattern on every theme would be noisy, so most stay clean.
        let index = (next_random() * 8.0) as usize;
        it.pattern = Some(if index < 5 { Pattern::None } else { PATTERNS[index % PATTERNS.len()] });
    }
    if it.component_style.is_none() {
        const STYLES: [ComponentStyle; 4] = [
            ComponentStyle::Soft,
            ComponentStyle::Solid,
            ComponentStyle::Glass,
            ComponentStyle::Outline,
        ];
        it.component_style = Some(STYLES[(next_random() * STYLES.len() as f32) as usize % STYLES.len()]);
    }
    if it.family.is_none() {
        const FAMILIES: [FontFamily; 4] =
            [FontFamily::System, FontFamily::Segoe, FontFamily::Trebuchet, FontFamily::Georgia];
        it.family = Some(FAMILIES[(next_random() * FAMILIES.len() as f32) as usize % FAMILIES.len()]);
    }
    it.radius_bias += next_random() * 0.8 - 0.4;
    it
}

/// Harmony offset for the background gradient relative to the accent.
fn harmony(seed: u32, hue_count: usize) -> f32 {
    match hue_count {
        0 | 1 => [30.0, 46.0, 58.0, -34.0, 72.0][(seed % 5) as usize],
        2 => [24.0, -22.0, 36.0, -30.0, 18.0][(seed % 5) as usize],
        _ => [150.0, 120.0, 180.0, -150.0, 138.0][(seed % 5) as usize],
    }
}

const NAMES: &[&str] = &[
    "Аврора", "Полярный свет", "Мягкий градиент", "Тихий сигнал", "Стеклянный сад", "Ночная гавань",
    "Мятный прилив", "Тёплый пепел", "Глубокий акварель", "Контур", "Полдень", "Северное сияние",
    "Линза", "Пыльный свет", "Гравитация", "Спокойное море", "Неоновая линия", "Бархат", "Контур света",
    "Графитовый дождь",
];

const ADJECTIVES: &[&str] = &[
    "", "Мягкий ", "Глубокий ", "Тихий ", "Яркий ", "Плотный ", "Лёгкий ", "Тёплый ", "Холодный ", "Неоновый ",
];

/// Compose a full theme from a free-form description.
pub fn compose(prompt: &str) -> ThemeSpec {
    let intent = parse_intent(prompt);
    let seed = hash32(&normalise(prompt));

    // Hue order: "a purple background with a green accent" reads
    // background-first, so the *last* named hue becomes the accent and the
    // first one colours the background family.
    let accent_hue = *intent.accent_hues.last().unwrap_or(&210.0);
    let backdrop_hue = intent.accent_hues.first().copied().unwrap_or(accent_hue);
    let tertiary_hue = intent
        .accent_hues
        .iter()
        .rev()
        .nth(1)
        .copied()
        .map(|h| (h + 150.0).rem_euclid(360.0))
        .unwrap_or_else(|| backdrop_hue + harmony(seed, intent.accent_hues.len()));

    let light = intent.light_mode.unwrap_or(false);
    let dark = intent.dark_mode.unwrap_or(!light && intent.lightness_shift <= 0.0);

    let sat = intent.accent_sat.clamp(0.05, 1.0);
    let accent_light = if dark { 0.66 } else { 0.48 };

    let mut tokens = defaults::default_tokens();
    let p = &mut tokens.palette;

    p.accent = hsl(accent_hue, sat, accent_light);
    p.accent_hover = hsl(accent_hue + 4.0, (sat + 0.04).min(1.0), (accent_light + 0.07).clamp(0.2, 0.9));
    p.success = hsl(152.0, 0.62, if dark { 0.66 } else { 0.44 });
    p.warning = hsl(38.0, 0.84, if dark { 0.68 } else { 0.48 });
    p.danger = hsl(356.0, 0.72, if dark { 0.66 } else { 0.50 });

    // Background family: a tinted near-black or a tinted near-white.
    let bg_light = if light { 0.90 } else { 0.08 };
    let shift = intent.lightness_shift.clamp(-0.3, 0.3);
    let bg_base_l = (bg_light + shift * 0.5).clamp(0.04, 0.94);

    let bg1 = hsl(backdrop_hue, sat * 0.55, bg_base_l.clamp(0.04, 0.2));
    let bg2 = hsl(
        backdrop_hue + harmony(seed, intent.accent_hues.len()),
        sat * 0.45,
        (bg_base_l + if dark { 0.12 } else { -0.06 }).clamp(0.05, 0.6),
    );
    let bg3 = hsl(tertiary_hue, sat * 0.38, (bg_base_l + if dark { 0.07 } else { -0.10 }).clamp(0.03, 0.7));

    p.surface = if dark {
        hsla(backdrop_hue, 0.20, 0.13, 0.66)
    } else {
        hsla(backdrop_hue, 0.18, 0.99, 0.80)
    };
    p.surface_raised = if dark {
        hsla(backdrop_hue, 0.18, 0.17, 0.74)
    } else {
        hsla(backdrop_hue, 0.10, 1.0, 0.94)
    };
    p.surface_sunken = if dark {
        hsla(backdrop_hue, 0.24, 0.08, 0.68)
    } else {
        hsla(backdrop_hue, 0.20, 0.93, 0.86)
    };
    p.text = if dark { hsl(backdrop_hue, 0.12, 0.96) } else { hsl(backdrop_hue, 0.35, 0.12) };
    p.text_muted = if dark {
        hsl(backdrop_hue, 0.10, 0.72)
    } else {
        hsl(backdrop_hue, 0.14, 0.42)
    };
    p.border = if dark { Color::rgba(255, 255, 255, 0.10) } else { Color::rgba(20, 20, 30, 0.10) };
    p.shadow = if dark { Color::rgba(0, 0, 0, 0.55) } else { Color::rgba(50, 44, 38, 0.20) };
    p.on_accent = p.accent.best_text_contrast();
    p.elevation = (0.5 + intent.contrast_shift).clamp(0.15, 0.9);

    // --- shape -------------------------------------------------------------
    let radius_base = (10.0 + intent.radius_bias * 12.0).clamp(0.0, 34.0);
    tokens.shape.radius_sm = radius_base.round() as u16;
    tokens.shape.radius = (radius_base + 5.0).round() as u16;
    tokens.shape.radius_lg = (radius_base + 12.0).round() as u16;
    tokens.shape.radius_pill = if radius_base > 24.0 { 999 } else { 999 };
    tokens.shape.border_width = (1.0 + intent.border_bias).clamp(0.0, 4.0).round() as u16;
    tokens.shape.ring_width = 3;

    // --- typography --------------------------------------------------------
    tokens.typography.family = intent.family.unwrap_or(FontFamily::System);
    tokens.typography.display_family =
        intent.display_family.or(intent.family).unwrap_or(FontFamily::System);
    tokens.typography.scale = (1.0 + intent.scale).clamp(0.85, 1.3);
    tokens.typography.weight_regular = 400;
    tokens.typography.weight_bold = (650.0 + intent.bold * 200.0).clamp(500.0, 900.0).round() as u16;
    tokens.typography.letter_spacing = intent.spacing.clamp(-0.03, 0.16);
    tokens.typography.uppercase_labels = intent.uppercase;

    // --- motion ------------------------------------------------------------
    let speed_factor = 1.0 + intent.speed;
    tokens.motion.duration_fast = (140.0 * speed_factor).clamp(60.0, 600.0).round() as u16;
    tokens.motion.duration = (240.0 * speed_factor).clamp(90.0, 1400.0).round() as u16;
    tokens.motion.duration_slow = (520.0 * speed_factor).clamp(200.0, 3000.0).round() as u16;
    tokens.motion.easing = intent.easing.unwrap_or(if intent.glass { Easing::Spring } else { Easing::Standard });
    tokens.motion.ambient = intent.ambient.unwrap_or(true);
    tokens.motion.hover_intensity = 1.0;

    // --- decor -------------------------------------------------------------
    tokens.decor.grain = intent.grain.unwrap_or(if light { 0.08 } else { 0.05 });
    tokens.decor.vignette = intent.vignette.unwrap_or(if light { 0.16 } else { 0.36 });
    tokens.decor.glow = intent.glow.unwrap_or(if dark { 0.45 } else { 0.25 });
    tokens.decor.aurora = intent.aurora.unwrap_or(if dark { 0.5 } else { 0.22 });
    tokens.decor.rim = if intent.glass { 0.55 } else { 0.22 };
    tokens.decor.pattern = intent.pattern.unwrap_or(Pattern::None);
    tokens.decor.pattern_scale = 0.5;
    tokens.decor.scanlines = intent.scanlines;

    // --- background --------------------------------------------------------
    // When the description did not pick a kind, `parse_intent` already chose
    // one; here it is only biased towards the ones that suit light vs dark.
    tokens.background.kind = intent.bg_kind.unwrap_or(if dark {
        BackgroundKind::Aurora
    } else {
        BackgroundKind::Mesh
    });
    tokens.background.colors =
        vec![bg1, bg2, bg3, hsl(backdrop_hue, sat * 0.3, (bg_base_l * 0.6).clamp(0.02, 0.5))];
    // The gradient angle rotates with the prompt so two themes built from the
    // same hue do not line up identically.
    tokens.background.angle = {
        const ANGLES: [f32; 6] = [145.0, 165.0, 200.0, 25.0, 90.0, 215.0];
        ANGLES[((seed / 5) as usize) % ANGLES.len()]
    };
    tokens.background.dim = if dark { 0.0 } else { -0.02 };
    tokens.background.image = None;

    // --- components --------------------------------------------------------
    let style = intent.component_style.unwrap_or(if intent.glass { ComponentStyle::Glass } else { ComponentStyle::Soft });
    tokens.components.button = if matches!(style, ComponentStyle::Glass) { ComponentStyle::Solid } else { style };
    tokens.components.input = ComponentStyle::Soft;
    tokens.components.card = if intent.glass { ComponentStyle::Glass } else { ComponentStyle::Soft };
    tokens.components.translucency = if intent.glass { 0.42 } else { 0.70 };
    tokens.components.density = 16;

    let mut spec = ThemeSpec {
        meta: ThemeMeta {
            id: defaults::new_id(),
            name: String::new(),
            author: "sifora:local".into(),
            description: String::new(),
            prompt: Some(crate::theme::model::sanitize_text(prompt, 1200, "")),
            created_at: crate::util::now_rfc3339(),
            builtin: false,
            schema: THEME_SCHEMA,
        },
        tokens,
    };
    spec.clamp_in_place();
    super::validate::validate_theme(&mut spec);
    spec.meta.name = compose_name(&intent, seed, prompt);
    spec.meta.description = compose_description(&intent);
    spec.sanitize_meta();
    spec
}

fn compose_name(intent: &Intent, seed: u32, prompt: &str) -> String {
    let adjectives: &[&str] = ADJECTIVES;
    let adj = if intent.glass {
        "Стеклянный "
    } else if matches!(intent.bg_kind, Some(BackgroundKind::Conic)) {
        "Конический "
    } else if intent.scanlines {
        "Сканлайновый "
    } else {
        adjectives[(seed % adjectives.len() as u32) as usize]
    };
    let noun = if !prompt.trim().is_empty() && (seed % 3 == 0) {
        NAMES[((seed / 7) as usize) % NAMES.len()]
    } else {
        NAMES[((seed / 13) as usize) % NAMES.len()]
    };
    format!("{adj}{noun}").trim().to_string()
}

fn compose_description(intent: &Intent) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(bg) = intent.bg_kind {
        let name = match bg {
            BackgroundKind::Solid => "однотонный фон",
            BackgroundKind::Linear => "линейный градиент",
            BackgroundKind::Radial => "радиальный градиент",
            BackgroundKind::Conic => "конический градиент",
            BackgroundKind::Mesh => "медовый градиент",
            BackgroundKind::Aurora => "аврора",
            BackgroundKind::Image => "собственное изображение",
        };
        parts.push(name.to_string());
    }
    if intent.glass {
        parts.push("стеклянные панели".into());
    }
    if let Some(p) = intent.pattern {
        if p != Pattern::None {
            let name = match p {
                Pattern::Dots => "точки",
                Pattern::Grid => "сетка",
                Pattern::Noise => "шум",
                Pattern::Diagonal => "диагонали",
                Pattern::Stars => "звёзды",
                Pattern::Topography => "топография",
                Pattern::None => "",
            };
            parts.push(format!("узор «{name}»"));
        }
    }
    if intent.scanlines {
        parts.push("сканлайны".into());
    }
    if intent.ambient == Some(false) {
        parts.push("статичный фон".into());
    }
    if parts.is_empty() {
        "Сгенерировано из вашего описания".into()
    } else {
        format!("Сгенерировано из описания: {}", parts.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::validate::validate_theme;

    #[test]
    fn short_prompt_still_produces_a_valid_theme() {
        let mut spec = compose("синий");
        assert!(validate_theme(&mut spec).is_valid());
        assert!(!spec.meta.name.is_empty());
    }

    #[test]
    fn empty_prompt_is_handled() {
        let mut spec = compose("");
        assert!(validate_theme(&mut spec).is_valid());
    }

    #[test]
    fn markup_in_the_prompt_cannot_leak_into_the_theme() {
        let hostile = "<script>alert(1)</script> красная тема";
        let mut spec = compose(hostile);
        assert!(validate_theme(&mut spec).is_valid());
        let json = serde_json::to_string(&spec).unwrap();
        assert!(!json.contains("<script"));
        assert!(!json.contains("url("));
    }

    #[test]
    fn colour_words_pick_different_accents() {
        let red = compose("красная тема");
        let blue = compose("синяя тема");
        let (rh, _, _) = red.tokens.palette.accent.hsl();
        let (bh, _, _) = blue.tokens.palette.accent.hsl();
        assert!((rh - bh).abs() > 60.0, "red {rh} blue {bh}");
    }

    #[test]
    fn multiple_colours_produce_a_harmony() {
        let spec = compose("фиолетовый фон с зелёным акцентом");
        let (accent_h, _, _) = spec.tokens.palette.accent.hsl();
        let (bg_h, _, _) = spec.tokens.background.colors[0].hsl();
        assert!(
            (accent_h - bg_h).abs() > 40.0 || (bg_h - 120.0).abs() < 40.0,
            "accent {accent_h} bg {bg_h}"
        );
    }

    #[test]
    fn glass_request_switches_component_style() {
        let spec = compose("стеклянные панели");
        assert_eq!(spec.tokens.components.card, ComponentStyle::Glass);
        assert!(spec.tokens.decor.rim > 0.4);
    }

    #[test]
    fn light_request_produces_light_surfaces() {
        let spec = compose("светлая минималистичная тема");
        assert!(spec.tokens.palette.surface_raised.is_light());
    }

    #[test]
    fn disabling_motion_is_honoured() {
        let spec = compose("тема без анимаций");
        assert!(!spec.tokens.motion.ambient);
    }

    #[test]
    fn prompt_with_only_punctuation_does_not_panic() {
        let _ = compose("!!! ??? *** ---");
        let _ = compose("   ");
        let _ = compose("🎨🌙✨");
    }

    /// The composer must not collapse: themes built from different prompts should
    /// differ on at least two identity signals, the same rule the shipped preset
    /// set is held to.
    #[test]
    fn different_prompts_produce_visibly_different_themes() {
        let prompts = [
            "красная тёмная тема",
            "синяя тёмная тема",
            "зелёная светлая тема",
            "жёлтая тёмная тема",
            "фиолетовая тёмная тема",
            "оранжевая тёмная тема",
            "мятная тема со стеклом",
            "терминальная зелёная тема",
        ];
        let themes: Vec<ThemeSpec> = prompts.iter().map(|p| compose(p)).collect();

        for (i, a) in themes.iter().enumerate() {
            for (j, b) in themes.iter().enumerate().skip(i + 1) {
                let hue_a = a.tokens.background.colors[0].hsl().0;
                let hue_b = b.tokens.background.colors[0].hsl().0;
                let raw = (hue_a - hue_b).abs();
                let hue_delta = raw.min(360.0 - raw);

                let signals = [
                    hue_delta > 25.0,
                    a.tokens.background.kind != b.tokens.background.kind,
                    (a.tokens.shape.radius as i32 - b.tokens.shape.radius as i32).abs() >= 4,
                    a.tokens.decor.pattern != b.tokens.decor.pattern,
                    a.tokens.components.button != b.tokens.components.button,
                    a.tokens.palette.surface.is_light() != b.tokens.palette.surface.is_light(),
                    a.tokens.typography.family != b.tokens.typography.family,
                ];
                let differing = signals.iter().filter(|s| **s).count();
                assert!(
                    differing >= 2,
                    "prompts {i} ({:?}) and {j} ({:?}) look too alike: {differing} differing \
                     signals, hue Δ{hue_delta:.0}°",
                    prompts[i],
                    prompts[j]
                );
            }
        }
    }

    /// Vague prompts must still land on different backgrounds, patterns and
    /// component styles rather than defaulting to one look.
    #[test]
    fn vague_prompts_vary_the_unspecified_dimensions() {
        let themes: Vec<ThemeSpec> = [
            "что-то красивое",
            "сделай уютно",
            "минимализм",
            "попробуй что-нибудь необычное",
            "для души",
            "летний вечер",
        ]
        .iter()
        .map(|p| compose(p))
        .collect();

        let count = |values: Vec<String>| -> usize { values.into_iter().collect::<std::collections::HashSet<_>>().len() };
        let kinds = count(themes.iter().map(|t| format!("{:?}", t.tokens.background.kind)).collect());
        let patterns = count(themes.iter().map(|t| format!("{:?}", t.tokens.decor.pattern)).collect());
        let styles = count(themes.iter().map(|t| format!("{:?}", t.tokens.components.button)).collect());

        assert!(kinds >= 3, "background kinds collapsed: {kinds}");
        assert!(patterns >= 2, "patterns collapsed: {patterns}");
        assert!(styles >= 3, "button styles collapsed: {styles}");
    }

    #[test]
    fn explicit_hex_colour_is_honoured() {
        let spec = compose("тема с акцентом #22cc88");
        let (h, s, _) = spec.tokens.palette.accent.hsl();
        assert!((h - 158.0).abs() < 12.0, "hue {h}");
        assert!(s > 0.4, "sat {s}");
    }

    #[test]
    fn composition_is_deterministic_for_the_same_prompt() {
        let a = compose("тёмная неоновая киберпанк тема");
        let b = compose("тёмная неоновая киберпанк тема");
        assert_eq!(a.tokens, b.tokens);
    }
}