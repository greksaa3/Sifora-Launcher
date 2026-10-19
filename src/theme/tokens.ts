/**
 * Theme → CSS custom properties.
 *
 * This is the only place where theme data becomes style values. The mapping is
 * exhaustive and typed: every value is a canonical hex colour produced by
 * `parseTheme`, a clamped number, or a fixed string chosen from a closed enum.
 * No theme value is ever concatenated into a selector, an at-rule, a `url()`
 * or a declaration name, so there is no path from theme data to CSS injection.
 */

import { parseTheme } from './validate'
import type { Easing, FontFamily, Pattern, ThemeSpec } from '@/types'

/**
 * Font stacks are compile-time constants keyed by the enum. A theme picks a
 * key; it never supplies a font name.
 */
const FONT_STACKS: Record<FontFamily, string> = {
  system: 'system-ui, -apple-system, "Segoe UI Variable Text", "Segoe UI", sans-serif',
  segoe: '"Segoe UI Variable Display", "Segoe UI", system-ui, sans-serif',
  georgia: 'Georgia, "Times New Roman", serif',
  palatino: '"Palatino Linotype", "Book Antiqua", Palatino, serif',
  trebuchet: '"Trebuchet MS", "Segoe UI", sans-serif',
  verdana: 'Verdana, Geneva, "Segoe UI", sans-serif',
  tahoma: 'Tahoma, "Segoe UI", Geneva, sans-serif',
  impact: 'Impact, Haettenschweiler, "Arial Narrow Bold", sans-serif',
  consolas: 'Consolas, "Cascadia Mono", "Courier New", monospace',
  cascadia: '"Cascadia Code", Consolas, "Courier New", monospace',
  courier: '"Courier New", Courier, monospace',
  times: '"Times New Roman", Times, serif',
}

/** Fixed cubic-bezier curves; durations are the tunable part. */
const EASINGS: Record<Easing, string> = {
  standard: 'cubic-bezier(0.4, 0, 0.2, 1)',
  linear: 'linear',
  decelerate: 'cubic-bezier(0, 0, 0.2, 1)',
  accelerate: 'cubic-bezier(0.4, 0, 1, 1)',
  emphasized: 'cubic-bezier(0.2, 0, 0, 1)',
  spring: 'cubic-bezier(0.34, 1.4, 0.64, 1)',
}

/** Pattern tiles are inline SVG data URIs chosen by enum; the theme supplies
 *  only the colour and the scale. */
const PATTERN_TILES: Record<Pattern, string | null> = {
  none: null,
  dots:
    "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='24' height='24'%3E%3Ccircle cx='2' cy='2' r='1' fill='white' fill-opacity='0.16'/%3E%3C/svg%3E\")",
  grid:
    "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='48' height='48'%3E%3Cpath d='M48 0H0v48' fill='none' stroke='white' stroke-opacity='0.08'/%3E%3C/svg%3E\")",
  noise:
    "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='120' height='120'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='3'/%3E%3C/filter%3E%3Crect width='120' height='120' filter='url(%23n)' opacity='0.5'/%3E%3C/svg%3E\")",
  diagonal:
    "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='16' height='16'%3E%3Cpath d='M0 16L16 0' stroke='white' stroke-opacity='0.07'/%3E%3C/svg%3E\")",
  stars:
    "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='160' height='160'%3E%3Ccircle cx='18' cy='34' r='1' fill='white' fill-opacity='0.5'/%3E%3Ccircle cx='96' cy='12' r='0.8' fill='white' fill-opacity='0.35'/%3E%3Ccircle cx='140' cy='96' r='1.1' fill='white' fill-opacity='0.4'/%3E%3Ccircle cx='44' cy='128' r='0.7' fill='white' fill-opacity='0.3'/%3E%3C/svg%3E\")",
  topography:
    "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='120' height='120'%3E%3Cg fill='none' stroke='white' stroke-opacity='0.08'%3E%3Ccircle cx='60' cy='60' r='12'/%3E%3Ccircle cx='60' cy='60' r='26'/%3E%3Ccircle cx='60' cy='60' r='40'/%3E%3Ccircle cx='60' cy='60' r='54'/%3E%3C/g%3E%3C/svg%3E\")",
}

const clamp = (value: number, min: number, max: number): number =>
  Math.min(max, Math.max(min, value))

const px = (value: number): string => `${Math.round(value * 100) / 100}px`
const ms = (value: number): string => `${Math.round(value)}ms`
const pct = (value: number): string => `${Math.round(value * 1000) / 1000}%`

export interface BackdropLayers {
  base: string
  pattern: string
  image: string
  overlay: string
}

