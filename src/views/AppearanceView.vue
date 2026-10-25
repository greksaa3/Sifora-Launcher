<script setup lang="ts">
/**
 * Appearance.
 *
 * Three parts, in order of how often they are used:
 *
 * 1. **Background** — the user's own image, on top of the theme's own backdrop.
 * 2. **Gallery** — apply, edit, export, import, delete, undo, reset.
 * 3. **Editor** — every field of the closed theme schema, edited by hand.
 *
 * There is deliberately no generator here. A theme is a design decision: an
 * imported file and a hand-made one go through exactly the same validation, and
 * nothing can enter the schema that the schema does not have a field for.
 *
 * Live preview works by asking Rust to validate the draft and returning the
 * result, which is written to CSS custom properties. Nothing is persisted until
 * Save, so an abandoned edit leaves no trace.
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import {
  ArrowDown,
  ArrowUp,
  Check,
  Download,
  Image as ImageIcon,
  ImagePlus,
  Loader,
  Pencil,
  Plus,
  RefreshCw,
  RotateCcw,
  Save,
  Trash2,
  Undo2,
  Upload,
  X,
} from '@lucide/vue'

import { backgrounds, themes as themeApi, toAssetUrl } from '@/lib/ipc'
import { pushToast, reportError } from '@/stores/app'
import {
  ARTWORK_LABELS,
  ARTWORK_VALUES,
  BACKGROUND_KINDS,
  BACKGROUND_KIND_LABELS,
  COMPONENT_STYLES,
  COMPONENT_STYLE_LABELS,
  CORNERS_LABELS,
  CORNERS_VALUES,
  EASINGS,
  EASING_LABELS,
  FONT_FAMILIES,
  FONT_LABELS,
  IMAGE_FITS,
  IMAGE_FIT_LABELS,
  LABEL_CASES,
  LABEL_CASE_LABELS,
  MOTION_STYLES,
  MOTION_STYLE_LABELS,
  PATTERNS,
  PATTERN_LABELS,
  PLACEMENTS,
  PLACEMENT_LABELS,
  REDUCED_MOTION_MODES,
  SECTION_LABELS,
  SHORTCUT_LABELS,
  SHORTCUT_VALUES,
} from '@/types'
import type {
  BackgroundImage,
  HexColor,
  Palette,
  ThemeSpec,
  ThemeSummary,
} from '@/types'

const props = defineProps<{
  catalog: ThemeSummary[]
  activeId: string
  canUndo: boolean
  images: BackgroundImage[]
  presetId: string | null
  imageId: string | null
}>()

const emit = defineEmits<{
  (e: 'applied', theme: ThemeSpec): void
  (e: 'preset-changed', id: string): void
  (e: 'image-changed', image: BackgroundImage | null): void
  (e: 'images-changed', images: BackgroundImage[]): void
  (e: 'catalog-changed', themes: ThemeSummary[]): void
}>()

/* ------------------------------------------------------------------ colours */

type PaletteColorKey = Exclude<keyof Palette, 'elevation'>

const PALETTE_FIELDS: Array<{ key: PaletteColorKey; label: string }> = [
  { key: 'accent', label: 'Акцент' },
  { key: 'accent_hover', label: 'Акцент при наведении' },
  { key: 'on_accent', label: 'Текст на акценте' },
  { key: 'surface', label: 'Поверхность' },
  { key: 'surface_raised', label: 'Приподнятая поверхность' },
  { key: 'surface_sunken', label: 'Утопленная поверхность' },
  { key: 'text', label: 'Текст' },
  { key: 'text_muted', label: 'Приглушённый текст' },
  { key: 'border', label: 'Границы' },
  { key: 'success', label: 'Успех' },
  { key: 'warning', label: 'Предупреждение' },
  { key: 'danger', label: 'Ошибка' },
  { key: 'shadow', label: 'Тень' },
]

/**
 * Colours cross the bridge as `#rrggbb` or `#rrggbbaa`, and the Rust type
 * round-trips alpha through that form. Splitting and rejoining them here means
 * editing a translucent surface does not silently make it opaque.
 */
