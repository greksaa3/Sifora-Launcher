import { computed, ref, shallowRef } from 'vue'

import { reportError, pushToast } from './app'
import { versions as versionApi } from '@/lib/ipc'
import type { ReleaseType, VersionGroups, VersionListResponse, VersionRow } from '@/types'

/**
 * Version catalogue.
 *
 * Mojang publishes ~900 versions; the full list lives in Rust and only a
 * compact row per version crosses the bridge. Everything here operates on that
 * array: grouping, filtering and search are plain array work with no caching
 * layer, because 900 strings are cheaper to re-filter than to invalidate.
 */

const rows = shallowRef<VersionRow[]>([])
const groups = ref<VersionGroups | null>(null)
const loading = ref(false)
const fromCache = ref(false)
const error = ref<string | null>(null)
const loadedAt = ref<string | null>(null)
const query = ref('')
const typeFilter = ref<ReleaseType | 'all'>('all')
const selectedId = ref<string | null>(null)
const selected = computed<VersionRow | null>(
  () => rows.value.find((r) => r.id === selectedId.value) ?? null,
)

const TYPE_LABEL: Record<ReleaseType, string> = {
  release: 'Релизы',
  snapshot: 'Снапшоты',
  old_beta: 'Beta',
  old_alpha: 'Alpha',
  other: 'Прочее',
}

/** Relevance score for search. Lower is better; `null` means "no match". */
function score(row: VersionRow, needle: string): number | null {
  const id = row.id.toLowerCase()
  const at = id.indexOf(needle)
  if (at === 0) return 0
  if (at > 0) return 1 + at / 1000
  // Fuzzy subsequence match, so "1204" finds "1.20.4".
  let cursor = 0
  for (const ch of id) {
    if (ch === needle[cursor]) cursor += 1
    if (cursor === needle.length) return 2
  }
  return null
}

const filtered = computed<VersionRow[]>(() => {
  const type = typeFilter.value
  const needle = query.value.trim().toLowerCase()
  const base = rows.value.filter((row) => type === 'all' || row.release_type === type)
  if (!needle) return base
  const scored: Array<{ row: VersionRow; rank: number }> = []
  for (const row of base) {
    const rank = score(row, needle)
    if (rank !== null) scored.push({ row, rank })
  }
  scored.sort((a, b) => a.rank - b.rank || a.row.id.localeCompare(b.row.id))
  return scored.map((entry) => entry.row)
})

/** Groups for the dropdown, with the most useful entries first. */
const sections = computed(() => {
  const list = filtered.value
  const out: Array<{ key: string; label: string; rows: VersionRow[] }> = []

  const pinned: VersionRow[] = []
  for (const id of [groups.value?.latestRelease, groups.value?.latestSnapshot]) {
    if (!id) continue
    const row = list.find((r) => r.id === id)
    if (row && !pinned.includes(row)) pinned.push(row)
  }
  if (pinned.length) {
    out.push({ key: 'pinned', label: 'Рекомендуемые', rows: pinned })
  }

  const installed = list.filter((r) => r.installed && !pinned.includes(r))
  if (installed.length) {
    out.push({ key: 'installed', label: 'Установленные', rows: installed })
  }

  const rest = list.filter((r) => !pinned.includes(r) && !r.installed)
  const byType: Array<ReleaseType> = ['release', 'snapshot', 'old_beta', 'old_alpha', 'other']
  for (const type of byType) {
    const group = rest.filter((r) => r.release_type === type)
    if (group.length) out.push({ key: type, label: TYPE_LABEL[type], rows: group })
  }
  return out
})

export function useVersionStore() {
  return {
    rows: computed(() => rows.value),
    filtered,
    sections,
    selected,
    selectedId,
    query,
    typeFilter,
    groups,
    loading,
    fromCache,
    error,
    loadedAt,
  }
}

export {
  error,
  loading,
  query,
  rows,
  sections,
  selected,
  selectedId,
  typeFilter,
}

export async function loadVersions(forceRefresh = false): Promise<void> {
  if (loading.value) return
  loading.value = true
  error.value = null
  try {
    const response: VersionListResponse = await versionApi.load(forceRefresh)
    rows.value = response.versions
    fromCache.value = response.fromCache
    loadedAt.value = response.loadedAt
    groups.value = await versionApi.groups().catch(() => null)
  } catch (e) {
    reportError(e)
    error.value = (e as Error)?.message ?? 'Не удалось получить список версий'
  } finally {
    loading.value = false
  }
}

export function selectVersion(id: string | null): void {
  selectedId.value = id
}

export function setTypeFilter(value: ReleaseType | 'all'): void {
  typeFilter.value = value
}

export function describeVersion(row: VersionRow): string {
  const bits: string[] = [row.type_label]
  if (row.java_major) bits.push(`Java ${row.java_major}`)
  if (row.installed) bits.push('установлена')
  return bits.join(' · ')
}

export function announceSelected(row: VersionRow | null): void {
  if (!row) return
  if (!row.supported) {
    pushToast('warning', `${row.id}: ${row.unsupported_reason}`)
  }
}