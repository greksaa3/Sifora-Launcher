//! Bilingual (ru/en) vocabulary used by the offline theme composer.
//!
//! The composer is not a template picker: it reads the description, extracts
//! colour words, mood words, shape/typography/motion preferences and composes a
//! *new* combination from them. Users can therefore describe things that were
//! never anticipated, as long as they map onto the design-system vocabulary.
//!
//! Unknown words are not an error — they simply do not steer the result, and
//! the prompt hash provides deterministic variety for the parts that are unset.

use super::model::{ComponentStyle, Easing, FontFamily};

/// A colour word: words -> hue anchor + saturation strength.
pub struct HueWord {
    pub words: &'static [&'static str],
    pub hue: f32,
    pub sat: f32,
}

/// Colour vocabulary.
///
/// Stems are used rather than whole words because Russian adjectives inflect
/// (`синий` / `синяя` / `синюю`), so matching a shared prefix covers every
/// form without listing them all.
pub static HUE_WORDS: &[HueWord] = &[
    HueWord { words: &["красн", "алый", "багров", "малинов", "кармин", "red"], hue: 356.0, sat: 0.78 },
    HueWord { words: &["розов", "pink", "фукси"], hue: 332.0, sat: 0.74 },
    HueWord { words: &["маджен", "magenta"], hue: 312.0, sat: 0.85 },
    HueWord { words: &["пурпур", "фиолет", "сиренев", "лаванд", "amethyst", "purple"], hue: 276.0, sat: 0.72 },
    HueWord { words: &["индиго", "indigo"], hue: 252.0, sat: 0.68 },
    HueWord { words: &["син", "blue", "лазур"], hue: 216.0, sat: 0.76 },
    HueWord { words: &["голуб", "skyblue"], hue: 205.0, sat: 0.62 },
    HueWord { words: &["циан", "cyan", "бирюз", "аква", "teal", "turquo"], hue: 186.0, sat: 0.72 },
    HueWord { words: &["изумруд", "emerald", "мятн", "mint"], hue: 160.0, sat: 0.64 },
    HueWord { words: &["зелён", "зелен", "лайм", "lime", "green"], hue: 132.0, sat: 0.68 },
    HueWord { words: &["оливк", "olive", "хаки"], hue: 92.0, sat: 0.48 },
    HueWord { words: &["жёлт", "желт", "yellow", "золот", "gold", "amber", "янтар"], hue: 44.0, sat: 0.84 },
    HueWord { words: &["оранж", "orange", "персик", "манго", "апельсин"], hue: 26.0, sat: 0.86 },
    HueWord { words: &["коричн", "brown", "шоколад", "кофе", "coffee", "карамел"], hue: 20.0, sat: 0.42 },
    HueWord { words: &["беж", "beige", "песочн", "песчан", "кремов"], hue: 38.0, sat: 0.28 },
    HueWord { words: &["серый", "серебр", "стальн", "grey", "gray", "slate"], hue: 220.0, sat: 0.06 },
    HueWord { words: &["чёрн", "черн", "black", "тёмн", "темн", "dark"], hue: 230.0, sat: 0.18 },
    HueWord { words: &["бел", "white", "светл", "light"], hue: 210.0, sat: 0.10 },
    HueWord { words: &["неон", "neon"], hue: 172.0, sat: 0.95 },
];

/// Mood / atmosphere words that set lightness and contrast direction.
pub struct MoodWord {
    pub words: &'static [&'static str],
    /// -1 = much darker, +1 = much lighter.
    pub lightness: f32,
    /// Added to saturation of accent.
    pub saturation: f32,
    /// Added to panel elevation.
    pub contrast: f32,
}