/** Build every custom property for a theme. Returns a plain object. */
export function themeVariables(theme: ThemeSpec): Record<string, string> {
  const parsed = parseTheme(theme)
  const spec = parsed?.theme ?? theme
  const { palette: p, shape: s, typography: t, motion: m, decor: d, background: b, components: c } =
    spec.tokens

  const stops = (b.colors.length ? b.colors : ['#0b0d16']).slice(0, 5)
  const elevation = clamp(p.elevation, 0, 1)

  const vars: Record<string, string> = {
    /* palette */
    '--sf-accent': p.accent,
    '--sf-accent-hover': p.accent_hover,
    '--sf-on-accent': p.on_accent,
    '--sf-surface': p.surface,
    '--sf-surface-raised': p.surface_raised,
    '--sf-surface-sunken': p.surface_sunken,
    '--sf-text': p.text,
    '--sf-text-muted': p.text_muted,
    '--sf-border': p.border,
    '--sf-success': p.success,
    '--sf-warning': p.warning,
    '--sf-danger': p.danger,
    '--sf-shadow': p.shadow,
    '--sf-elevation': String(elevation),

    /* shape */
    '--sf-radius-sm': px(s.radius_sm),
    '--sf-radius': px(s.radius),
    '--sf-radius-lg': px(s.radius_lg),
    '--sf-radius-pill': s.radius_pill >= 999 ? '9999px' : px(s.radius_pill),
    '--sf-border-width': px(s.border_width),
    '--sf-ring-width': px(s.ring_width),

    /* typography */
    '--sf-font': FONT_STACKS[t.family],
    '--sf-font-display': FONT_STACKS[t.display_family],
    '--sf-scale': String(clamp(t.scale, 0.8, 1.35)),
    '--sf-weight': String(clamp(t.weight_regular, 100, 700)),
    '--sf-weight-bold': String(clamp(t.weight_bold, 100, 900)),
    '--sf-letter-spacing': `${clamp(t.letter_spacing, -0.05, 0.2)}em`,
    '--sf-line-height': String(clamp(t.line_height, 1, 1.8)),
    '--sf-label-transform': t.uppercase_labels ? 'uppercase' : 'none',

    /* motion */
    '--sf-dur-fast': ms(clamp(m.duration_fast, 60, 600)),
    '--sf-dur': ms(clamp(m.duration, 60, 1400)),
    '--sf-dur-slow': ms(clamp(m.duration_slow, 60, 3000)),
    '--sf-ease': EASINGS[m.easing],
    '--sf-hover': String(clamp(m.hover_intensity, 0.5, 2)),
    '--sf-ambient': m.ambient ? 'running' : 'paused',

    /* decor */
    '--sf-grain': String(clamp(d.grain, 0, 1)),
    '--sf-vignette': String(clamp(d.vignette, 0, 1)),
    '--sf-glow': String(clamp(d.glow, 0, 1)),
    '--sf-aurora': String(clamp(d.aurora, 0, 1)),
    '--sf-rim': String(clamp(d.rim, 0, 1)),
    '--sf-pattern-scale': String(clamp(d.pattern_scale, 0.05, 1)),
    '--sf-pattern': PATTERN_TILES[d.pattern] ?? 'none',
    '--sf-scanlines': d.scanlines ? 'repeating-linear-gradient(to bottom, rgb(0 0 0 / 0.22) 0 1px, transparent 1px 3px)' : 'none',

    /* components */
    '--sf-translucency': String(clamp(c.translucency, 0, 1)),
    '--sf-density': px(c.density),
    '--sf-btn': c.button,
    '--sf-field': c.input,
    '--sf-card': c.card,

    /* background stops */
    '--sf-bg-1': stops[0],
    '--sf-bg-2': stops[1] ?? stops[0],
    '--sf-bg-3': stops[2] ?? stops[1] ?? stops[0],
    '--sf-bg-4': stops[3] ?? stops[2] ?? stops[0],
    '--sf-bg-5': stops[4] ?? stops[3] ?? stops[0],
    '--sf-bg-angle': `${clamp(b.angle, 0, 360)}deg`,
    '--sf-bg-dim': String(clamp(b.dim, -0.6, 0.6)),
    '--sf-img-opacity': String(clamp(b.image_opacity, 0, 1)),
    '--sf-img-blur': px(clamp(b.image_blur, 0, 40)),
    '--sf-img-fit': b.image_fit,
  }

  // Derived, theme-independent composites. `color-mix` is used so the exact
  // alpha values stay in one place (the theme tokens) instead of being
  // re-derived per component.
  vars['--sf-surface-a'] = `color-mix(in srgb, var(--sf-surface) calc(var(--sf-translucency) * 100%), transparent)`
  vars['--sf-raised-a'] = `color-mix(in srgb, var(--sf-surface-raised) calc(var(--sf-translucency) * 100%), transparent)`
  vars['--sf-accent-a'] = `color-mix(in srgb, var(--sf-accent) 22%, transparent)`
  vars['--sf-accent-ring'] = `color-mix(in srgb, var(--sf-accent) 45%, transparent)`
  vars['--sf-border-strong'] = `color-mix(in srgb, var(--sf-border) 160%, transparent)`
  vars['--sf-shadow-color'] = `color-mix(in srgb, var(--sf-shadow) 55%, transparent)`
  vars['--sf-shadow-lg'] = `0 calc(var(--sf-radius-lg) * 0.8) calc(var(--sf-radius-lg) * 2.2) calc(var(--sf-shadow-color))`
  vars['--sf-shadow-sm'] = `0 calc(var(--sf-radius) * 0.35) calc(var(--sf-radius) * 1.1) calc(var(--sf-shadow-color))`
  vars['--sf-accent-glow'] = `0 0 0 calc(var(--sf-ring-width) + var(--sf-glow) * 10px) var(--sf-accent-ring)`
  vars['--sf-hover-lift'] = `${Math.max(0, clamp(m.hover_intensity, 0.5, 2) - 1) * 3}px`
  vars['--sf-background-image'] = b.image ? `var(--sf-bg-image, none)` : 'none'

  return vars
}