function splitColor(value: unknown): { hex: string; alpha: number } {
  const raw = typeof value === 'string' ? value.trim().replace(/^#/, '') : ''
  if (raw.length === 8) {
    const alpha = Number.parseInt(raw.slice(6, 8), 16)
    return { hex: `#${raw.slice(0, 6)}`, alpha: Number.isFinite(alpha) ? alpha / 255 : 1 }
  }
  if (raw.length === 6 || raw.length === 3) return { hex: `#${raw}`, alpha: 1 }
  return { hex: '#000000', alpha: 1 }
}

function joinColor(hex: string, alpha: number): string {
  const rgb = (hex.replace(/^#/, '') + '000000').slice(0, 6).toLowerCase()
  if (alpha >= 0.999) return `#${rgb}`
  const byte = Math.round(Math.min(1, Math.max(0, alpha)) * 255)
  return `#${rgb}${byte.toString(16).padStart(2, '0')}`
}

const colorHex = (value: unknown): string => splitColor(value).hex
const colorAlpha = (value: unknown): number => splitColor(value).alpha

/* --------------------------------------------------------------- background */

const picking = ref(false)

async function pickImage(): Promise<void> {
  picking.value = true
  try {
    const picked = await backgrounds.pick()
    if (!picked) return
    emit('image-changed', picked)
    pushToast('success', 'Фон загружен', picked.fileName)
  } catch (error) {
    reportError(error)
  } finally {
    picking.value = false
  }
}

async function removeImage(image: BackgroundImage): Promise<void> {
  try {
    await backgrounds.remove(image.id)
    emit('images-changed', props.images.filter((i) => i.id !== image.id))
    if (props.imageId === image.id) emit('image-changed', null)
    pushToast('info', 'Фон удалён')
  } catch (error) {
    reportError(error)
  }
}

function useImage(image: BackgroundImage): void {
  emit('image-changed', image)
}

/* ------------------------------------------------------------------ gallery */

const busy = ref(false)
/** Undo availability as the backend reports it; the prop is only a seed. */
const undoAvailable = ref(props.canUndo)

async function refreshCatalog(): Promise<void> {
  try {
    const list = await themeApi.list()
    emit('catalog-changed', list.themes)
    undoAvailable.value = list.canUndo
  } catch {
    // The parent's own list is still usable; a refresh failure is not fatal.
  }
}

onMounted(() => void refreshCatalog())

async function applyById(id: string): Promise<void> {
  if (id === props.activeId) return
  try {
    const theme = await themeApi.applyById(id)
    emit('applied', theme)
    pushToast('info', `Тема «${theme.meta.name}» применена`)
  } catch (error) {
    reportError(error)
  }
}

async function undo(): Promise<void> {
  try {
    const theme = await themeApi.undo()
    emit('applied', theme)
    pushToast('info', `Возвращена тема «${theme.meta.name}»`)
  } catch (error) {
    reportError(error)
  }
}

async function reset(): Promise<void> {
  try {
    const theme = await themeApi.reset()
    emit('applied', theme)
    pushToast('info', 'Восстановлена исходная тема')
  } catch (error) {
    reportError(error)
  }
}

const armedDelete = ref<string | null>(null)

async function remove(theme: ThemeSummary): Promise<void> {
  armedDelete.value = null
  try {
    await themeApi.remove(theme.id)
    pushToast('info', `Тема «${theme.name}» удалена`)
    if (draft.value?.meta.id === theme.id) closeEditor()
    await refreshCatalog()
  } catch (error) {
    reportError(error)
  }
}

async function exportTheme(id: string): Promise<void> {
  try {
    const path = await themeApi.exportTheme(id)
    if (path) pushToast('success', 'Тема сохранена в файл', path)
  } catch (error) {
    reportError(error)
  }
}

async function importTheme(): Promise<void> {
  try {
    const imported = await themeApi.importTheme()
    if (!imported) return
    pushToast('success', `Тема «${imported.name}» импортирована`)
    await refreshCatalog()
  } catch (error) {
    reportError(error)
  }
}

/* ------------------------------------------------------------------- editor */

const draft = ref<ThemeSpec | null>(null)
const saving = ref(false)
const editing = computed(() => draft.value !== null)

let previewTimer: ReturnType<typeof setTimeout> | null = null

async function edit(id: string): Promise<void> {
  busy.value = true
  try {
    draft.value = await themeApi.get(id)
  } catch (error) {
    reportError(error)
  } finally {
    busy.value = false
  }
}

function closeEditor(): void {
  if (previewTimer) clearTimeout(previewTimer)
  draft.value = null
  void (async () => {
    // The preview must be cleared *before* the real theme is re-applied, or a
    // preview still in flight could paint the abandoned draft back on screen.
    try {
      await themeApi.clearPreview()
    } catch {
      // Nothing to clear.
    }
    await restoreActive()
  })()
}

/** Put the really-applied theme back on screen after abandoning a draft. */
async function restoreActive(): Promise<void> {
  try {
    emit('applied', await themeApi.get(props.activeId))
  } catch {
    // Nothing to restore is not a problem worth reporting.
  }
}

/** Validate and show the draft without committing it. */
async function schedulePreview(): Promise<void> {
  if (previewTimer) clearTimeout(previewTimer)
  previewTimer = setTimeout(async () => {
    const spec = draft.value
    if (!spec) return
    try {
      const result = await themeApi.preview(spec)
      // Rust clamps on the way through, and showing its answer is the only way
      // the preview and the saved result agree. The draft itself is *not*
      // rewritten with the reply: that would re-trigger this watcher and the
      // two would trade IPC calls forever.
      emit('applied', result.theme)
    } catch {
      // A half-typed draft (an empty number field, say) is not a failure the
      // user needs a toast for: the last valid preview simply stays on screen,
      // and pressing Save reports the real problem.
    }
  }, 250)
}

watch(draft, () => void schedulePreview(), { deep: true })

/** Names must stay unique, or a gallery becomes unusable. */
function uniqueName(base: string): string {
  const trimmed = base.trim() || 'Своя тема'
  const taken = new Set(props.catalog.map((t) => t.name.toLowerCase()))
  if (!taken.has(trimmed.toLowerCase())) return trimmed
  for (let n = 2; n < 200; n += 1) {
    const candidate = `${trimmed} (${n})`
    if (!taken.has(candidate.toLowerCase())) return candidate
  }
  return `${trimmed} ${Date.now()}`
}

async function save(): Promise<void> {
  const spec = draft.value
  if (!spec) return
  saving.value = true
  try {
    if (spec.meta.builtin) {
      // A built-in cannot be overwritten, so the edit becomes a new theme —
      // and it is made active, exactly like a saved custom one.
      const created = await themeApi.derive(spec, uniqueName(spec.meta.name))
      const applied = await themeApi.applyById(created.id)
      emit('applied', applied)
      pushToast('success', `Создана тема «${created.name}»`)
    } else {
      const summary = await themeApi.apply(spec)
      emit('applied', spec)
      pushToast('success', `Тема «${summary.name}» сохранена`)
    }
    await refreshCatalog()
    await themeApi.clearPreview()
    draft.value = null
  } catch (error) {
    reportError(error)
  } finally {
    saving.value = false
  }
}

/** Work on a copy of the active theme without picking one from the gallery. */
async function editActive(): Promise<void> {
  await edit(props.activeId)
}

/* --------------------------------------------------------------- mutations */

function palette(key: PaletteColorKey): string {
  const value = draft.value?.tokens.palette[key]
  return typeof value === 'string' ? value : '#000000'
}

function setPaletteHex(key: PaletteColorKey, hex: string): void {
  if (!draft.value) return
  draft.value.tokens.palette[key] = joinColor(hex, colorAlpha(palette(key)))
}

function setPaletteAlpha(key: PaletteColorKey, alpha: number): void {
  if (!draft.value) return
  draft.value.tokens.palette[key] = joinColor(colorHex(palette(key)), alpha)
}

function background(): HexColor[] {
  return draft.value?.tokens.background.colors ?? []
}

function setBackgroundColor(index: number, hex: string): void {
  const spec = draft.value
  if (!spec) return
  const current = spec.tokens.background.colors[index] ?? '#000000'
  spec.tokens.background.colors[index] = joinColor(hex, colorAlpha(current))
}

function setBackgroundAlpha(index: number, alpha: number): void {
  const spec = draft.value
  if (!spec) return
  const current = spec.tokens.background.colors[index] ?? '#000000'
  spec.tokens.background.colors[index] = joinColor(colorHex(current), alpha)
}

/** Point the theme itself at one of the user's pictures. */
function onThemeImageChange(event: Event): void {
  const spec = draft.value
  if (!spec) return
  const path = (event.target as HTMLSelectElement).value
  spec.tokens.background.image = path ? { kind: 'local', path } : null
  if (path) spec.tokens.background.kind = 'image'
}

function addStop(): void {
  const spec = draft.value
  if (!spec || spec.tokens.background.colors.length >= 5) return
  const last = spec.tokens.background.colors.at(-1) ?? '#101010'
  spec.tokens.background.colors.push(last)
}

function removeStop(index: number): void {
  const spec = draft.value
  if (!spec || spec.tokens.background.colors.length <= 1) return
  spec.tokens.background.colors.splice(index, 1)
}

function moveOrder(index: number, delta: number): void {
  const spec = draft.value
  if (!spec) return
  const order = spec.tokens.layout.order
  const target = index + delta
  if (target < 0 || target >= order.length) return
  const [item] = order.splice(index, 1)
  order.splice(target, 0, item)
}

function toggleHidden(section: (typeof SHORTCUT_VALUES)[number] | string): void {
  const spec = draft.value
  if (!spec) return
  const hidden = spec.tokens.layout.hidden
  const value = section as (typeof hidden)[number]
  const at = hidden.indexOf(value)
  if (at === -1) hidden.push(value)
  else hidden.splice(at, 1)
}

function toggleShortcut(shortcut: (typeof SHORTCUT_VALUES)[number]): void {
  const spec = draft.value
  if (!spec) return
  const list = spec.tokens.layout.shortcuts
  const at = list.indexOf(shortcut)
  if (at === -1) list.push(shortcut)
  else list.splice(at, 1)
}

onBeforeUnmount(() => {
  if (previewTimer) clearTimeout(previewTimer)
})
</script>

<template>
  <div class="sf-view">
    <!-- ------------------------------------------------------------ gallery -->
    <section class="sf-card">
      <header class="sf-card__head">
        <h2 class="sf-card__title"><ImageIcon :size="16" /> Темы</h2>
        <div class="sf-card__actions">
          <button class="sf-btn sf-btn--small" type="button" @click="importTheme">
            <Upload :size="14" /> Импорт
          </button>
          <button
            class="sf-btn sf-btn--ghost sf-btn--small"
            type="button"
            :disabled="!undoAvailable"
            @click="undo"
          >
            <Undo2 :size="14" /> Отменить
          </button>
          <button class="sf-btn sf-btn--ghost sf-btn--small" type="button" @click="reset">
            <RotateCcw :size="14" /> По умолчанию
          </button>
        </div>
      </header>

      <p class="sf-card__lead sf-muted">
        Тема — это набор значений: цвета, форма, шрифт, узор, движение и
        расположение разделов. Любую тему можно изменить вручную или выгрузить в
        файл и передать другому.
      </p>

      <ul class="sf-themes">
        <li
          v-for="theme in catalog"
          :key="theme.id"
          class="sf-theme"
          :class="{ 'is-active': theme.id === activeId }"
        >
          <button class="sf-theme__swatch" type="button" @click="applyById(theme.id)">
            <span class="sf-theme__dots" aria-hidden="true">
              <span :style="{ background: theme.background }" />
              <span :style="{ background: theme.accent }" />
            </span>
            <span class="sf-theme__text">
              <span class="sf-theme__name">
                {{ theme.name }}
                <Check v-if="theme.id === activeId" :size="13" />
              </span>
              <span class="sf-theme__desc sf-muted">{{ theme.description }}</span>
            </span>
          </button>

          <div class="sf-theme__actions">
            <button
              class="sf-btn sf-btn--icon"
              type="button"
              title="Изменить вручную"
              @click="edit(theme.id)"
            >
              <Pencil :size="15" />
            </button>
            <button
              class="sf-btn sf-btn--icon"
              type="button"
              title="Выгрузить в файл"
              @click="exportTheme(theme.id)"
            >
              <Download :size="15" />
            </button>
            <template v-if="!theme.builtin">
              <template v-if="armedDelete === theme.id">
                <button
                  class="sf-btn sf-btn--danger sf-btn--small"
                  type="button"
                  @click="remove(theme)"
                >
                  Удалить
                </button>
                <button
                  class="sf-btn sf-btn--soft sf-btn--small"
                  type="button"
                  @click="armedDelete = null"
                >
                  Нет
                </button>
              </template>
              <button
                v-else
                class="sf-btn sf-btn--icon"
                type="button"
                title="Удалить тему"
                @click="armedDelete = theme.id"
              >
                <Trash2 :size="15" />
              </button>
            </template>
          </div>
        </li>
      </ul>

      <div class="sf-themes__foot">
        <button class="sf-btn sf-btn--small" type="button" @click="editActive">
          <Pencil :size="14" /> Изменить текущую
        </button>
        <span v-if="busy" class="sf-muted sf-themes__busy">
          <Loader :size="13" class="sf-spin" /> Открываем…
        </span>
      </div>
    </section>

    <!-- ------------------------------------------------------------- editor -->
    <section v-if="editing && draft" class="sf-card">
      <header class="sf-card__head">
        <h2 class="sf-card__title"><Pencil :size="16" /> Редактор темы</h2>
        <button class="sf-btn sf-btn--icon" type="button" title="Закрыть" @click="closeEditor">
          <X :size="15" />
        </button>
      </header>

      <p v-if="draft.meta.builtin" class="sf-card__note">
        Это встроенная тема. При сохранении будет создана своя копия — исходную
        изменить нельзя.
      </p>

      <div class="sf-editor__meta">
        <label class="sf-field-wrap">
          <span class="sf-field-label">Название</span>
          <input v-model="draft.meta.name" class="sf-field" maxlength="48" />
        </label>
        <label class="sf-field-wrap">
          <span class="sf-field-label">Описание</span>
          <input v-model="draft.meta.description" class="sf-field" maxlength="240" />
        </label>
      </div>

      <details class="sf-group" open>
        <summary>Палитра</summary>
        <div class="sf-colors">
          <div v-for="field in PALETTE_FIELDS" :key="field.key" class="sf-color">
            <input
              class="sf-color__pick"
              type="color"
              :value="colorHex(palette(field.key))"
              :aria-label="field.label"
              @input="setPaletteHex(field.key, ($event.target as HTMLInputElement).value)"
            />
            <span class="sf-color__label">{{ field.label }}</span>
            <input
              class="sf-color__alpha"
              type="range"
              min="0"
              max="1"
              step="0.01"
              :value="colorAlpha(palette(field.key))"
              aria-label="Прозрачность"
              @input="setPaletteAlpha(field.key, Number(($event.target as HTMLInputElement).value))"
            />
            <span class="sf-color__value sf-mono sf-muted">{{ palette(field.key) }}</span>
          </div>
        </div>
        <label class="sf-field-wrap sf-editor__inline">
          <span class="sf-field-label">Выраженность панелей {{ draft.tokens.palette.elevation.toFixed(2) }}</span>
          <input
            v-model.number="draft.tokens.palette.elevation"
            type="range"
            min="0"
            max="1"
            step="0.05"
          />
        </label>
      </details>

      <details class="sf-group">
        <summary>Форма</summary>
        <div class="sf-grid-3">
          <label class="sf-field-wrap">
            <span class="sf-field-label">Радиус малый</span>
            <input v-model.number="draft.tokens.shape.radius_sm" class="sf-field" type="number" min="0" max="48" />
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Радиус</span>
            <input v-model.number="draft.tokens.shape.radius" class="sf-field" type="number" min="0" max="64" />
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Радиус большой</span>
            <input v-model.number="draft.tokens.shape.radius_lg" class="sf-field" type="number" min="0" max="96" />
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Радиус «пилюли»</span>
            <input v-model.number="draft.tokens.shape.radius_pill" class="sf-field" type="number" min="0" max="999" />
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Толщина границ</span>
            <input v-model.number="draft.tokens.shape.border_width" class="sf-field" type="number" min="0" max="6" />
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Толщина обводки</span>
            <input v-model.number="draft.tokens.shape.ring_width" class="sf-field" type="number" min="0" max="12" />
          </label>
        </div>
      </details>

      <details class="sf-group">
        <summary>Типографика</summary>
        <div class="sf-grid-3">
          <label class="sf-field-wrap">
            <span class="sf-field-label">Основной шрифт</span>
            <select v-model="draft.tokens.typography.family" class="sf-field">
              <option v-for="font in FONT_FAMILIES" :key="font" :value="font">
                {{ FONT_LABELS[font] }}
              </option>
            </select>
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Шрифт заголовков</span>
            <select v-model="draft.tokens.typography.display_family" class="sf-field">
              <option v-for="font in FONT_FAMILIES" :key="font" :value="font">
                {{ FONT_LABELS[font] }}
              </option>
            </select>
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Размер, множитель</span>
            <input
              v-model.number="draft.tokens.typography.scale"
              class="sf-field"
              type="number"
              min="0.8"
              max="1.35"
              step="0.01"
            />
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Вес обычного</span>
            <input
              v-model.number="draft.tokens.typography.weight_regular"
              class="sf-field"
              type="number"
              min="100"
              max="700"
              step="10"
            />
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Вес жирного</span>
            <input
              v-model.number="draft.tokens.typography.weight_bold"
              class="sf-field"
              type="number"
              min="100"
              max="900"
              step="10"
            />
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Межбуквенный интервал</span>
            <input
              v-model.number="draft.tokens.typography.letter_spacing"
              class="sf-field"
              type="number"
              min="-0.05"
              max="0.2"
              step="0.01"
            />
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Межстрочный интервал</span>
            <input
              v-model.number="draft.tokens.typography.line_height"
              class="sf-field"
              type="number"
              min="1"
              max="1.8"
              step="0.05"
            />
          </label>
          <label class="sf-check">
            <input v-model="draft.tokens.typography.uppercase_labels" type="checkbox" />
            <span>Подписи заглавными</span>
          </label>
        </div>
      </details>

      <details class="sf-group">
        <summary>Компоненты</summary>
        <div class="sf-grid-3">
          <label class="sf-field-wrap">
            <span class="sf-field-label">Кнопки</span>
            <select v-model="draft.tokens.components.button" class="sf-field">
              <option v-for="style in COMPONENT_STYLES" :key="style" :value="style">
                {{ COMPONENT_STYLE_LABELS[style] }}
              </option>
            </select>
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Поля ввода</span>
            <select v-model="draft.tokens.components.input" class="sf-field">
              <option v-for="style in COMPONENT_STYLES" :key="style" :value="style">
                {{ COMPONENT_STYLE_LABELS[style] }}
              </option>
            </select>
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Карточки</span>
            <select v-model="draft.tokens.components.card" class="sf-field">
              <option v-for="style in COMPONENT_STYLES" :key="style" :value="style">
                {{ COMPONENT_STYLE_LABELS[style] }}
              </option>
            </select>
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">
              Прозрачность {{ draft.tokens.components.translucency.toFixed(2) }}
            </span>
            <input
              v-model.number="draft.tokens.components.translucency"
              type="range"
              min="0"
              max="1"
              step="0.02"
            />
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Плотность, px</span>
            <input v-model.number="draft.tokens.components.density" class="sf-field" type="number" min="0" max="40" />
          </label>
        </div>
      </details>

      <details class="sf-group">
        <summary>Фон и декор</summary>
        <div class="sf-grid-3">
          <label class="sf-field-wrap">
            <span class="sf-field-label">Тип фона</span>
            <select v-model="draft.tokens.background.kind" class="sf-field">
              <option v-for="kind in BACKGROUND_KINDS" :key="kind" :value="kind">
                {{ BACKGROUND_KIND_LABELS[kind] }}
              </option>
            </select>
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Угол, °</span>
            <input
              v-model.number="draft.tokens.background.angle"
              class="sf-field"
              type="number"
              min="0"
              max="360"
              step="1"
            />
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">
              Затемнение {{ draft.tokens.background.dim.toFixed(2) }}
            </span>
            <input
              v-model.number="draft.tokens.background.dim"
              type="range"
              min="-0.6"
              max="0.6"
              step="0.05"
            />
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Рисунок</span>
            <select v-model="draft.tokens.decor.pattern" class="sf-field">
              <option v-for="pattern in PATTERNS" :key="pattern" :value="pattern">
                {{ PATTERN_LABELS[pattern] }}
              </option>
            </select>
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Задник</span>
            <select v-model="draft.tokens.background.artwork" class="sf-field">
              <option v-for="art in ARTWORK_VALUES" :key="art" :value="art">
                {{ ARTWORK_LABELS[art] }}
              </option>
            </select>
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">
              Сила задника {{ draft.tokens.background.artwork_strength.toFixed(2) }}
            </span>
            <input
              v-model.number="draft.tokens.background.artwork_strength"
              type="range"
              min="0"
              max="1"
              step="0.02"
            />
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Картинка внутри темы</span>
            <select
              class="sf-field"
              :value="draft.tokens.background.image?.path ?? ''"
              @change="onThemeImageChange"
            >
              <option value="">Без картинки</option>
              <option v-for="image in images" :key="image.id" :value="image.path">
                {{ image.fileName }}
              </option>
            </select>
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Как вписать картинку фона</span>
            <select v-model="draft.tokens.background.image_fit" class="sf-field">
              <option v-for="fit in IMAGE_FITS" :key="fit" :value="fit">
                {{ IMAGE_FIT_LABELS[fit] }}
              </option>
            </select>
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">
              Прозрачность картинки {{ draft.tokens.background.image_opacity.toFixed(2) }}
            </span>
            <input
              v-model.number="draft.tokens.background.image_opacity"
              type="range"
              min="0"
              max="1"
              step="0.02"
            />
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">
              Размытие картинки {{ draft.tokens.background.image_blur.toFixed(1) }} px
            </span>
            <input
              v-model.number="draft.tokens.background.image_blur"
              type="range"
              min="0"
              max="40"
              step="0.5"
              :disabled="!draft.tokens.background.image"
            />
          </label>
        </div>

        <div class="sf-colors">
          <div v-for="(stop, index) in background()" :key="index" class="sf-color">
            <input
              class="sf-color__pick"
              type="color"
              :value="colorHex(stop)"
              :aria-label="`Цвет ${index + 1}`"
              @input="setBackgroundColor(index, ($event.target as HTMLInputElement).value)"
            />
            <span class="sf-color__label">Цвет {{ index + 1 }}</span>
            <input
              class="sf-color__alpha"
              type="range"
              min="0"
              max="1"
              step="0.01"
              :value="colorAlpha(stop)"
              aria-label="Прозрачность"
              @input="setBackgroundAlpha(index, Number(($event.target as HTMLInputElement).value))"
            />
            <button
              class="sf-btn sf-btn--icon sf-btn--tiny"
              type="button"
              title="Убрать цвет"
              :disabled="background().length <= 1"
              @click="removeStop(index)"
            >
              <X :size="13" />
            </button>
          </div>
          <button
            class="sf-btn sf-btn--ghost sf-btn--small"
            type="button"
            :disabled="background().length >= 5"
            @click="addStop"
          >
            <Plus :size="13" /> Добавить цвет
          </button>
        </div>

        <div class="sf-grid-3">
          <label class="sf-field-wrap">
            <span class="sf-field-label">Крапинка {{ draft.tokens.decor.grain.toFixed(2) }}</span>
            <input v-model.number="draft.tokens.decor.grain" type="range" min="0" max="1" step="0.02" />
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Виньетка {{ draft.tokens.decor.vignette.toFixed(2) }}</span>
            <input v-model.number="draft.tokens.decor.vignette" type="range" min="0" max="1" step="0.02" />
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Свечение {{ draft.tokens.decor.glow.toFixed(2) }}</span>
            <input v-model.number="draft.tokens.decor.glow" type="range" min="0" max="1" step="0.02" />
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Аврора {{ draft.tokens.decor.aurora.toFixed(2) }}</span>
            <input v-model.number="draft.tokens.decor.aurora" type="range" min="0" max="1" step="0.02" />
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Контровой свет {{ draft.tokens.decor.rim.toFixed(2) }}</span>
            <input v-model.number="draft.tokens.decor.rim" type="range" min="0" max="1" step="0.02" />
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Масштаб узора {{ draft.tokens.decor.pattern_scale.toFixed(2) }}</span>
            <input
              v-model.number="draft.tokens.decor.pattern_scale"
              type="range"
              min="0.05"
              max="1"
              step="0.01"
            />
          </label>
          <label class="sf-check">
            <input v-model="draft.tokens.decor.scanlines" type="checkbox" />
            <span>Полосы развёртки</span>
          </label>
        </div>
      </details>

      <details class="sf-group">
        <summary>Движение</summary>
        <div class="sf-grid-3">
          <label class="sf-field-wrap">
            <span class="sf-field-label">Быстро, мс</span>
            <input
              v-model.number="draft.tokens.motion.duration_fast"
              class="sf-field"
              type="number"
              min="0"
              max="600"
              step="10"
            />
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Обычно, мс</span>
            <input
              v-model.number="draft.tokens.motion.duration"
              class="sf-field"
              type="number"
              min="0"
              max="1400"
              step="10"
            />
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Долго, мс</span>
            <input
              v-model.number="draft.tokens.motion.duration_slow"
              class="sf-field"
              type="number"
              min="0"
              max="3000"
              step="10"
            />
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Кривая</span>
            <select v-model="draft.tokens.motion.easing" class="sf-field">
              <option v-for="easing in EASINGS" :key="easing" :value="easing">
                {{ EASING_LABELS[easing] }}
              </option>
            </select>
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Уменьшение движения</span>
            <select v-model="draft.tokens.motion.reduced" class="sf-field">
              <option v-for="mode in REDUCED_MOTION_MODES" :key="mode" :value="mode">
                {{ mode === 'system' ? 'Как в системе' : mode === 'always' ? 'Всегда' : 'Никогда' }}
              </option>
            </select>
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">
              Отклик на наведение {{ draft.tokens.motion.hover_intensity.toFixed(2) }}
            </span>
            <input
              v-model.number="draft.tokens.motion.hover_intensity"
              type="range"
              min="0.5"
              max="2"
              step="0.05"
            />
          </label>
          <label class="sf-check">
            <input v-model="draft.tokens.motion.ambient" type="checkbox" />
            <span>Медленный дрейф фона</span>
          </label>
        </div>
        <p class="sf-card__note">
          Длительности применяются с множителем расположения; «Никогда» в
          уменьшении движения перебивает системную настройку доступности.
        </p>
      </details>

      <details class="sf-group">
        <summary>Расположение</summary>
        <div class="sf-grid-3">
          <label class="sf-field-wrap">
            <span class="sf-field-label">Масштаб интерфейса</span>
            <input
              v-model.number="draft.tokens.layout.scale"
              class="sf-field"
              type="number"
              min="0.8"
              max="1.4"
              step="0.02"
            />
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Плотность, px</span>
            <input v-model.number="draft.tokens.layout.density" class="sf-field" type="number" min="12" max="28" />
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Положение</span>
            <select v-model="draft.tokens.layout.placement" class="sf-field">
              <option v-for="placement in PLACEMENTS" :key="placement" :value="placement">
                {{ PLACEMENT_LABELS[placement] }}
              </option>
            </select>
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Углы</span>
            <select v-model="draft.tokens.layout.corners" class="sf-field">
              <option v-for="corner in CORNERS_VALUES" :key="corner" :value="corner">
                {{ CORNERS_LABELS[corner] }}
              </option>
            </select>
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Характер движения</span>
            <select v-model="draft.tokens.layout.motion" class="sf-field">
              <option v-for="style in MOTION_STYLES" :key="style" :value="style">
                {{ MOTION_STYLE_LABELS[style] }}
              </option>
            </select>
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">
              Множитель движения {{ draft.tokens.layout.motion_scale.toFixed(2) }}
            </span>
            <input
              v-model.number="draft.tokens.layout.motion_scale"
              type="range"
              min="0"
              max="2"
              step="0.05"
            />
          </label>
          <label class="sf-field-wrap">
            <span class="sf-field-label">Регистр подписей</span>
            <select v-model="draft.tokens.layout.label_case" class="sf-field">
              <option v-for="mode in LABEL_CASES" :key="mode" :value="mode">
                {{ LABEL_CASE_LABELS[mode] }}
              </option>
            </select>
          </label>
          <label class="sf-check">
            <input v-model="draft.tokens.layout.icon_only" type="checkbox" />
            <span>Только значки, без подписей</span>
          </label>
        </div>

        <span class="sf-field-label">Порядок разделов меню</span>
        <ul class="sf-order">
          <li v-for="(section, index) in draft.tokens.layout.order" :key="section" class="sf-order__row">
            <span class="sf-order__name">{{ SECTION_LABELS[section] }}</span>
            <label class="sf-check">
              <input
                type="checkbox"
                :checked="draft.tokens.layout.hidden.includes(section)"
                @change="toggleHidden(section)"
              />
              <span>скрыт</span>
            </label>
            <button
              class="sf-btn sf-btn--icon sf-btn--tiny"
              type="button"
              title="Выше"
              :disabled="index === 0"
              @click="moveOrder(index, -1)"
            >
              <ArrowUp :size="13" />
            </button>
            <button
              class="sf-btn sf-btn--icon sf-btn--tiny"
              type="button"
              title="Ниже"
              :disabled="index === draft.tokens.layout.order.length - 1"
              @click="moveOrder(index, 1)"
            >
              <ArrowDown :size="13" />
            </button>
          </li>
        </ul>

        <span class="sf-field-label">Быстрые действия на главном экране</span>
        <div class="sf-chips">
          <button
            v-for="shortcut in SHORTCUT_VALUES"
            :key="shortcut"
            class="sf-chip sf-chip--button"
            :class="{ 'is-active': draft.tokens.layout.shortcuts.includes(shortcut) }"
            type="button"
            @click="toggleShortcut(shortcut)"
          >
            {{ SHORTCUT_LABELS[shortcut] }}
          </button>
        </div>
      </details>

      <div class="sf-editor__foot">
        <button class="sf-btn sf-btn--soft" type="button" @click="closeEditor">
          <RefreshCw :size="14" /> Отменить правки
        </button>
        <button class="sf-btn" type="button" :disabled="saving" @click="save">
          <Save :size="14" />
          {{ draft.meta.builtin ? 'Сохранить как свою' : 'Сохранить' }}
        </button>
      </div>
    </section>

    <!-- --------------------------------------------------------- background -->
    <section class="sf-card">
      <header class="sf-card__head">
        <h2 class="sf-card__title"><ImageIcon :size="16" /> Своя картинка фона</h2>
        <button class="sf-btn sf-btn--small" type="button" :disabled="picking" @click="pickImage">
          <ImagePlus :size="14" /> Добавить
        </button>
      </header>

      <p v-if="!images.length" class="sf-card__lead sf-muted">
        Картинка рисуется поверх оформления темы. Прозрачность и размытие
        задаются самой темой в разделе «Фон и декор».
      </p>

      <ul v-else class="sf-images">
        <li
          v-for="image in images"
          :key="image.id"
          class="sf-image"
          :class="{ 'is-active': image.id === imageId }"
        >
          <button class="sf-image__pick" type="button" @click="useImage(image)">
            <img :src="toAssetUrl(image.path)" alt="" loading="lazy" />
            <span class="sf-image__name sf-muted">{{ image.fileName }}</span>
          </button>
          <button
            class="sf-btn sf-btn--icon"
            type="button"
            title="Удалить файл"
            @click="removeImage(image)"
          >
            <Trash2 :size="15" />
          </button>
        </li>
      </ul>
    </section>
  </div>
</template>

<style scoped>
@import './panel.css';

/* ------------------------------------------------------------------ themes */

.sf-themes {
  list-style: none;
  margin: 0 0 0.6rem;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 0.3rem;
  max-height: 26rem;
  overflow-y: auto;
}

.sf-theme {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  padding: 0.3rem 0.35rem;
  border-radius: var(--sf-radius);
  border: var(--sf-border-width) solid transparent;
  background: color-mix(in srgb, var(--sf-text) 5%, transparent);
}

.sf-theme.is-active {
  border-color: color-mix(in srgb, var(--sf-accent) 55%, transparent);
}

.sf-theme__swatch {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  flex: 1;
  min-width: 0;
  padding: 0.25rem 0.3rem;
  border: 0;
  border-radius: var(--sf-radius-sm);
  background: transparent;
  color: inherit;
  cursor: pointer;
  text-align: left;
}

.sf-theme__swatch:hover {
  background: color-mix(in srgb, var(--sf-text) 8%, transparent);
}

.sf-theme__dots {
  flex: none;
  display: grid;
  grid-template-columns: 1fr 1fr;
  width: 2.1rem;
  height: 2.1rem;
  border-radius: var(--sf-radius-sm);
  overflow: hidden;
  border: var(--sf-border-width) solid var(--sf-border);
}

.sf-theme__dots > span {
  display: block;
}

.sf-theme__text {
  display: flex;
  flex-direction: column;
  gap: 0.05rem;
  min-width: 0;
}

.sf-theme__name {
  display: flex;
  align-items: center;
  gap: 0.3rem;
  font-size: 0.85rem;
  font-weight: var(--sf-weight-bold);
}

.sf-theme__desc {
  font-size: 0.7rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.sf-theme__actions {
  display: flex;
  align-items: center;
  gap: 0.1rem;
  flex: none;
}

.sf-themes__foot {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.sf-themes__busy {
  display: flex;
  align-items: center;
  gap: 0.3rem;
  font-size: 0.72rem;
}

/* ------------------------------------------------------------------ editor */

.sf-editor__meta {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 2fr);
  gap: 0.6rem;
  margin-bottom: 0.7rem;
}

.sf-group {
  border-top: var(--sf-border-width) solid var(--sf-border);
  padding: 0.5rem 0;
}

.sf-group > summary {
  cursor: pointer;
  font-size: 0.8rem;
  font-weight: var(--sf-weight-bold);
  padding: 0.2rem 0;
  list-style: none;
}

.sf-group > summary::before {
  content: '▸ ';
  color: var(--sf-text-muted);
}

.sf-group[open] > summary::before {
  content: '▾ ';
}

.sf-group > * {
  margin-top: 0.5rem;
}

.sf-field-wrap {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
  min-width: 0;
}

.sf-field-label {
  font-size: 0.7rem;
  color: var(--sf-text-muted);
}

.sf-editor__inline {
  margin-top: 0.5rem;
}

.sf-check {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  font-size: 0.74rem;
  color: var(--sf-text-muted);
}

.sf-colors {
  display: flex;
  flex-direction: column;
  gap: 0.2rem;
}

.sf-color {
  display: flex;
  align-items: center;
  gap: 0.45rem;
  font-size: 0.72rem;
}

.sf-color__pick {
  flex: none;
  width: 1.6rem;
  height: 1.6rem;
  padding: 0;
  border: var(--sf-border-width) solid var(--sf-border);
  border-radius: var(--sf-radius-sm);
  background: none;
  cursor: pointer;
}

.sf-color__label {
  flex: 0 0 12rem;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.sf-color__alpha {
  flex: 1;
  min-width: 4rem;
}

.sf-color__value {
  flex: none;
  width: 5.5rem;
  font-size: 0.66rem;
}

.sf-order {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 0.2rem;
}

.sf-order__row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.25rem 0.35rem;
  border-radius: var(--sf-radius-sm);
  background: color-mix(in srgb, var(--sf-text) 5%, transparent);
  font-size: 0.74rem;
}

.sf-order__name {
  flex: 1;
  min-width: 0;
}

.sf-chips {
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

.sf-editor__foot {
  display: flex;
  justify-content: flex-end;
  gap: 0.5rem;
  margin-top: 0.8rem;
  padding-top: 0.7rem;
  border-top: var(--sf-border-width) solid var(--sf-border);
}

/* -------------------------------------------------------------- background */

.sf-images {
  list-style: none;
  margin: 0;
  padding: 0;
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(8rem, 1fr));
  gap: 0.4rem;
}

.sf-image {
  display: flex;
  flex-direction: column;
  gap: 0.2rem;
  padding: 0.25rem;
  border-radius: var(--sf-radius);
  border: var(--sf-border-width) solid transparent;
  background: color-mix(in srgb, var(--sf-text) 5%, transparent);
}

.sf-image.is-active {
  border-color: color-mix(in srgb, var(--sf-accent) 55%, transparent);
}

.sf-image__pick {
  display: flex;
  flex-direction: column;
  gap: 0.2rem;
  padding: 0;
  border: 0;
  background: none;
  color: inherit;
  cursor: pointer;
  text-align: left;
}

.sf-image__pick img {
  width: 100%;
  height: 4rem;
  object-fit: cover;
  border-radius: var(--sf-radius-sm);
}

.sf-image__name {
  font-size: 0.66rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.sf-btn--tiny {
  padding: 0.2rem;
}

.sf-spin {
  animation: sf-spin 0.9s linear infinite;
}

@keyframes sf-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
