//! A strict, injection-proof colour primitive.
//!
//! Themes (including AI-generated ones) may only ever express colours through
//! [`Color`]. The type has no `FromStr` that accepts arbitrary text: it accepts
//! a hexadecimal literal or a numeric RGB(A) tuple and nothing else, and it
//! *re-serialises* into a canonical form. That makes it impossible for a theme
//! to smuggle `url(...)`, `expression(...)`, a semicolon, or a comment into a
//! stylesheet, because such a string can never be represented by this type.

use serde::de::{self, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    /// 0.0 ..= 1.0
    pub a: f32,
}

impl Default for Color {
    fn default() -> Self {
        Color { r: 0, g: 0, b: 0, a: 1.0 }
    }
}

impl Color {
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    pub const WHITE: Color = Color::rgb(255, 255, 255);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b, a: 1.0 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: f32) -> Self {
        Color { r, g, b, a }
    }

    pub fn parse(input: &str) -> Option<Self> {
        let s = input.trim();
        let hex = s.strip_prefix('#').unwrap_or(s);
        if hex.is_empty() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let nib = |i: usize| -> Option<u8> {
            let byte = hex.as_bytes().get(i)?.to_ascii_lowercase();
            Some((hex_val(byte)? << 4) | hex_val(*hex.as_bytes().get(i + 1)?)?)
        };
        match hex.len() {
            3 => {
                // #rgb -> #rrggbb
                let b = hex.as_bytes();
                let v = |i: usize| hex_val(b[i].to_ascii_lowercase());
                Some(Color::rgb(v(0)? * 17, v(1)? * 17, v(2)? * 17))
            }
            4 => {
                let b = hex.as_bytes();
                let v = |i: usize| hex_val(b[i].to_ascii_lowercase());
                let (r, g, bb, a) = (v(0)?, v(1)?, v(2)?, v(3)?);
                Some(Color::rgba(r * 17, g * 17, bb * 17, a as f32 / 15.0))
            }
            6 => Some(Color::rgb(nib(0)?, nib(2)?, nib(4)?)),
            8 => {
                let a = (nib(6)? as f32) / 255.0;
                Some(Color::rgba(nib(0)?, nib(2)?, nib(4)?, a))
            }
            _ => None,
        }
    }

    /// Canonical `#rrggbb` / `#rrggbbaa` output.
    pub fn to_css(self) -> String {
        if self.a >= 0.999 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, (self.a.clamp(0.0, 1.0) * 255.0).round() as u8)
        }
    }

    /// Relative luminance per WCAG 2.1.
    pub fn luminance(self) -> f32 {
        fn lin(c: f32) -> f32 {
            let c = c / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * lin(self.r as f32) + 0.7152 * lin(self.g as f32) + 0.0722 * lin(self.b as f32)
    }

    /// WCAG contrast ratio between two opaque colours (1.0 ..= 21.0).
    pub fn contrast_ratio(a: Self, b: Self) -> f32 {
        let (la, lb) = (a.luminance(), b.luminance());
        let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// Blend towards `other` by `t` (0..=1) in sRGB space.
    pub fn mix(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let f = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round().clamp(0.0, 255.0) as u8;
        Color { r: f(self.r, other.r), g: f(self.g, other.g), b: f(self.b, other.b), a: self.a + (other.a - self.a) * t }
    }

    pub fn with_alpha(self, a: f32) -> Self {
        Color { a: a.clamp(0.0, 1.0), ..self }
    }

    /// Slightly shift hue/saturation/lightness in HSL space.
    pub fn adjust(self, dh: f32, ds: f32, dl: f32) -> Self {
        let (h, s, l) = self.hsl();
        hsla((h + dh).rem_euclid(360.0), (s + ds).clamp(0.0, 1.0), (l + dl).clamp(0.0, 1.0), self.a)
    }

    pub fn hsl(self) -> (f32, f32, f32) {
        let (r, g, b) = (self.r as f32 / 255.0, self.g as f32 / 255.0, self.b as f32 / 255.0);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let l = (max + min) / 2.0;
        if (max - min).abs() < f32::EPSILON {
            return (0.0, 0.0, l);
        }
        let d = max - min;
        let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
        let h = if (max - r).abs() < f32::EPSILON {
            ((g - b) / d + if g < b { 6.0 } else { 0.0 }) * 60.0
        } else if (max - g).abs() < f32::EPSILON {
            ((b - r) / d + 2.0) * 60.0
        } else {
            ((r - g) / d + 4.0) * 60.0
        };
        (h, s, l)
    }

    /// Pick black or white text for this background (returns `prefer_dark`).
    pub fn best_text_contrast(self) -> Color {
        if self.luminance() > 0.42 {
            Color::rgb(17, 17, 20)
        } else {
            Color::rgb(245, 246, 250)
        }
    }

    pub fn is_light(self) -> bool {
        self.luminance() > 0.42
    }
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Convenience constructor from an opaque HSL triple.
pub fn hsl(h: f32, s: f32, l: f32) -> Color {
    hsla(h, s, l, 1.0)
}

/// Convenience constructor from HSL plus an alpha channel.
pub fn hsla(h: f32, s: f32, l: f32, a: f32) -> Color {
    let h = h.rem_euclid(360.0);
    let s = s.clamp(0.0, 1.0);
    let l = l.clamp(0.0, 1.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h / 60.0) as u32 % 6 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color { r: to8(r + m), g: to8(g + m), b: to8(b + m), a: a.clamp(0.0, 1.0) }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_css())
    }
}

