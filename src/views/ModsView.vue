<script setup lang="ts">
/**
 * Mods — a Modrinth browser for one build.
 *
 * The filters default to what the build can actually run: its Minecraft version
 * and its loader. That default is not cosmetic — Modrinth will happily return a
 * Fabric mod for a Forge build, and installing one produces a silently broken
 * game. The list can still be widened deliberately, and when a project has no
 * release for the build the panel says so instead of offering the wrong file.
 *
 * Everything remote arrives as plain values: no HTML from Modrinth is ever
 * rendered, and the only images are project icons from Modrinth's CDN, which is
 * the single remote host the window's CSP allows.
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { AlertTriangle, ArrowLeft, Download, Puzzle, Search, Trash2, X } from '@lucide/vue'

import { mods as modsApi } from '@/lib/ipc'
import { formatBytes, formatCount, formatShortDate } from '@/lib/format'
import { pushToast, reportError } from '@/stores/app'
import { MOD_LOADER_LABELS } from '@/types'
import type {
  BuildModContext,
  InstalledMod,
  ModDetails,
  ModSearchPage,
  ModSearchQuery,
  ModSummary,
  ModTag,
  ModTagBundle,
  ModVersion,
  ModIndex,
} from '@/types'

const props = defineProps<{ slug: string }>()

const PAGE = 20

/** Loaders a mod browser is genuinely useful for. */
const LOADER_CHOICES = ['fabric', 'forge', 'neoforge', 'quilt'] as const

/**
 * Sorts the UI offers.
 *
 * Kept as a closed list so an unvalidated string can never reach the request —
 * Rust falls back to relevance for anything it does not recognise, but the
 * renderer should not be the thing that decides that.
 */
const MOD_INDEXES: ModIndex[] = ['relevance', 'downloads', 'follows', 'newest', 'updated']

function asIndex(value: string): ModIndex | undefined {
  return (MOD_INDEXES as string[]).includes(value) ? (value as ModIndex) : undefined
}

const context = ref<BuildModContext | null>(null)
const tags = ref<ModTagBundle | null>(null)
const installed = ref<InstalledMod[]>([])
const installedContext = ref<{ canLoadMods: boolean; manualCount: number }>({
  canLoadMods: false,
  manualCount: 0,
})

const query = ref('')
const index = ref<string>('relevance')
const selectedCategories = ref<string[]>([])
const selectedLoaders = ref<string[]>([])
const selectedVersions = ref<string[]>([])
const offset = ref(0)

const page = ref<ModSearchPage | null>(null)
const searching = ref(false)
const searchError = ref<string | null>(null)

const showFilters = ref(false)

const detail = ref<ModDetails | null>(null)
const detailVersions = ref<ModVersion[]>([])
const detailLoading = ref(false)
const detailError = ref<string | null>(null)
const detailWidened = ref(false)

const installing = ref<string | null>(null)
const armedRemoval = ref<string | null>(null)

let debounce: ReturnType<typeof setTimeout> | null = null
let searchSeq = 0

/** Category chips, grouped by the heading Modrinth files them under. */
const categoryGroups = computed<Array<{ header: string; items: ModTag[] }>>(() => {
  const list = tags.value?.categories ?? []
  const mods = list.filter((tag) => tag.projectType === 'mod' || tag.projectType === '')
  const seen = new Map<string, ModTag[]>()
  for (const tag of mods) {
    const header = tag.header || 'Категории'
    const bucket = seen.get(header)
    if (bucket) bucket.push(tag)
    else seen.set(header, [tag])
  }
  // "categories" first: it is the one people actually browse by.
  return [...seen.entries()]
    .sort((a, b) => (a[0] === 'categories' ? -1 : b[0] === 'categories' ? 1 : 0))
    .map(([header, items]) => ({ header, items }))
})

const HEADERS: Record<string, string> = {
  categories: 'Категории',
  features: 'Возможности',
  resolutions: 'Разрешение',
  'performance impact': 'Влияние на производительность',
}

