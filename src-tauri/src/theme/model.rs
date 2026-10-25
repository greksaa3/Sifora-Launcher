//! The closed theme schema.
//!
//! Everything a theme is allowed to express lives here and nowhere else. There
//! is intentionally **no** free-form string field that reaches the CSS layer,
//! no nested key/value escape hatch, no "extra" bag and no raw animation or
//! selector syntax. A theme is a fixed shape of numbers, enums and colours.
//!
//! Consequences that matter for safety:
//!
//! * A theme can never contain JavaScript, Rust, HTML, CSS at-rule or selector
//!   text — there is no field for it.
//! * A theme can never reach the filesystem, the network, the Tauri ACL or the
//!   command layer. The renderer only maps these fields onto CSS custom
//!   properties.
//! * A theme can never *remove* functionality. Hiding a button hides markup;
//!   the underlying command stays callable, because visibility and capability
//!   are two different layers.
//!
//! Every struct uses `deny_unknown_fields`, so anything a hand-edited file or a
//! future format adds beyond the schema is a hard error instead of a silent
//! injection vector.

use super::color::Color;
use serde::{Deserialize, Serialize};

pub const THEME_SCHEMA: u32 = 1;

/// Longest free-text values we ever keep from a theme file.
const MAX_NAME: usize = 48;
const MAX_DESCRIPTION: usize = 240;
const MAX_PROMPT: usize = 1200;

// ---------------------------------------------------------------------------
// Enums (every one of them is a closed allow-list)
// ---------------------------------------------------------------------------

/// Font stacks limited to faces that ship with the operating system, so a theme
/// never triggers a webfont download or an extra resident font cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum FontFamily {
    #[default]
    System,
    Segoe,
    Georgia,
    Palatino,
    Trebuchet,
    Verdana,
    Tahoma,
    Impact,
    Consolas,
    Cascadia,
    Courier,
    Times,
}

impl FontFamily {
    pub const ALL: [FontFamily; 12] = [
        FontFamily::System,
        FontFamily::Segoe,
        FontFamily::Georgia,
        FontFamily::Palatino,
        FontFamily::Trebuchet,
        FontFamily::Verdana,
        FontFamily::Tahoma,
        FontFamily::Impact,
        FontFamily::Consolas,
        FontFamily::Cascadia,
        FontFamily::Courier,
        FontFamily::Times,
    ];

