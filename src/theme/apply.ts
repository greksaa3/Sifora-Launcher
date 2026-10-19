/**
 * Theme application.
 *
 * Applying a theme is a *write of CSS custom properties on one element*. It
 * never touches the DOM structure, never injects a stylesheet, never changes
 * which features exist, and never leaves a half-applied state behind: the
 * previous variable map is returned so callers can undo it exactly.
 */

import { themeVariables } from './tokens'
import type { ThemeSpec } from '@/types'

const root = (): HTMLElement => document.documentElement

export interface AppliedTheme {
  variables: Record<string, string>
  themeId: string
}

let previous: AppliedTheme | null = null

/** Write a theme's variables. Returns what was applied so it can be undone. */
export function applyTheme(theme: ThemeSpec): AppliedTheme {
  const vars = themeVariables(theme)
  const el = root()
  for (const [name, value] of Object.entries(vars)) {
    el.style.setProperty(name, value)
  }
  el.dataset.theme = theme.meta.id
  // `data-` reflects the active theme for debugging and for CSS hooks that
  // want to react to a specific built-in theme.
  el.dataset.themeAuthor = theme.meta.author
  const applied: AppliedTheme = { variables: vars, themeId: theme.meta.id }
  previous = applied
  return applied
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