const sortOptions = computed(() => {
  const available = tags.value?.indexes ?? ['relevance', 'downloads', 'follows', 'newest', 'updated']
  return available.map((value) => ({ value, label: SORT_LABEL[value] ?? value }))
})

const SORT_LABEL: Record<string, string> = {
  relevance: 'По релевантности',
  downloads: 'По загрузкам',
  follows: 'По подпискам',
  newest: 'Сначала новые',
  updated: 'Недавно обновлённые',
}

const totalPages = computed(() => {
  const total = page.value?.totalHits ?? 0
  return Math.max(1, Math.ceil(total / PAGE))
})

const currentPage = computed(() => Math.floor(offset.value / PAGE) + 1)

const buildLoaderLabel = computed(() =>
  context.value ? MOD_LOADER_LABELS[context.value.loader] : '',
)

function toggle(list: string[], value: string): string[] {
  return list.includes(value) ? list.filter((v) => v !== value) : [...list, value]
}

async function loadTags(): Promise<void> {
  if (tags.value) return
  try {
    tags.value = await modsApi.tags()
  } catch (error) {
    reportError(error)
  }
}

async function loadInstalled(): Promise<void> {
  try {
    const list = await modsApi.list(props.slug)
    installed.value = list.mods
    installedContext.value = {
      canLoadMods: list.canLoadMods,
      manualCount: list.manualCount,
    }
  } catch (error) {
    reportError(error)
  }
}

/** Open a build: reset the browser to filters that match it. */
async function openBuild(): Promise<void> {
  context.value = null
  detail.value = null
  detailVersions.value = []
  query.value = ''
  selectedCategories.value = []
  offset.value = 0
  selectedLoaders.value = []
  selectedVersions.value = []

  try {
    const ctx = await modsApi.context(props.slug)
    // A slower response for a previous build must not apply here.
    if (ctx.slug !== props.slug) return
    context.value = ctx
    if (ctx.modrinthLoader) selectedLoaders.value = [ctx.modrinthLoader]
    if (ctx.gameVersion) selectedVersions.value = [ctx.gameVersion]
  } catch (error) {
    reportError(error)
  }

  await Promise.all([loadTags(), loadInstalled()])
  await runSearch(0)
}

function buildQuery(at: number): ModSearchQuery {
  return {
    query: query.value.trim(),
    index: asIndex(index.value),
    categories: selectedCategories.value,
    versions: selectedVersions.value,
    loaders: selectedLoaders.value,
    projectType: 'mod',
    limit: PAGE,
    offset: at,
  }
}

async function runSearch(at = 0): Promise<void> {
  searching.value = true
  searchError.value = null
  const forBuild = props.slug
  const seq = ++searchSeq
  try {
    const result = await modsApi.search(buildQuery(at))
    // An older in-flight request must never overwrite a newer result.
    if (seq !== searchSeq) return
    if (forBuild !== props.slug) return
    page.value = result
    offset.value = result.offset
  } catch (error) {
    if (seq !== searchSeq) return
    searchError.value = (error as { message?: string })?.message ?? 'Поиск не удался'
    page.value = null
  } finally {
    if (seq === searchSeq) searching.value = false
  }
}

function onQueryInput(): void {
  if (debounce) clearTimeout(debounce)
  // Modrinth is a shared public service; typing should not become a request
  // per keystroke.
  debounce = setTimeout(() => void runSearch(0), 350)
}

function applyFilters(): void {
  if (debounce) clearTimeout(debounce)
  void runSearch(0)
}

function clearFilters(): void {
  selectedCategories.value = []
  selectedLoaders.value = context.value?.modrinthLoader ? [context.value.modrinthLoader] : []
  selectedVersions.value = context.value?.gameVersion ? [context.value.gameVersion] : []
  applyFilters()
}