pub static MOOD_WORDS: &[MoodWord] = &[
    MoodWord { words: &["ноч", "night", "тёмн", "темн", "мрачн"], lightness: -0.22, saturation: -0.05, contrast: 0.10 },
    MoodWord { words: &["рассвет", "утренн", "morning", "dawn", "нежн"], lightness: 0.18, saturation: -0.08, contrast: -0.05 },
    MoodWord { words: &["ярк", "bright", "насыщ", "vivid", "сочн"], lightness: 0.04, saturation: 0.28, contrast: 0.12 },
    MoodWord { words: &["приглуш", "muted", "пастель", "pastel", "нежно"], lightness: 0.12, saturation: -0.28, contrast: -0.08 },
    MoodWord { words: &["контраст", "contrast", "резк", "чётк"], lightness: 0.0, saturation: 0.10, contrast: 0.25 },
    MoodWord { words: &["минимал", "minimal", "чист", "clean", "прост"], lightness: 0.04, saturation: -0.12, contrast: 0.05 },
    MoodWord { words: &["уют", "cozy", "cosy", "тёпл", "тепл", "warm", "уютн"], lightness: 0.06, saturation: 0.08, contrast: -0.02 },
    MoodWord { words: &["холод", "cool", "холодн", "ледяной", "ice", "iceberg"], lightness: -0.06, saturation: -0.05, contrast: 0.05 },
    MoodWord { words: &["неон", "neon", "cyber", "кибер", "synth"], lightness: -0.14, saturation: 0.34, contrast: 0.20 },
    MoodWord { words: &["винтаж", "vintage", "ретро", "retro", "70"], lightness: 0.10, saturation: -0.10, contrast: -0.04 },
    MoodWord { words: &["панк", "punk", "агрессив", "aggressive"], lightness: -0.10, saturation: 0.30, contrast: 0.28 },
    MoodWord { words: &["покой", "calm", "спокойн", "тихий", "quiet", "zen"], lightness: 0.02, saturation: -0.14, contrast: -0.06 },
    MoodWord { words: &["энерг", "energy", "энергич", "быстр"], lightness: 0.02, saturation: 0.22, contrast: 0.16 },
    MoodWord { words: &["роскош", "luxury", "премиум", "premium", "дорог"], lightness: -0.12, saturation: 0.04, contrast: 0.18 },
    MoodWord { words: &["игрив", "playful", "весёл", "весел", "fun", "ярк-"], lightness: 0.10, saturation: 0.24, contrast: 0.08 },
];

/// Shape / component vocabulary.
pub struct ShapeWord {
    pub words: &'static [&'static str],
    /// -1 = sharp, +1 = very round.
    pub radius: f32,
    /// Added to border width.
    pub border: f32,
    /// Requests translucent component styling.
    pub glass: bool,
}

pub static SHAPE_WORDS: &[ShapeWord] = &[
    ShapeWord { words: &["скругл", "rounded", "мягк", "soft", "плавн"], radius: 0.45, border: 0.0, glass: false },
    ShapeWord { words: &["кругл", "circle", "pill", "таблет"], radius: 0.9, border: 0.0, glass: false },
    ShapeWord { words: &["остры", "sharp", "прям", "прямоугольн", "жёстк", "жестк", "angular"], radius: -0.7, border: 0.5, glass: false },
    ShapeWord { words: &["стекл", "glass", "glassmorphism", " frosted", "матов"], radius: 0.25, border: 0.4, glass: true },
    ShapeWord { words: &["минимал", "minimal", "flat"], radius: -0.2, border: -0.5, glass: false },
    ShapeWord { words: &["толст", "thick", "жирн", "chunky", "плотн"], radius: 0.1, border: 1.0, glass: false },
    ShapeWord { words: &["тонк", "thin", "hairline", "изящн", "изящн"], radius: 0.0, border: -0.8, glass: false },
];

/// Typography vocabulary.
pub struct TypeWord {
    pub words: &'static [&'static str],
    pub family: Option<FontId>,
    pub mono: bool,
    pub bold: f32,
    pub spacing: f32,
    pub scale: f32,
    pub uppercase: bool,
}

/// Local mirror of [`super::model::FontFamily`] kept as a plain id so the
/// lexicon table stays `const`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct FontId(pub u8);

pub fn font_from_id(id: FontId) -> FontFamily {
    match id.0 {
        0 => FontFamily::System,
        1 => FontFamily::Segoe,
        2 => FontFamily::Georgia,
        3 => FontFamily::Palatino,
        4 => FontFamily::Trebuchet,
        5 => FontFamily::Verdana,
        6 => FontFamily::Tahoma,
        7 => FontFamily::Impact,
        8 => FontFamily::Consolas,
        9 => FontFamily::Cascadia,
        10 => FontFamily::Courier,
        11 => FontFamily::Times,
        _ => FontFamily::System,
    }
}