impl Serialize for Color {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_css())
    }
}

const MAX_INPUT_LEN: usize = 32;

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct ColorVisitor;

        impl<'v> Visitor<'v> for ColorVisitor {
            type Value = Color;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("hex colour like \"#7c5cff\", \"#7c5cffaa\", \"#7cf\" or [r,g,b,a] numbers")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Color, E> {
                if v.len() > MAX_INPUT_LEN {
                    return Err(E::custom("colour literal is too long"));
                }
                Color::parse(v).ok_or_else(|| {
                    E::custom("colour must be a hexadecimal literal (#rgb, #rgba, #rrggbb, #rrggbbaa)")
                })
            }

            fn visit_seq<A: SeqAccess<'v>>(self, mut seq: A) -> Result<Color, A::Error> {
                let mut comps: Vec<f32> = Vec::with_capacity(4);
                while let Some(v) = seq.next_element::<f32>()? {
                    comps.push(v);
                    if comps.len() > 4 {
                        return Err(de::Error::custom("colour tuple accepts at most 4 components"));
                    }
                }
                let take = |i: usize, fallback: f32| comps.get(i).copied().unwrap_or(fallback);
                let chk = |v: f32| -> Result<u8, A::Error> {
                    if !v.is_finite() || !(0.0..=255.0).contains(&v) {
                        return Err(de::Error::custom("colour channel out of range 0..255"));
                    }
                    Ok(v.round() as u8)
                };
                let alpha = take(3, 1.0);
                if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
                    return Err(de::Error::custom("colour alpha out of range 0..1"));
                }
                Ok(Color { r: chk(take(0, 0.0))?, g: chk(take(1, 0.0))?, b: chk(take(2, 0.0))?, a: alpha })
            }
        }

        d.deserialize_any(ColorVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_canonical_forms() {
        assert_eq!(Color::parse("#7c5cff"), Some(Color::rgb(124, 92, 255)));
        assert_eq!(Color::parse("7c5cff"), Some(Color::rgb(124, 92, 255)));
        assert_eq!(Color::parse("#7CF"), Some(Color::rgb(119, 204, 255)));
        assert_eq!(Color::parse("#00000080").unwrap().a, 0.5019608);
    }

    #[test]
    fn rejects_anything_that_is_not_a_hex_literal() {
        for hostile in [
            "url(javascript:alert(1))",
            "red; background: url(http://evil/x.png)",
            "#7c5cff } body { display:none",
            "rgb(1,2,3)",
            "var(--x)",
            "#12345",
            "expression(alert(1))",
        ] {
            assert!(Color::parse(hostile).is_none(), "must reject {hostile:?}");
        }
    }

    #[test]
    fn serialises_to_a_canonical_hex_string() {
        let c = Color::rgba(255, 0, 128, 0.5);
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.starts_with("\"#"), "unexpected serialisation {json}");
        assert!(!json.contains('('));
    }

    #[test]
    fn deserialiser_rejects_hostile_strings() {
        for hostile in [
            "\"url(javascript:alert(1))\"",
            "\"#fff); background-image:url(http://evil/)\"",
            "\"\\22\"",
        ] {
            let r: Result<Color, _> = serde_json::from_str(hostile);
            assert!(r.is_err(), "must reject {hostile}");
        }
    }

    #[test]
    fn hsl_roundtrip_is_stable() {
        let c = hsl(258.0, 0.92, 0.68);
        let (h, s, l) = c.hsl();
        assert!((h - 258.0).abs() < 1.5, "hue {h}");
        assert!((s - 0.92).abs() < 0.02, "sat {s}");
        assert!((l - 0.68).abs() < 0.02, "light {l}");
    }

    #[test]
    fn contrast_ratio_matches_reference_values() {
        let black = Color::rgb(0, 0, 0);
        let white = Color::rgb(255, 255, 255);
        assert!((Color::contrast_ratio(black, white) - 21.0).abs() < 0.01);
        let same = Color::rgb(90, 90, 90);
        assert!((Color::contrast_ratio(same, same) - 1.0).abs() < 0.001);
    }
}