async function openDetail(hit: ModSummary): Promise<void> {
  detail.value = null
  detailVersions.value = []
  detailError.value = null
  detailLoading.value = true
  detailWidened.value = false
  const forBuild = props.slug
  try {
    const [project, versions] = await Promise.all([
      modsApi.project(hit.projectId),
      modsApi.versions({
        projectId: hit.projectId,
        gameVersion: context.value?.gameVersion,
        loader: context.value?.modrinthLoader ?? undefined,
      }),
    ])
    if (forBuild !== props.slug) return
    detail.value = project
    detailVersions.value = versions
  } catch (error) {
    detailError.value = (error as { message?: string })?.message ?? 'Не удалось открыть мод'
  } finally {
    detailLoading.value = false
  }
}

/** Widen the version list to everything the project published. */
async function widenVersions(): Promise<void> {
  const project = detail.value
  if (!project) return
  detailLoading.value = true
  try {
    detailVersions.value = await modsApi.versions({ projectId: project.projectId })
    detailWidened.value = true
  } catch (error) {
    reportError(error)
  } finally {
    detailLoading.value = false
  }
}

function closeDetail(): void {
  detail.value = null
  detailVersions.value = []
  detailError.value = null
}

async function install(version: ModVersion): Promise<void> {
  const project = detail.value
  if (!project) return
  installing.value = version.id
  try {
    await modsApi.install({
      slug: props.slug,
      projectId: project.projectId,
      versionId: version.id,
    })
    pushToast('success', `${project.title} ${version.versionNumber} установлен`)
    await loadInstalled()
  } catch (error) {
    reportError(error)
  } finally {
    installing.value = null
  }
}

async function remove(mod: InstalledMod): Promise<void> {
  armedRemoval.value = null
  try {
    const list = await modsApi.remove(props.slug, mod.filename)
    installed.value = list.mods
    installedContext.value = { canLoadMods: list.canLoadMods, manualCount: list.manualCount }
    pushToast('info', `${mod.title} удалён`)
  } catch (error) {
    reportError(error)
  }
}

function primaryFile(version: ModVersion) {
  return version.files.find((file) => file.primary) ?? version.files[0] ?? null
}

watch(() => props.slug, () => void openBuild())

onMounted(() => void openBuild())

onBeforeUnmount(() => {
  if (debounce) clearTimeout(debounce)
})
</script>