pub static TYPE_WORDS: &[TypeWord] = &[
    TypeWord { words: &["моно", "mono", "терминал", "terminal", "код", "code"], family: Some(FontId(8)), mono: true, bold: 0.0, spacing: 0.02, scale: 1.0, uppercase: true },
    TypeWord { words: &["шрифт", "font", "типограф"], family: None, mono: false, bold: 0.0, spacing: 0.0, scale: 1.0, uppercase: false },
    TypeWord { words: &["жирн", "bold", "толст-текст"], family: None, mono: false, bold: 0.3, spacing: 0.0, scale: 1.0, uppercase: false },
    TypeWord { words: &["изящн", "serif", "шрифт-"], family: Some(FontId(2)), mono: false, bold: 0.0, spacing: 0.01, scale: 1.0, uppercase: false },
    TypeWord { words: &["крупн", "large", "большой", "крупный"], family: None, mono: false, bold: 0.1, spacing: 0.0, scale: 0.08, uppercase: false },
    TypeWord { words: &["мелк", "small", "компактн"], family: None, mono: false, bold: 0.0, spacing: 0.0, scale: -0.08, uppercase: false },
    TypeWord { words: &["капс", "uppercase", "прописн", "заглавн"], family: None, mono: false, bold: 0.15, spacing: 0.05, scale: 0.0, uppercase: true },
    TypeWord { words: &["просторн", "spaced", "разрядк", "tracking"], family: None, mono: false, bold: 0.0, spacing: 0.08, scale: 0.0, uppercase: false },
];

/// Background / decoration vocabulary.
pub struct DecorWord {
    pub words: &'static [&'static str],
    pub kind: Option<BgId>,
    pub pattern: Option<PatternId>,
    pub aurora: Option<f32>,
    pub grain: Option<f32>,
    pub vignette: Option<f32>,
    pub glow: Option<f32>,
    pub scanlines: bool,
}

#[derive(Clone, Copy)]
pub struct BgId(pub u8);
#[derive(Clone, Copy, PartialEq)]
pub struct PatternId(pub u8);

pub static DECOR_WORDS: &[DecorWord] = &[
    DecorWord {
        words: &["градиент", "gradient", "перелив", "заливк"],
        kind: Some(BgId(1)),
        pattern: None,
        aurora: None,
        grain: None,
        vignette: None,
        glow: None,
        scanlines: false,
    },
    DecorWord {
        words: &["радиальн", "radial", "круг", "излучен"],
        kind: Some(BgId(2)),
        pattern: None,
        aurora: None,
        grain: None,
        vignette: Some(0.4),
        glow: None,
        scanlines: false,
    },
    DecorWord {
        words: &["конус", "conic", "спираль", "spiral"],
        kind: Some(BgId(3)),
        pattern: None,
        aurora: None,
        grain: None,
        vignette: None,
        glow: None,
        scanlines: false,
    },
    DecorWord {
        words: &["меш", "mesh", "сетк-цвет", "blobs", "блоб", "пятн"],
        kind: Some(BgId(4)),
        pattern: None,
        aurora: Some(0.5),
        grain: None,
        vignette: None,
        glow: None,
        scanlines: false,
    },
    DecorWord {
        words: &["аврора", "aurora", "сиян", "мерцан", "свечен", "glow", "свет"],
        kind: Some(BgId(5)),
        pattern: None,
        aurora: Some(0.75),
        grain: None,
        vignette: Some(0.3),
        glow: Some(0.7),
        scanlines: false,
    },
    DecorWord {
        words: &["изображени", "image", "фото", "picture", "обои"],
        kind: Some(BgId(6)),
        pattern: None,
        aurora: None,
        grain: None,
        vignette: Some(0.35),
        glow: None,
        scanlines: false,
    },
    DecorWord {
        words: &["точк", "dots", "dot", "крап", "крапин"],
        kind: None,
        pattern: Some(PatternId(1)),
        aurora: None,
        grain: None,
        vignette: None,
        glow: None,
        scanlines: false,
    },
    DecorWord {
        words: &["сетк", "grid", "лини", "lines", "полос", "клетк"],
        kind: None,
        pattern: Some(PatternId(2)),
        aurora: None,
        grain: None,
        vignette: None,
        glow: None,
        scanlines: false,
    },
    DecorWord {
        words: &["шум", "noise", "зерн", "плёночн", "пленочн", "grain", "analog", "помех"],
        kind: None,
        pattern: Some(PatternId(3)),
        aurora: None,
        grain: Some(0.16),
        vignette: Some(0.45),
        glow: None,
        scanlines: false,
    },
    DecorWord {
        words: &["диагонал", "diagonal", "скос"],
        kind: None,
        pattern: Some(PatternId(4)),
        aurora: None,
        grain: None,
        vignette: None,
        glow: None,
        scanlines: false,
    },
    DecorWord {
        words: &["звёзд", "звезд", "star", "космос", "space", "галакт"],
        kind: None,
        pattern: Some(PatternId(5)),
        aurora: Some(0.6),
        grain: Some(0.08),
        vignette: Some(0.5),
        glow: None,
        scanlines: false,
    },
    DecorWord {
        words: &["топограф", "topo", "contour", "рельеф", "карт"],
        kind: None,
        pattern: Some(PatternId(6)),
        aurora: None,
        grain: None,
        vignette: Some(0.3),
        glow: None,
        scanlines: false,
    },
    DecorWord {
        words: &["сканлайн", "scanline", "crt", "строк", "ретро-экран"],
        kind: None,
        pattern: None,
        aurora: None,
        grain: Some(0.12),
        vignette: Some(0.55),
        glow: Some(0.4),
        scanlines: true,
    },
];