/** Compose the four stacked backdrop layers for the given background spec. */
export function backdropLayers(theme: ThemeSpec, imageUrl?: string): BackdropLayers {
  const parsed = parseTheme(theme)
  const b = (parsed?.theme ?? theme).tokens.background
  const angle = clamp(b.angle, 0, 360)

  let base: string
  switch (b.kind) {
    case 'solid':
      base = `linear-gradient(${angle}deg, var(--sf-bg-1), var(--sf-bg-1))`
      break
    case 'radial':
      base =
        `radial-gradient(120% 90% at 50% 8%, var(--sf-bg-2), var(--sf-bg-3) 55%, var(--sf-bg-4) 100%),` +
        `linear-gradient(${angle}deg, var(--sf-bg-1), var(--sf-bg-4))`
      break
    case 'conic':
      base =
        `conic-gradient(from ${angle}deg at 50% 40%, var(--sf-bg-1), var(--sf-bg-2), var(--sf-bg-3), var(--sf-bg-4), var(--sf-bg-1))`
      break
    case 'mesh':
      base =
        `radial-gradient(70% 60% at 12% 18%, var(--sf-bg-2), transparent 70%),` +
        `radial-gradient(60% 55% at 86% 26%, var(--sf-bg-3), transparent 72%),` +
        `radial-gradient(80% 70% at 50% 96%, var(--sf-bg-4), transparent 74%),` +
        `linear-gradient(${angle}deg, var(--sf-bg-1), var(--sf-bg-5))`
      break
    case 'aurora':
      base =
        `radial-gradient(55% 45% at 18% 12%, var(--sf-bg-2), transparent 70%),` +
        `radial-gradient(50% 40% at 84% 22%, var(--sf-bg-3), transparent 72%),` +
        `radial-gradient(70% 55% at 60% 104%, var(--sf-bg-4), transparent 76%),` +
        `radial-gradient(40% 35% at 8% 92%, var(--sf-bg-5), transparent 70%),` +
        `linear-gradient(${angle}deg, var(--sf-bg-1), var(--sf-bg-5))`
      break
    case 'image':
      base =
        `radial-gradient(60% 50% at 50% 10%, var(--sf-bg-2), transparent 72%),` +
        `linear-gradient(${angle}deg, var(--sf-bg-1), var(--sf-bg-4))`
      break
    default:
      base = `linear-gradient(${angle}deg, var(--sf-bg-1), var(--sf-bg-2), var(--sf-bg-4))`
  }

  const pattern =
    parsed?.theme.tokens.decor.pattern === 'none' ? 'none' : 'var(--sf-pattern)'
  const image = imageUrl
    ? `linear-gradient(var(--sf-img-overlay, transparent), var(--sf-img-overlay, transparent)),` +
      `url("${imageUrl.replace(/["'\\)]/g, '')}")`
    : 'none'

  return { base, pattern, image, overlay: 'var(--sf-overlay-tint)' }
}

/** Human readable summary used in the stage-2 theme editor. */
export function describeTheme(theme: ThemeSpec): string {
  const parsed = parseTheme(theme)
  if (!parsed) return 'Некорректная тема'
  const { tokens, meta } = parsed.theme
  return [
    meta.name,
    tokens.background.kind,
    `${tokens.typography.family}`,
    tokens.components.card === 'glass' ? 'стекло' : tokens.components.card,
  ].join(' · ')
}

export { pct }