<template>
  <div class="sf-view">
    <section class="sf-card">
      <header class="sf-card__head">
        <h2 class="sf-card__title"><Puzzle :size="16" /> Моды из Modrinth</h2>
        <span class="sf-muted sf-mods__context">
          <template v-if="context">
            {{ context.name }} · Minecraft {{ context.gameVersion }}
            <template v-if="context.loader !== 'none'">
              · {{ buildLoaderLabel }} {{ context.loaderVersion }}
            </template>
          </template>
          <template v-else>загружаем сборку…</template>
        </span>
      </header>

      <p v-if="context && !installedContext.canLoadMods" class="sf-card__note">
        <AlertTriangle :size="13" /> Это Vanilla-сборка. Моды требуют Fabric, Forge
        или NeoForge — создайте сборку с загрузчиком.
      </p>

      <div class="sf-mods__search">
        <span class="sf-mods__searchbox">
          <Search :size="15" />
          <input
            v-model="query"
            class="sf-field sf-mods__input"
            type="search"
            placeholder="Поиск модов: sodium, JEI, шейдеры…"
            autocomplete="off"
            @input="onQueryInput"
            @keydown.enter.prevent="applyFilters"
          />
          <button
            v-if="query"
            class="sf-btn sf-btn--icon sf-btn--tiny"
            type="button"
            title="Очистить"
            @click="query = ''; applyFilters()"
          >
            <X :size="13" />
          </button>
        </span>

        <select v-model="index" class="sf-field sf-mods__select" @change="applyFilters">
          <option v-for="option in sortOptions" :key="option.value" :value="option.value">
            {{ option.label }}
          </option>
        </select>

        <button class="sf-btn sf-btn--ghost sf-btn--small" type="button" @click="showFilters = !showFilters">
          Фильтры
        </button>
      </div>

      <div v-if="showFilters" class="sf-mods__filters">
        <div class="sf-mods__filter-row">
          <label class="sf-mods__filter">
            <span class="sf-field-label">Загрузчик</span>
            <select
              class="sf-field"
              :value="selectedLoaders[0] ?? ''"
              @change="selectedLoaders = ($event.target as HTMLSelectElement).value ? [($event.target as HTMLSelectElement).value] : []; applyFilters()"
            >
              <option value="">Любой</option>
              <option v-for="value in LOADER_CHOICES" :key="value" :value="value">
                {{ value }}
              </option>
            </select>
          </label>

          <label class="sf-mods__filter">
            <span class="sf-field-label">Версия Minecraft</span>
            <select
              class="sf-field"
              :value="selectedVersions[0] ?? ''"
              @change="selectedVersions = ($event.target as HTMLSelectElement).value ? [($event.target as HTMLSelectElement).value] : []; applyFilters()"
            >
              <option value="">Любая</option>
              <option v-for="value in tags?.gameVersions ?? []" :key="value" :value="value">
                {{ value }}
              </option>
            </select>
          </label>

          <button class="sf-btn sf-btn--ghost sf-btn--small" type="button" @click="clearFilters">
            Сбросить к сборке
          </button>
        </div>

        <div v-for="group in categoryGroups" :key="group.header" class="sf-mods__cats">
          <span class="sf-field-label">{{ HEADERS[group.header] ?? group.header }}</span>
          <div class="sf-mods__chips">
            <button
              v-for="tag in group.items"
              :key="tag.name"
              class="sf-chip sf-chip--button"
              :class="{ 'is-active': selectedCategories.includes(tag.name) }"
              type="button"
              @click="selectedCategories = toggle(selectedCategories, tag.name); applyFilters()"
            >
              {{ tag.name }}
            </button>
          </div>
        </div>
      </div>

      <div class="sf-mods__body">
        <div class="sf-mods__list">
          <p v-if="searching" class="sf-muted sf-mods__status">Ищем…</p>
          <p v-else-if="searchError" class="sf-card__note"><AlertTriangle :size="13" /> {{ searchError }}</p>
          <p v-else-if="!page?.hits.length" class="sf-muted sf-mods__status">
            Ничего не нашлось. Попробуйте изменить фильтры или запрос.
          </p>

          <ul v-else class="sf-mods__results">
            <li v-for="hit in page.hits" :key="hit.projectId">
              <button
                class="sf-mod"
                :class="{ 'is-open': detail?.projectId === hit.projectId }"
                type="button"
                @click="openDetail(hit)"
              >
                <span class="sf-mod__icon" aria-hidden="true">
                  <img v-if="hit.iconUrl" :src="hit.iconUrl" alt="" loading="lazy" />
                  <Puzzle v-else :size="16" />
                </span>
                <span class="sf-mod__text">
                  <span class="sf-mod__title">{{ hit.title }}</span>
                  <span class="sf-mod__desc sf-muted">{{ hit.description }}</span>
                  <span class="sf-mod__meta sf-muted">
                    {{ hit.author }} · {{ formatCount(hit.downloads) }} загрузок ·
                    {{ formatShortDate(hit.updated) }}
                  </span>
                </span>
              </button>
            </li>
          </ul>

          <div v-if="page && page.totalHits > PAGE" class="sf-mods__pager">
            <button
              class="sf-btn sf-btn--small sf-btn--soft"
              type="button"
              :disabled="offset === 0 || searching"
              @click="runSearch(Math.max(0, offset - PAGE))"
            >
              Назад
            </button>
            <span class="sf-muted">{{ currentPage }} / {{ totalPages }}</span>
            <button
              class="sf-btn sf-btn--small sf-btn--soft"
              type="button"
              :disabled="offset + PAGE >= page.totalHits || searching"
              @click="runSearch(offset + PAGE)"
            >
              Вперёд
            </button>
          </div>
        </div>

        <aside v-if="detail || detailLoading" class="sf-mods__detail">
          <button class="sf-btn sf-btn--ghost sf-btn--small" type="button" @click="closeDetail">
            <ArrowLeft :size="14" /> К списку
          </button>

          <p v-if="detailLoading" class="sf-muted sf-mods__status">Загружаем карточку мода…</p>
          <p v-else-if="detailError" class="sf-card__note"><AlertTriangle :size="13" /> {{ detailError }}</p>

          <template v-else-if="detail">
            <div class="sf-mods__detail-head">
              <span class="sf-mod__icon sf-mod__icon--lg" aria-hidden="true">
                <img v-if="detail.iconUrl" :src="detail.iconUrl" alt="" />
                <Puzzle v-else :size="22" />
              </span>
              <div>
                <h3 class="sf-mods__detail-title">{{ detail.title }}</h3>
                <p class="sf-muted sf-mods__detail-sub">
                  {{ detail.description }}
                </p>
              </div>
            </div>

            <p class="sf-muted sf-mods__detail-stats">
              {{ formatCount(detail.downloads) }} загрузок · {{ formatCount(detail.followers) }} подписок
              · {{ detail.licenseName || 'лицензия не указана' }}
            </p>

            <div class="sf-mods__chips">
              <span v-for="tag in detail.categories" :key="tag" class="sf-chip">{{ tag }}</span>
            </div>

            <p v-if="detail.body" class="sf-mods__detail-body">{{ detail.body }}</p>

            <h4 class="sf-mods__detail-section">
              Версии
              <span v-if="!detailWidened && context" class="sf-muted">
                для Minecraft {{ context.gameVersion }}
                <template v-if="context.modrinthLoader"> ({{ context.modrinthLoader }})</template>
              </span>
              <span v-else class="sf-muted">все</span>
            </h4>

            <p v-if="!detailVersions.length" class="sf-card__note">
              Нет подходящих версий для этой сборки.
              <button class="sf-link" type="button" @click="widenVersions">
                Показать все версии мода
              </button>
            </p>

            <ul v-else class="sf-mods__versions">
              <li v-for="version in detailVersions" :key="version.id" class="sf-mods__version">
                <div class="sf-mods__version-main">
                  <span class="sf-mods__version-name">{{ version.versionNumber }}</span>
                  <span class="sf-muted sf-mods__version-meta">
                    {{ version.versionType }} ·
                    {{ version.loaders.join(', ') || 'без загрузчика' }} ·
                    {{ formatBytes(primaryFile(version)?.size ?? 0) }}
                  </span>
                </div>
                <button
                  class="sf-btn sf-btn--small"
                  type="button"
                  :disabled="installing === version.id || !primaryFile(version)"
                  @click="install(version)"
                >
                  <Download :size="14" />
                  {{ installing === version.id ? 'Ставим…' : 'Установить' }}
                </button>
              </li>
            </ul>
          </template>
        </aside>
      </div>
    </section>

    <section class="sf-card">
      <header class="sf-card__head">
        <h2 class="sf-card__title"><Download :size="16" /> Установленные моды</h2>
        <span class="sf-muted">{{ installed.length }}</span>
      </header>

      <p v-if="!installed.length" class="sf-muted sf-mods__status">
        В этой сборке пока нет модов.
      </p>

      <ul v-else class="sf-mods__installed">
        <li v-for="mod in installed" :key="mod.filename" class="sf-mods__installed-row">
          <span class="sf-mod__icon sf-mod__icon--sm" aria-hidden="true">
            <img v-if="mod.iconUrl" :src="mod.iconUrl" alt="" loading="lazy" />
            <Puzzle v-else :size="13" />
          </span>
          <span class="sf-mods__installed-text">
            <span class="sf-mods__installed-title">
              {{ mod.title }}
              <span v-if="mod.manual" class="sf-chip sf-chip--warn">вручную</span>
            </span>
            <span class="sf-muted sf-mods__installed-meta">
              {{ mod.manual ? mod.filename : mod.versionNumber }} · {{ formatBytes(mod.sizeBytes) }}
            </span>
          </span>

          <template v-if="armedRemoval === mod.filename">
            <button class="sf-btn sf-btn--danger sf-btn--small" type="button" @click="remove(mod)">
              Удалить
            </button>
            <button class="sf-btn sf-btn--soft sf-btn--small" type="button" @click="armedRemoval = null">
              Нет
            </button>
          </template>
          <button
            v-else
            class="sf-btn sf-btn--icon"
            type="button"
            title="Удалить мод"
            @click="armedRemoval = mod.filename"
          >
            <Trash2 :size="15" />
          </button>
        </li>
      </ul>

      <p v-if="installedContext.manualCount" class="sf-card__foot sf-muted">
        {{ installedContext.manualCount }} файлов были положены в папку mods вручную —
        Sifora их не устанавливала и не сможет обновлять.
      </p>
    </section>
  </div>
