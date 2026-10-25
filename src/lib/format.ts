/** Small formatting helpers shared by the UI. */

const UNITS = ['Б', 'КБ', 'МБ', 'ГБ', 'ТБ']

/** Human readable byte count. Uses 1000-based units like Mojang does. */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 Б'
  let value = bytes
  let unit = 0
  while (value >= 1000 && unit < UNITS.length - 1) {
    value /= 1000
    unit += 1
  }
  const digits = value >= 100 || unit === 0 ? 0 : value >= 10 ? 1 : 2
  return `${value.toFixed(digits)} ${UNITS[unit]}`
}

const MONTHS = [
  'января',
  'февраля',
  'марта',
  'апреля',
  'мая',
  'июня',
  'июля',
  'августа',
  'сентября',
  'октября',
  'ноября',
  'декабря',
]

/**
 * Mojang timestamps are ISO-8601 with milliseconds. Rendering them ourselves
 * avoids pulling in a date library for four labels.
 */
export function formatReleaseDate(iso: string): string {
  if (!iso) return ''
  const date = new Date(iso)
  if (Number.isNaN(date.getTime())) return iso.slice(0, 10)
  return `${date.getUTCDate()} ${MONTHS[date.getUTCMonth()]} ${date.getUTCFullYear()}`
}

export function formatShortDate(iso: string): string {
  if (!iso) return ''
  const date = new Date(iso)
  if (Number.isNaN(date.getTime())) return ''
  const month = String(date.getUTCMonth() + 1).padStart(2, '0')
  const day = String(date.getUTCDate()).padStart(2, '0')
  return `${day}.${month}.${date.getUTCFullYear()}`
}

export function formatClock(iso: string): string {
  if (!iso) return ''
  const date = new Date(iso)
  if (Number.isNaN(date.getTime())) return ''
  return `${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`
}

/** Progress as a 0..1 fraction, safe when the total is unknown. */
export function fraction(done: number, total: number): number {
  if (!Number.isFinite(total) || total <= 0) return 0
  return Math.min(1, Math.max(0, done / total))
}

const COUNT_UNITS: Array<[number, string]> = [
  [1_000_000_000, 'млрд'],
  [1_000_000, 'млн'],
  [1_000, 'тыс.'],
]

/** Compact download/follower counts, e.g. `412 тыс.`. */
export function formatCount(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return '0'
  for (const [scale, suffix] of COUNT_UNITS) {
    if (value >= scale) {
      const scaled = value / scale
      // The comma is the decimal separator this UI already uses.
      return `${scaled >= 100 ? Math.round(scaled) : scaled.toFixed(1).replace('.', ',')} ${suffix}`
    }
  }
  return String(Math.round(value))
}