/// Motion vocabulary.
pub struct MotionWord {
    pub words: &'static [&'static str],
    pub speed: Option<f32>,
    pub easing: Option<EasingId>,
    pub ambient: Option<bool>,
}

#[derive(Clone, Copy)]
pub struct EasingId(pub u8);

pub fn easing_from_id(id: EasingId) -> Easing {
    match id.0 {
        0 => Easing::Standard,
        1 => Easing::Linear,
        2 => Easing::Decelerate,
        3 => Easing::Accelerate,
        4 => Easing::Emphasized,
        5 => Easing::Spring,
        _ => Easing::Standard,
    }
}

pub static MOTION_WORDS: &[MotionWord] = &[
    MotionWord { words: &["быстр", "fast", "snappy", "резк-анимац"], speed: Some(-0.30), easing: Some(EasingId(5)), ambient: None },
    MotionWord { words: &["плавн", "smooth", "soft-анимац", "текуч", "flow"], speed: Some(0.25), easing: Some(EasingId(2)), ambient: None },
    MotionWord { words: &["медленн", "slow", "calm-анимац", "спокойн-анимац"], speed: Some(0.45), easing: Some(EasingId(2)), ambient: None },
    MotionWord { words: &["линейн", "linear", "механик", "robot"], speed: None, easing: Some(EasingId(1)), ambient: None },
    MotionWord { words: &["без анимац", "no-animation", "статич", "static", "без эффект"], speed: Some(1.0), easing: None, ambient: Some(false) },
    MotionWord { words: &["без движени", "reduce-motion", "reduced", "доступн", "accessib"], speed: None, easing: None, ambient: Some(false) },
    MotionWord { words: &["живое", "ambient", "дышан", "пульс", "плавающ"], speed: None, easing: None, ambient: Some(true) },
];

/// Component style vocabulary.
pub struct ComponentWord {
    pub words: &'static [&'static str],
    pub style: Option<StyleId>,
}

#[derive(Clone, Copy)]
pub struct StyleId(pub u8);

pub fn component_style_from_id(id: StyleId) -> ComponentStyle {
    match id.0 {
        0 => ComponentStyle::Solid,
        1 => ComponentStyle::Soft,
        2 => ComponentStyle::Outline,
        3 => ComponentStyle::Glass,
        _ => ComponentStyle::Soft,
    }
}

pub static COMPONENT_WORDS: &[ComponentWord] = &[
    ComponentWord { words: &["залит", "solid", "filled", "плотн-"], style: Some(StyleId(0)) },
    ComponentWord { words: &["полупрозрачн", "translucent", "полупрозр"], style: Some(StyleId(3)) },
    ComponentWord { words: &["контур", "outline", "обводк", "рамк"], style: Some(StyleId(2)) },
    ComponentWord { words: &["приглуш", "soft-", "тонирован"], style: Some(StyleId(1)) },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hue_lookup_handles_cyrillic_and_english() {
        let found: Vec<&HueWord> =
            HUE_WORDS.iter().filter(|h| h.words.iter().any(|w| "фиолетовая тема".contains(w))).collect();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].hue, 276.0);
    }

    #[test]
    fn font_ids_map_to_every_font() {
        for id in 0..12u8 {
            // Must not panic and must be inside the declared set.
            let _ = font_from_id(FontId(id));
        }
        assert!(FontFamily::ALL.contains(&font_from_id(FontId(9))));
    }
}