</template>

<style scoped>
@import './panel.css';

.sf-mods__context {
  font-size: 0.72rem;
  text-align: right;
}

.sf-mods__search {
  display: flex;
  gap: 0.4rem;
  align-items: center;
  margin-bottom: 0.6rem;
}

.sf-mods__searchbox {
  position: relative;
  display: flex;
  align-items: center;
  gap: 0.4rem;
  flex: 1;
  min-width: 0;
  padding: 0 0.5rem;
  border-radius: var(--sf-radius-sm);
  border: var(--sf-border-width) solid var(--sf-border);
  background: color-mix(in srgb, var(--sf-text) 5%, transparent);
  color: var(--sf-text-muted);
}

.sf-mods__input {
  flex: 1;
  min-width: 0;
  border: 0;
  background: transparent;
  padding: 0.4rem 0;
}

.sf-mods__input:focus {
  outline: none;
}

.sf-mods__select {
  flex: none;
  width: 12rem;
}

.sf-mods__filters {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  padding: 0.6rem;
  margin-bottom: 0.7rem;
  border-radius: var(--sf-radius-sm);
  background: color-mix(in srgb, var(--sf-text) 4%, transparent);
}

.sf-mods__filter-row {
  display: flex;
  align-items: flex-end;
  gap: 0.6rem;
  flex-wrap: wrap;
}

