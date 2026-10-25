/**
 * Build store.
 *
 * A **build** is what the player picks on the home screen: an isolated game
 * directory with its own Minecraft version, mod loader, memory and mods. This
 * store owns the list and the current selection; every mutation goes through a
 * Rust command and the returned summary replaces the stale one, so the UI never
 * derives state it could get wrong.
 */
import { computed, ref } from 'vue'
import { app, builds as buildsApi } from '@/lib/ipc'
import { persistSettings, settings } from '@/stores/app'
import type { BuildPatch, BuildSummary, CreateBuildRequest, LoaderCatalog } from '@/types'

const list = ref<BuildSummary[]>([])
const selectedSlug = ref<string | null>(null)
const loading = ref(false)
const loaded = ref(false)

/** Replace one entry in place, keeping list order stable. */
function replace(next: BuildSummary): void {
  const index = list.value.findIndex((b) => b.slug === next.slug)
  if (index === -1) list.value = [...list.value, next]
  else list.value = list.value.map((b) => (b.slug === next.slug ? next : b))
}

function pickDefault(entries: BuildSummary[]): string | null {
  // Prefer what was last played: that is almost always what the player wants.
  const played = entries
    .filter((b) => b.lastPlayedAt)
    .sort((a, b) => (b.lastPlayedAt ?? '').localeCompare(a.lastPlayedAt ?? ''))[0]
  return (played ?? entries[0])?.slug ?? null
}

/**
 * Remember the selection.
 *
 * Failing to persist it is not fatal — the build stays selected for this
 * session — so the error is swallowed rather than interrupting play.
 */
async function persistSelection(slug: string | null): Promise<void> {
  const current = settings.value
  if (!current || current.selected_build === slug) return
  try {
    const saved = await app.setSettings({ ...current, selected_build: slug })
    await persistSettings(saved)
  } catch {
    /* the selection still applies for this session */
  }
}

export async function refreshBuilds(): Promise<void> {
  loading.value = true
  try {
    const entries = await buildsApi.list()
    list.value = entries
    const saved = settings.value?.selected_build ?? null
    const stillThere = saved && entries.some((b) => b.slug === saved)
    selectedSlug.value = stillThere ? saved : pickDefault(entries)
    loaded.value = true
  } finally {
    loading.value = false
  }
}

export async function selectBuild(slug: string | null): Promise<void> {
  selectedSlug.value = slug
  await persistSelection(slug)
}

export async function createBuild(request: CreateBuildRequest): Promise<BuildSummary> {
  const created = await buildsApi.create(request)
  replace(created)
  selectedSlug.value = created.slug
  await persistSelection(created.slug)
  return created
}

export async function updateBuild(slug: string, patch: BuildPatch): Promise<BuildSummary> {
  const updated = await buildsApi.update(slug, patch)
  replace(updated)
  return updated
}

export async function deleteBuild(slug: string): Promise<void> {
  await buildsApi.remove(slug)
  list.value = list.value.filter((b) => b.slug !== slug)
  if (selectedSlug.value === slug) {
    selectedSlug.value = pickDefault(list.value)
    await persistSelection(selectedSlug.value)
  }
}

export async function pickBuildIcon(slug: string): Promise<BuildSummary | null> {
  const updated = await buildsApi.pickIcon(slug)
  if (updated) replace(updated)
  return updated
}

export async function clearBuildIcon(slug: string): Promise<BuildSummary> {
  const updated = await buildsApi.clearIcon(slug)
  replace(updated)
  return updated
}

export function loadersFor(gameVersion: string): Promise<LoaderCatalog> {
  return buildsApi.loaders(gameVersion)
}

export function useBuilds() {
  return {
    // Exposed as computeds rather than `readonly` refs: deep-readonly would make
    // every array in a build (`jvmArgs`, `gameArgs`) `readonly string[]`, which
    // the rest of the app cannot pass back to the backend.
    list: computed(() => list.value),
    selectedSlug: computed(() => selectedSlug.value),
    loading: computed(() => loading.value),
    loaded: computed(() => loaded.value),
    selected: computed(() => list.value.find((b) => b.slug === selectedSlug.value) ?? null),
    count: computed(() => list.value.length),
  }
}

export { list as buildList, loaded as buildsLoaded, selectedSlug }