    /// The CSS value used by the renderer. Fixed strings only — the caller can
    /// never influence them.
    pub fn stack(self) -> &'static str {
        match self {
            FontFamily::System => {
                "system-ui, -apple-system, 'Segoe UI Variable Text', 'Segoe UI', sans-serif"
            }
            FontFamily::Segoe => "'Segoe UI Variable Display', 'Segoe UI', system-ui, sans-serif",
            FontFamily::Georgia => "Georgia, 'Times New Roman', serif",
            FontFamily::Palatino => "Palatino Linotype, 'Book Antiqua', Palatino, serif",
            FontFamily::Trebuchet => "'Trebuchet MS', 'Segoe UI', sans-serif",
            FontFamily::Verdana => "Verdana, Geneva, 'Segoe UI', sans-serif",
            FontFamily::Tahoma => "Tahoma, 'Segoe UI', Geneva, sans-serif",
            FontFamily::Impact => "Impact, Haettenschweiler, 'Arial Narrow Bold', sans-serif",
            FontFamily::Consolas => "Consolas, 'Cascadia Mono', 'Courier New', monospace",
            FontFamily::Cascadia => "'Cascadia Code', Consolas, 'Courier New', monospace",
            FontFamily::Courier => "'Courier New', Courier, monospace",
            FontFamily::Times => "'Times New Roman', Times, serif",
        }
    }

    pub fn is_mono(self) -> bool {
        matches!(self, FontFamily::Consolas | FontFamily::Cascadia | FontFamily::Courier)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Easing {
    #[default]
    Standard,
    Linear,
    Decelerate,
    Accelerate,
    Emphasized,
    Spring,
}

impl Easing {
    pub const ALL: [Easing; 6] = [
        Easing::Standard,
        Easing::Linear,
        Easing::Decelerate,
        Easing::Accelerate,
        Easing::Emphasized,
        Easing::Spring,
    ];

    /// Fixed cubic-bezier definitions; durations are the only tunable part.
    pub fn bezier(self) -> &'static str {
        match self {
            Easing::Standard => "cubic-bezier(0.4, 0.0, 0.2, 1)",
            Easing::Linear => "linear",
            Easing::Decelerate => "cubic-bezier(0.0, 0.0, 0.2, 1)",
            Easing::Accelerate => "cubic-bezier(0.4, 0.0, 1, 1)",
            Easing::Emphasized => "cubic-bezier(0.2, 0.0, 0, 1)",
            Easing::Spring => "cubic-bezier(0.34, 1.56, 0.64, 1)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ReducedMotion {
    /// Respect the OS-level `prefers-reduced-motion` setting.
    #[default]
    System,
    Always,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum BackgroundKind {
    #[default]
    Linear,
    Solid,
    Radial,
    Conic,
    Mesh,
    Aurora,
    Image,
}

impl BackgroundKind {
    pub const ALL: [BackgroundKind; 7] = [
        BackgroundKind::Solid,
        BackgroundKind::Linear,
        BackgroundKind::Radial,
        BackgroundKind::Conic,
        BackgroundKind::Mesh,
        BackgroundKind::Aurora,
        BackgroundKind::Image,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Pattern {
    #[default]
    None,
    Dots,
    Grid,
    Noise,
    Diagonal,
    Stars,
    Topography,
}

impl Pattern {
    pub const ALL: [Pattern; 7] = [
        Pattern::None,
        Pattern::Dots,
        Pattern::Grid,
        Pattern::Noise,
        Pattern::Diagonal,
        Pattern::Stars,
        Pattern::Topography,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ImageFit {
    #[default]
    Cover,
    Contain,
    Stretch,
    Tile,
}

impl ImageFit {
    pub const ALL: [ImageFit; 4] =
        [ImageFit::Cover, ImageFit::Contain, ImageFit::Stretch, ImageFit::Tile];
}

/// Filled / tonal / outlined / frosted — presentation only, never behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ComponentStyle {
    #[default]
    Soft,
    Solid,
    Outline,
    Glass,
}

impl ComponentStyle {
    pub const ALL: [ComponentStyle; 4] =
        [ComponentStyle::Solid, ComponentStyle::Soft, ComponentStyle::Outline, ComponentStyle::Glass];
}

/// A background image reference.
///
/// Only a file the user explicitly picked through the native file dialog is
/// accepted. Remote URLs are deliberately *not* supported: fetching them from
/// the renderer would widen the CSP and add an outbound network surface the
/// product does not need. The enum is internally tagged with unknown fields
/// rejected, so no other shape can be constructed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ImageRef {
    Local { path: String },
}

// ---------------------------------------------------------------------------
// Token groups
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Palette {
    pub accent: Color,
    pub accent_hover: Color,
    pub on_accent: Color,
    pub surface: Color,
    pub surface_raised: Color,
    pub surface_sunken: Color,
    pub text: Color,
    pub text_muted: Color,
    pub border: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    /// Colour (usually `text`) used for drop shadows.
    pub shadow: Color,
    /// How strongly panels separate from the background, 0..=1.
    pub elevation: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shape {
    pub radius_sm: u16,
    pub radius: u16,
    pub radius_lg: u16,
    pub radius_pill: u16,
    pub border_width: u16,
    pub ring_width: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Typography {
    pub family: FontFamily,
    pub display_family: FontFamily,
    /// Root font-size multiplier, 0.8 ..= 1.35.
    pub scale: f32,
    pub weight_regular: u16,
    pub weight_bold: u16,
    /// em units, -0.05 ..= 0.2.
    pub letter_spacing: f32,
    /// multiplier, 1.0 ..= 1.8.
    pub line_height: f32,
    pub uppercase_labels: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Motion {
    pub duration_fast: u16,
    pub duration: u16,
    pub duration_slow: u16,
    pub easing: Easing,
    pub reduced: ReducedMotion,
    /// Slow ambient background drift, purely decorative.
    pub ambient: bool,
    /// Hover response multiplier, 0.5 ..= 2.0 (visual only).
    pub hover_intensity: f32,
}

/// A large, hand-built backdrop drawn behind the whole window.
///
/// A palette alone reads as the same product in a different colour. These give
/// each theme a silhouette you can recognise before you read a word of text.
///
/// It is a closed enum on purpose, for the same reason the rest of the schema
/// is: a theme may *choose* artwork, never supply it. There is no path by which
/// a theme file can carry markup, a data URL or a script.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Artwork {
    /// No artwork; the gradient stands on its own.
    #[default]
    None,
    /// Comic-book sunburst radiating from the top centre.
    Sunburst,
    /// Halftone dot screen, like printed comics.
    Halftone,
    /// Chunky isometric blocks.
    Blocks,
    /// Circuit traces with pads and vias.
    Circuit,
    /// Soft overlapping waves.
    Waves,
    /// Scattered confetti shards.
    Confetti,
    /// Rising bubbles.
    Bubbles,
    /// Flowing topographic contour lines.
    Contours,
}

impl Artwork {
    pub const ALL: [Artwork; 9] = [
        Artwork::None,
        Artwork::Sunburst,
        Artwork::Halftone,
        Artwork::Blocks,
        Artwork::Circuit,
        Artwork::Waves,
        Artwork::Confetti,
        Artwork::Bubbles,
        Artwork::Contours,
    ];
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decor {
    /// Film-grain overlay strength, 0..=1.
    pub grain: f32,
    /// Corner darkening, 0..=1.
    pub vignette: f32,
    /// Accent-coloured halo around primary controls, 0..=1.
    pub glow: f32,
    /// Drifting colour blobs behind the content, 0..=1.
    pub aurora: f32,
    /// Accent rim light on panels, 0..=1.
    pub rim: f32,
    /// Size of the repeating background pattern, 0..=1.
    pub pattern_scale: f32,
    pub pattern: Pattern,
    pub scanlines: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Background {
    pub kind: BackgroundKind,
    /// 1 ..= 5 stops, always sorted by the renderer.
    pub colors: Vec<Color>,
    /// Degrees for gradient kinds, 0..=360.
    pub angle: f32,
    pub image: Option<ImageRef>,
    pub image_fit: ImageFit,
    /// 0..=1
    pub image_opacity: f32,
    /// px, 0..=40
    pub image_blur: f32,
    /// Uniform darkening/lightening applied over the background, -0.6..=0.6.
    pub dim: f32,
    /// Large hand-built backdrop drawn behind the window.
    #[serde(default)]
    pub artwork: Artwork,
    /// 0..=1 strength of the artwork.
    #[serde(default = "default_artwork_strength")]
    pub artwork_strength: f32,
}

fn default_artwork_strength() -> f32 {
    0.55
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Components {
    pub button: ComponentStyle,
    pub input: ComponentStyle,
    pub card: ComponentStyle,
    /// 0..=1 extra surface opacity applied to translucent component styles.
    pub translucency: f32,
    /// px, 0..=40 — base spacing unit that layout utilities multiply.
    pub density: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tokens {
    pub palette: Palette,
    pub shape: Shape,
    pub typography: Typography,
    pub motion: Motion,
    pub decor: Decor,
    pub background: Background,
    pub components: Components,
    pub layout: Layout,
}

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

/// A section of the launcher a theme is allowed to reorder, hide or promote.
///
/// A closed list by design. A theme may *arrange* capabilities that already
/// exist; it can never introduce one, and there is no field here that carries
/// markup, a URL or an action name invented by the theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Section {
    Appearance,
    Instances,
    Mods,
    Settings,
    About,
}

impl Section {
    pub const ALL: [Section; 5] = [
        Section::Appearance,
        Section::Instances,
        Section::Mods,
        Section::Settings,
        Section::About,
    ];
}

/// An existing capability that may be surfaced on the home screen.
///
/// Every one of these already exists in the application. This enum is a way to
/// decide *where* a feature appears, never what it does — which is what keeps
/// this an arrangement of known behaviour instead of an extension point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Shortcut {
    Appearance,
    Instances,
    Mods,
    Settings,
    Log,
    RandomTheme,
    ToggleBackground,
    Performance,
}

impl Shortcut {
    pub const ALL: [Shortcut; 8] = [
        Shortcut::Appearance,
        Shortcut::Instances,
        Shortcut::Mods,
        Shortcut::Settings,
        Shortcut::Log,
        Shortcut::RandomTheme,
        Shortcut::ToggleBackground,
        Shortcut::Performance,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Placement {
    /// Content centred, the default launcher shape.
    Centered,
    /// Content pushed to the left edge, artwork on the right.
    Left,
    /// Content pushed to the right edge.
    Right,
    /// Wide artwork band with content along the bottom.
    Banner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Corners {
    Sharp,
    Rounded,
    Pill,
    Squircle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MotionStyle {
    Still,
    Smooth,
    Brisk,
    Springy,
    Sharp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LabelCase {
    AsWritten,
    Upper,
    Title,
}

/// How the interface itself is arranged, as opposed to how it is coloured.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    /// Global UI multiplier, 0.8..=1.4.
    pub scale: f32,
    /// Spacing unit in px, 12..=28.
    pub density: u16,
    /// Section order in the menu. Repaired to a complete permutation.
    pub order: Vec<Section>,
    /// Sections to keep out of the menu.
    pub hidden: Vec<Section>,
    /// Capabilities promoted to the home screen.
    pub shortcuts: Vec<Shortcut>,
    pub placement: Placement,
    pub corners: Corners,
    pub motion: MotionStyle,
    /// Multiplier on every transition duration, 0..=2.
    pub motion_scale: f32,
    pub label_case: LabelCase,
    /// Render controls as icon-only, dropping their text labels.
    pub icon_only: bool,
}

impl Default for Layout {
    fn default() -> Self {
        Layout {
            scale: 1.0,
            density: 16,
            order: Section::ALL.to_vec(),
            hidden: Vec::new(),
            shortcuts: Vec::new(),
            placement: Placement::Centered,
            corners: Corners::Rounded,
            motion: MotionStyle::Smooth,
            motion_scale: 1.0,
            label_case: LabelCase::AsWritten,
            icon_only: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeMeta {
    pub id: String,
    pub name: String,
    pub author: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    pub created_at: String,
    /// Built-in themes cannot be deleted by the user.
    pub builtin: bool,
    pub schema: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeSpec {
    pub meta: ThemeMeta,
    pub tokens: Tokens,
}

// ---------------------------------------------------------------------------
// Bounds + sanitisation helpers
// ---------------------------------------------------------------------------

impl ThemeSpec {
    pub fn clamp_in_place(&mut self) {
        self.tokens.clamp_in_place();
    }
}

impl Tokens {
    pub fn clamp_in_place(&mut self) {
        let p = &mut self.palette;
        // `f32::clamp` propagates NaN, so every float goes through `finite`
        // first. A theme carrying NaN would serialise as `null` and would make
        // the renderer drop the whole custom-property block.
        p.elevation = finite(p.elevation, 0.5).clamp(0.0, 1.0);

        let s = &mut self.shape;
        s.radius_sm = s.radius_sm.min(48);
        s.radius = s.radius.min(64);
        s.radius_lg = s.radius_lg.min(96);
        s.radius_pill = s.radius_pill.min(999);
        s.border_width = s.border_width.min(6);
        s.ring_width = s.ring_width.min(12);
        // Keep the shape ramp monotonic so components never look inverted.
        s.radius = s.radius.max(s.radius_sm);
        s.radius_lg = s.radius_lg.max(s.radius);

        let t = &mut self.typography;
        t.scale = finite(t.scale, 1.0).clamp(0.8, 1.35);
        t.weight_regular = t.weight_regular.clamp(100, 700);
        t.weight_bold = t.weight_bold.clamp(t.weight_regular, 900);
        t.letter_spacing = finite(t.letter_spacing, 0.0).clamp(-0.05, 0.2);
        t.line_height = finite(t.line_height, 1.4).clamp(1.0, 1.8);

        let m = &mut self.motion;
        m.duration_fast = m.duration_fast.min(600);
        m.duration = m.duration.clamp(m.duration_fast, 1400);
        m.duration_slow = m.duration_slow.clamp(m.duration, 3000);
        m.hover_intensity = finite(m.hover_intensity, 1.0).clamp(0.5, 2.0);

        let d = &mut self.decor;
        d.grain = finite(d.grain, 0.0).clamp(0.0, 1.0);
        d.vignette = finite(d.vignette, 0.0).clamp(0.0, 1.0);
        d.glow = finite(d.glow, 0.0).clamp(0.0, 1.0);
        d.aurora = finite(d.aurora, 0.0).clamp(0.0, 1.0);
        d.rim = finite(d.rim, 0.0).clamp(0.0, 1.0);
        d.pattern_scale = finite(d.pattern_scale, 0.5).clamp(0.05, 1.0);

        let b = &mut self.background;
        b.angle = finite(b.angle, 0.0).rem_euclid(360.0);
        b.image_opacity = finite(b.image_opacity, 1.0).clamp(0.0, 1.0);
        b.image_blur = finite(b.image_blur, 0.0).clamp(0.0, 40.0);
        b.dim = finite(b.dim, 0.0).clamp(-0.6, 0.6);
        b.artwork_strength = finite(b.artwork_strength, default_artwork_strength()).clamp(0.0, 1.0);
        if b.artwork_strength <= 0.001 {
            // Zero strength is indistinguishable from "no artwork", and keeping
            // the layer around for it would cost a paint for nothing.
            b.artwork = Artwork::None;
        }
        if b.colors.is_empty() {
            b.colors.push(Color::rgb(12, 14, 20));
        }
        if b.colors.len() > 5 {
            b.colors.truncate(5);
        }
        if b.kind == BackgroundKind::Image && b.image.is_none() {
            b.kind = BackgroundKind::Linear;
        }
        if !matches!(b.kind, BackgroundKind::Image) {
            b.image = None;
        }

        let c = &mut self.components;
        c.translucency = finite(c.translucency, 0.6).clamp(0.0, 1.0);
        c.density = c.density.clamp(0, 40);

        self.layout.clamp_in_place();
    }
}

impl Layout {
    /// Repair the arrangement into something renderable.
    ///
    /// The menu order is the sharp edge here: it is the one list a theme fills
    /// in itself, so it is rebuilt rather than trusted. Unknown entries are
    /// dropped by the enum decoder before they get this far, duplicates are
    /// collapsed, and anything missing is appended in the default order. The
    /// result is always a complete permutation of [`Section::ALL`], which is
    /// what stops a hand-edited theme from hiding the whole menu.
    pub fn clamp_in_place(&mut self) {
        self.scale = finite(self.scale, 1.0).clamp(0.8, 1.4);
        self.density = self.density.clamp(12, 28);
        self.motion_scale = finite(self.motion_scale, 1.0).clamp(0.0, 2.0);

        let mut order: Vec<Section> = Vec::with_capacity(Section::ALL.len());
        for section in std::mem::take(&mut self.order) {
            if !order.contains(&section) {
                order.push(section);
            }
        }
        for section in Section::ALL {
            if !order.contains(&section) {
                order.push(section);
            }
        }
        self.order = order;

        self.hidden.retain(|s| Section::ALL.contains(s));
        self.hidden.dedup();

        self.shortcuts.retain(|s| Shortcut::ALL.contains(s));
        self.shortcuts.dedup();
        // A home screen with a wall of buttons is not a home screen.
        self.shortcuts.truncate(6);
    }
}

impl ThemeSpec {
    /// Trim free-text metadata and neutralise control characters. Rendering
    /// uses text interpolation, never `innerHTML`, so this is defence in depth.
    pub fn sanitize_meta(&mut self) {
        self.meta.name = sanitize_text(&self.meta.name, MAX_NAME, "Sifora theme");
        self.meta.author = sanitize_text(&self.meta.author, 48, "sifora");
        self.meta.description = sanitize_text(&self.meta.description, MAX_DESCRIPTION, "");
        self.meta.prompt = self.meta.prompt.take().map(|p| sanitize_text(&p, MAX_PROMPT, ""));
        self.meta.id = sanitize_slug(&self.meta.id);
        self.meta.schema = THEME_SCHEMA;
    }
}

fn finite(v: f32, fallback: f32) -> f32 {
    if v.is_finite() {
        v
    } else {
        fallback
    }
}

/// Remove control characters and angle brackets, collapse whitespace and
/// hard-cap the length.
///
/// `<` and `>` are stripped even though the renderer uses text interpolation
/// and never `innerHTML`: it makes "a theme can never contain markup" a
/// property of the data itself rather than of a rendering path we have to
/// remember to keep safe.
pub fn sanitize_text(input: &str, max: usize, fallback: &str) -> String {
    let cleaned: String = input
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .map(|c| match c {
            '\n' | '\t' => ' ',
            '<' | '>' => ' ',
            other => other,
        })
        .collect();
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return fallback.to_string();
    }
    if collapsed.chars().count() > max {
        let out: String = collapsed.chars().take(max).collect();
        out.trim_end().to_string()
    } else {
        collapsed
    }
}

/// Lowercase `[a-z0-9-]` identifier, used for theme ids and builtin keys.
pub fn sanitize_slug(input: &str) -> String {
    let mut out = String::with_capacity(input.len().min(48));
    let mut prev_dash = true;
    for ch in input.chars() {
        let c = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else {
            '-'
        };
        if c == '-' {
            if prev_dash {
                continue;
            }
            prev_dash = true;
        } else {
            prev_dash = false;
        }
        out.push(c);
        if out.len() >= 48 {
            break;
        }
    }
    out.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tokens() -> Tokens {
        let mut t = crate::theme::defaults::default_tokens();
        t.clamp_in_place();
        t
    }

    #[test]
    fn rejects_unknown_fields() {
        let json = r#"{"meta":{"id":"x","name":"x","author":"x","created_at":"0","builtin":false,"schema":1},
            "tokens":{"__proto__":{"polluted":true}}}"#;
        let err = serde_json::from_str::<ThemeSpec>(json);
        assert!(err.is_err(), "unknown keys must be rejected outright");
    }

    #[test]
    fn rejects_css_injection_shaped_fields() {
        // `author` is the widest free-text field we have; even so, an object
        // or a selector-looking string is only ever stored as plain text.
        let mut spec = crate::theme::defaults::default_theme();
        spec.meta.author = "</style><script>alert(1)</script>".into();
        spec.sanitize_meta();
        assert!(!spec.meta.author.contains('<'));
    }

    #[test]
    fn clamping_keeps_shape_ramp_monotonic() {
        let mut t = crate::theme::defaults::default_tokens();
        t.shape.radius = 0;
        t.shape.radius_lg = 1;
        t.clamp_in_place();
        assert!(t.shape.radius >= t.shape.radius_sm);
        assert!(t.shape.radius_lg >= t.shape.radius);
    }

    #[test]
    fn clamping_neutralises_nan_and_infinity() {
        let mut t = sample_tokens();
        t.palette.elevation = f32::NAN;
        t.typography.scale = f32::INFINITY;
        t.motion.hover_intensity = f32::NEG_INFINITY;
        t.clamp_in_place();
        assert!(t.palette.elevation.is_finite());
        assert!(t.typography.scale.is_finite());
        assert!(t.motion.hover_intensity.is_finite());
    }

    #[test]
    fn image_kind_without_image_falls_back_to_gradient() {
        let mut t = sample_tokens();
        t.background.kind = BackgroundKind::Image;
        t.background.image = None;
        t.clamp_in_place();
        assert_eq!(t.background.kind, BackgroundKind::Linear);
    }

    #[test]
    fn slug_sanitizer_produces_safe_identifiers() {
        assert_eq!(sanitize_slug("  My Theme!! 2024 "), "my-theme-2024");
        assert_eq!(sanitize_slug("../../etc/passwd"), "etc-passwd");
        assert_eq!(sanitize_slug(""), "");
    }
}