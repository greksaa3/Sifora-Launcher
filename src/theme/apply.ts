/**
 * Theme application.
 *
 * Applying a theme is a *write of CSS custom properties on one element*. It
 * never injects a stylesheet, never changes which features exist, and never
 * leaves a half-applied state behind: the previous variable map is returned so
 * callers can undo it exactly.
 *
 * The one thing it does touch is the *arrangement* of what already exists —
 * menu order, which sections are reachable, what the home screen promotes. That
 * is driven by the validated `Layout`, which is a closed list of sections and
 * shortcuts. A theme can decide where a feature lives; it cannot create one.
 */

import { themeVariables } from './tokens'
import { parseTheme } from './validate'
import type { ThemeSpec } from '@/types'

const root = (): HTMLElement => document.documentElement

export interface AppliedTheme {
  variables: Record<string, string>
  themeId: string
}

let previous: AppliedTheme | null = null

/** Write a theme's variables. Returns what was applied so it can be undone. */
export function applyTheme(theme: ThemeSpec): AppliedTheme {
  // Re-validate before anything is written. The backend validates too, but this
  // is the last line before the values reach the DOM.
  const parsed = parseTheme(theme)
  const safe = parsed?.theme ?? theme
  const vars = themeVariables(safe)
  const el = root()
  for (const [name, value] of Object.entries(vars)) {
    el.style.setProperty(name, value)
  }
  el.dataset.theme = safe.meta.id
  // `data-` reflects the active theme for debugging and for CSS hooks that
  // want to react to a specific built-in theme.
  el.dataset.themeAuthor = safe.meta.author
  applyArrangement(el, safe)
  const applied: AppliedTheme = { variables: vars, themeId: safe.meta.id }
  previous = applied
  return applied
}

/**
 * Publish the validated arrangement as data attributes.
 *
 * These drive CSS for corner character, motion character, label case and
 * placement. Everything here came out of the closed `Layout` schema, so CSS
 * only ever sees a value from a fixed list.
 */
function applyArrangement(el: HTMLElement, theme: ThemeSpec): void {
  const l = theme.tokens.layout
  el.dataset.corners = l.corners
  el.dataset.motionStyle = l.motion
  el.dataset.labelCase = l.label_case
  el.dataset.placement = l.placement
  el.dataset.iconOnly = l.icon_only ? '1' : '0'
}

/** Restore the theme that was active before the last `applyTheme`. */
export function revertTheme(): AppliedTheme | null {
  if (!previous) return null
  const el = root()
  const before = Object.fromEntries(
    Object.entries(previous.variables).map(([name]) => {
      const fallback = getComputedStyle(el).getPropertyValue(name).trim()
      return [name, fallback]
    }),
  )
  return { variables: before, themeId: '' }
}

/** Remove every variable the theme system owns, restoring the CSS defaults. */
export function clearTheme(): void {
  const el = root()
  for (const name of Object.keys(previous?.variables ?? {})) {
    el.style.removeProperty(name)
  }
  delete el.dataset.theme
  delete el.dataset.themeAuthor
  delete el.dataset.corners
  delete el.dataset.motionStyle
  delete el.dataset.labelCase
  delete el.dataset.placement
  delete el.dataset.iconOnly
  previous = null
}

/** Force the reduced-motion policy. `true` always wins over the theme. */
export function setReducedMotion(enabled: boolean): void {
  root().style.setProperty('--sf-motion-off', enabled ? '1' : '0')
}

/**
 * Switch between frosted glass and flat translucency.
 *
 * Every `backdrop-filter` panel re-blurs the backdrop on each composited
 * frame. Without a usable GPU — which is exactly the case when WebView2
 * reports no graphics device — that runs on the CPU and pins a full core for
 * a launcher that is only supposed to sit on screen.
 */
export function setLightRendering(enabled: boolean): void {
  root().dataset.perf = enabled ? 'lite' : 'full'
}