.sf-mods__filter {
  display: flex;
  flex-direction: column;
  gap: 0.2rem;
  min-width: 10rem;
}

.sf-field-label {
  font-size: 0.7rem;
  color: var(--sf-text-muted);
}

.sf-mods__cats {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.sf-mods__chips {
  display: flex;
  flex-wrap: wrap;
  gap: 0.25rem;
}

.sf-chip--button {
  cursor: pointer;
  border: var(--sf-border-width) solid var(--sf-border);
  background: transparent;
  color: inherit;
}

.sf-chip--button.is-active {
  background: color-mix(in srgb, var(--sf-accent) 26%, transparent);
  border-color: color-mix(in srgb, var(--sf-accent) 55%, transparent);
  color: var(--sf-text);
}

.sf-mods__body {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  gap: 0.8rem;
}

@media (min-width: 60rem) {
  .sf-mods__body:has(.sf-mods__detail) {
    grid-template-columns: minmax(0, 1.1fr) minmax(0, 1fr);
  }
}

.sf-mods__status {
  margin: 0;
  padding: 0.8rem 0.4rem;
  font-size: 0.78rem;
  text-align: center;
}

.sf-mods__results,
.sf-mods__versions,
.sf-mods__installed {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.sf-mods__results {
  max-height: 30rem;
  overflow-y: auto;
}

.sf-mod {
  display: flex;
  align-items: flex-start;
  gap: 0.55rem;
  width: 100%;
  padding: 0.45rem;
  border: var(--sf-border-width) solid transparent;
  border-radius: var(--sf-radius);
  background: color-mix(in srgb, var(--sf-text) 5%, transparent);
  color: inherit;
  cursor: pointer;
  text-align: left;
  transition: background-color var(--sf-dur-fast) var(--sf-ease);
}

.sf-mod:hover {
  background: color-mix(in srgb, var(--sf-text) 9%, transparent);
}

.sf-mod.is-open {
  border-color: color-mix(in srgb, var(--sf-accent) 55%, transparent);
}

.sf-mod__icon {
  flex: none;
  display: grid;
  place-items: center;
  width: 2.1rem;
  height: 2.1rem;
  border-radius: var(--sf-radius-sm);
  overflow: hidden;
  background: color-mix(in srgb, var(--sf-accent) 14%, transparent);
  color: var(--sf-accent);
}

.sf-mod__icon--sm {
  width: 1.6rem;
  height: 1.6rem;
}

.sf-mod__icon--lg {
  width: 3rem;
  height: 3rem;
}

.sf-mod__icon img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.sf-mod__text {
  display: flex;
  flex-direction: column;
  gap: 0.1rem;
  min-width: 0;
}

.sf-mod__title {
  font-size: 0.85rem;
  font-weight: var(--sf-weight-bold);
}

.sf-mod__desc {
  font-size: 0.72rem;
  line-height: 1.35;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.sf-mod__meta {
  font-size: 0.68rem;
}

.sf-mods__pager {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.6rem;
  margin-top: 0.6rem;
  font-size: 0.74rem;
}

.sf-mods__detail {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  padding: 0.6rem;
  border-radius: var(--sf-radius);
  background: color-mix(in srgb, var(--sf-text) 4%, transparent);
  max-height: 34rem;
  overflow-y: auto;
}

.sf-mods__detail-head {
  display: flex;
  gap: 0.6rem;
  align-items: flex-start;
}

.sf-mods__detail-title {
  margin: 0;
  font-size: 0.95rem;
  font-weight: var(--sf-weight-bold);
}

.sf-mods__detail-sub,
.sf-mods__detail-stats {
  margin: 0.1rem 0 0;
  font-size: 0.72rem;
  line-height: 1.4;
}

.sf-mods__detail-body {
  margin: 0;
  font-size: 0.74rem;
  line-height: 1.5;
  white-space: pre-wrap;
  max-height: 12rem;
  overflow-y: auto;
  color: var(--sf-text-muted);
}

.sf-mods__detail-section {
  margin: 0.3rem 0 0;
  font-size: 0.8rem;
  font-weight: var(--sf-weight-bold);
}

.sf-mods__version {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.5rem;
  padding: 0.35rem 0.45rem;
  border-radius: var(--sf-radius-sm);
  background: color-mix(in srgb, var(--sf-text) 5%, transparent);
}

.sf-mods__version-main {
  display: flex;
  flex-direction: column;
  gap: 0.05rem;
  min-width: 0;
}

.sf-mods__version-name {
  font-size: 0.78rem;
  font-weight: var(--sf-weight-bold);
}

.sf-mods__version-meta {
  font-size: 0.68rem;
}

.sf-mods__installed-row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.35rem 0.45rem;
  border-radius: var(--sf-radius);
  background: color-mix(in srgb, var(--sf-text) 5%, transparent);
}

.sf-mods__installed-text {
  display: flex;
  flex-direction: column;
  gap: 0.05rem;
  min-width: 0;
  flex: 1;
}

.sf-mods__installed-title {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  font-size: 0.82rem;
  font-weight: var(--sf-weight-bold);
}

.sf-mods__installed-meta {
  font-size: 0.68rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.sf-btn--tiny {
  padding: 0.2rem;
}

.sf-link {
  border: 0;
  background: none;
  padding: 0;
  color: var(--sf-accent);
  font-size: 0.74rem;
  cursor: pointer;
  text-decoration: underline;
}
</style>
