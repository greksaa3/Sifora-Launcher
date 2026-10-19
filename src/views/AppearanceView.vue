<script setup lang="ts">
/**
 * Appearance.
 *
 * Three parts, in order of how often they are used:
 *
 * 1. **Background** — built-in presets plus the user's own image.
 * 2. **AI designer** — describe a theme in words, preview it, then apply.
 *    The model can only produce design tokens: colours, radii, fonts,
 *    decoration and timing. It cannot emit code, touch the filesystem or
 *    change what the launcher can do (see `src-tauri/src/theme/`).
 * 3. **Gallery** — apply, undo, reset or delete any theme.
 */
import { computed, onMounted, ref } from 'vue'
import { AnimatePresence, motion } from 'motion-v'
import {
  Check,
  Image as ImageIcon,
  ImagePlus,
  Loader,
  PlugZap,
  RefreshCw,
  RotateCcw,
  Sparkles,
  Trash2,
  Undo2,
  Wand2,
} from '@lucide/vue'

import { backgrounds, themes as themeApi } from '@/lib/ipc'
import { formatBytes } from '@/lib/format'
import { pushToast, reportError } from '@/stores/app'
import type {
  BackgroundImage,
  ModelOption,
  ProviderKind,
  ProviderPreset,
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
}>()

/* ------------------------------------------------------------- background */

const picking = ref(false)
const imageUrl = ref<string | null>(null)

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

async function useImage(image: BackgroundImage): Promise<void> {
  imageUrl.value = image.path
  emit('image-changed', image)
}

/* ------------------------------------------------------------- AI designer */

const prompt = ref('')
const generating = ref(false)
const preview = ref<ThemeSpec | null>(null)
const previewNotes = ref<string[]>([])

async function generate(): Promise<void> {
  if (prompt.value.trim().length < 3) {
    pushToast('warning', 'Опишите тему подробнее — хотя бы несколько слов')
    return
  }
  generating.value = true
  try {
    const result = await themeApi.generate(prompt.value.trim())
    preview.value = result.theme
    previewNotes.value = [
      result.note,
      ...result.report.fixes.map((issue) => `Исправлено: ${issue.message}`),
      ...result.report.warnings.map((issue) => issue.message),
    ]
    if (result.fellBack) {
      pushToast('warning', 'Использована локальная композиция', result.note)
    }
  } catch (error) {
    reportError(error)
  } finally {
    generating.value = false
  }
}

async function applyPreview(): Promise<void> {
  if (!preview.value) return
  try {
    await themeApi.apply(preview.value)
    emit('applied', preview.value)
    pushToast('success', `Тема «${preview.value.meta.name}» применена`)
    preview.value = null
  } catch (error) {
    reportError(error)
  }
}

function discardPreview(): void {
  preview.value = null
  previewNotes.value = []
}

const EXAMPLES = [
  'тёмная фиолетовая тема со стеклянными панелями и сеткой',
  'светлая минималистичная тема в оттенках моря, скруглённая, без анимаций',
  'неоновая киберпанк тема с терминальным шрифтом',
  'тёплая уютная тема с крупными радиусами и мягкими тенями',
]

/* ----------------------------------------------------------------- gallery */

async function applyById(id: string): Promise<void> {
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

async function remove(theme: ThemeSummary): Promise<void> {
  try {
    await themeApi.remove(theme.id)
    pushToast('info', `Тема «${theme.name}» удалена`)
  } catch (error) {
    reportError(error)
  }
}

/* ---------------------------------------------------------- AI connection */

const showProvider = ref(false)
const provider = ref<ProviderKind>('local')
const baseUrl = ref('')
const model = ref('')
const apiKey = ref('')
const hasKey = ref(false)
const temperature = ref(0.7)

const presets = ref<ProviderPreset[]>([])
const models = ref<ModelOption[]>([])
const loadingModels = ref(false)
const modelProblem = ref<string | null>(null)
const testing = ref(false)
const typedModel = ref(false)

/** The preset whose address is currently in the field, if any. */
const activePreset = computed(() => {
  const url = normaliseUrl(baseUrl.value)
  if (!url) return null
  return presets.value.find((p) => p.baseUrl === url) ?? null
})

/** Trailing slashes make two spellings of the same address look different. */
function normaliseUrl(value: unknown): string {
  return typeof value === 'string' ? value.trim().replace(/\/+$/, '') : ''
}

const providerLabel = computed(() =>
  provider.value === 'local'
    ? 'Встроенный композитор — бесплатно, офлайн'
    : `Своя модель: ${model.value || 'не указана'} @ ${baseUrl.value || 'не указан адрес'}`,
)

/**
 * Free models first, then the rest. `null` (unknown) sits with the paid ones
 * rather than with the free ones: promising "free" on a guess is exactly the
 * kind of small lie that makes a settings panel untrustworthy.
 */
const freeModels = computed(() => models.value.filter((m) => m.free === true))
const otherModels = computed(() => models.value.filter((m) => m.free !== true))

/** A saved model the provider no longer lists must stay selectable. */
const savedModelMissing = computed(
  () => !!model.value && !typedModel.value && !models.value.some((m) => m.id === model.value),
)

onMounted(async () => {
  try {
    presets.value = await themeApi.presets()
  } catch {
    // The panel still works as free text without presets.
  }
  try {
    // The declared type is a promise, not a guarantee: anything odd in the
    // reply is coerced here rather than trusted, so a single unexpected field
    // can never blank the panel.
    const config = await themeApi.aiConfig()
    provider.value = config?.provider === 'openai_compatible' ? 'openai_compatible' : 'local'
    baseUrl.value = asText(config?.baseUrl)
    model.value = asText(config?.model)
    hasKey.value = config?.hasApiKey === true
    temperature.value = asNumber(config?.temperature, 0.7)
    if (provider.value === 'openai_compatible' && baseUrl.value) void loadModels()
  } catch {
    // The offline composer always works; provider settings are optional.
  }
})

function asText(value: unknown): string {
  return typeof value === 'string' ? value : ''
}

function asNumber(value: unknown, fallback: number): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback
}

/** Fetch the provider's own model list for the address in the field. */
async function loadModels(): Promise<void> {
  const url = baseUrl.value.trim()
  if (!url) return
  loadingModels.value = true
  modelProblem.value = null
  try {
    models.value = await themeApi.models(url)
    if (!models.value.length) modelProblem.value = 'Провайдер не сообщил ни одной модели'
  } catch (error) {
    models.value = []
    modelProblem.value = errorMessage(error)
  } finally {
    loadingModels.value = false
  }
}

function errorMessage(error: unknown): string {
  if (typeof error === 'string') return error
  if (error && typeof error === 'object' && 'message' in error) {
    return String((error as { message: unknown }).message)
  }
  return 'Не удалось получить список моделей'
}

/** One click fills the address and pulls the model list. */
function usePreset(preset: ProviderPreset): void {
  provider.value = 'openai_compatible'
  baseUrl.value = preset.baseUrl
  typedModel.value = false
  models.value = []
  modelProblem.value = null
  void loadModels()
}

async function testConnection(): Promise<void> {
  testing.value = true
  try {
    pushToast('info', 'Проверяю связь', 'Один короткий запрос к модели')
    const message = await themeApi.testConnection()
    pushToast('success', 'Связь есть', message)
  } catch (error) {
    modelProblem.value = errorMessage(error)
    pushToast('error', 'Модель не ответила', errorMessage(error))
  } finally {
    testing.value = false
  }
}

async function saveProvider(): Promise<void> {
  try {
    const saved = await themeApi.setAiConfig({
      provider: provider.value,
      baseUrl: baseUrl.value,
      model: model.value,
      apiKey: apiKey.value,
      temperature: temperature.value,
    })
    provider.value = saved.provider
    hasKey.value = saved.hasApiKey
    apiKey.value = ''
    pushToast(
      'success',
      'Настройки модели сохранены',
      saved.provider === 'local'
        ? 'Используется встроенный композитор тем'
        : `${saved.model} @ ${saved.baseUrl}`,
    )
  } catch (error) {
    reportError(error)
  }
}

const customThemes = computed(() => props.catalog.filter((t) => !t.builtin))
</script>

<template>
  <div class="sf-view">
    <!-- ---------------------------------------------------------- AI designer -->
    <section class="sf-card">
      <header class="sf-card__head">
        <h2 class="sf-card__title">
          <Wand2 :size="16" />
          Дизайнер тем
        </h2>
        <button class="sf-btn sf-btn--ghost" type="button" @click="showProvider = !showProvider">
          {{ showProvider ? 'Скрыть' : 'Своя модель' }}
        </button>
      </header>

      <p class="sf-card__provider">{{ providerLabel }}</p>

      <p class="sf-card__lead sf-muted">
        Опишите желаемый вид словами. По умолчанию тему собирает
        <b>встроенный бесплатный композитор</b> — он работает без интернета и
        без ключей: разбирает описание, выбирает цвета, форму, шрифт и
        анимацию. Хотите настоящую языковую модель — подключите свою в разделе
        «Своя модель».
      </p>
      <p class="sf-card__note">
        Тема меняет только внешний вид. Ни одна тема не может отключить функции
        лаунчера, изменить права приложения или выполнить код.
      </p>

      <textarea
        v-model="prompt"
        class="sf-field sf-designer__input"
        rows="3"
        maxlength="2000"
        placeholder="например: тёмная синяя тема, скруглённые кнопки, мягкое свечение, без анимаций"
        @keydown.ctrl.enter="generate"
      />

      <div class="sf-designer__examples">
        <button
          v-for="example in EXAMPLES"
          :key="example"
          class="sf-chip"
          type="button"
          @click="prompt = example"
        >
          {{ example.length > 42 ? example.slice(0, 42) + '…' : example }}
        </button>
      </div>

      <div class="sf-designer__actions">
        <button class="sf-btn sf-btn--primary" type="button" :disabled="generating" @click="generate">
          <Loader v-if="generating" :size="15" class="animate-spin" />
          <Sparkles v-else :size="15" />
          {{ generating ? 'Создаём…' : 'Создать тему' }}
        </button>
        <span class="sf-muted sf-designer__hint">Ctrl + Enter</span>
      </div>

      <AnimatePresence>
        <motion.div
          v-if="preview"
          class="sf-preview"
          :initial="{ opacity: 0, height: 0 }"
          :animate="{ opacity: 1, height: 'auto' }"
          :exit="{ opacity: 0, height: 0 }"
          :transition="{ duration: 0.22 }"
        >
          <div class="sf-preview__card">
            <div
              class="sf-preview__swatch"
              :style="{
                background: `linear-gradient(${preview.tokens.background.angle}deg, ${preview.tokens.background.colors.join(', ')})`,
              }"
            >
              <span class="sf-preview__name">{{ preview.meta.name }}</span>
            </div>
            <div class="sf-preview__body">
              <p class="sf-preview__desc sf-muted">{{ preview.meta.description }}</p>
              <div class="sf-preview__tokens">
                <span
                  v-for="color in [
                    preview.tokens.palette.accent,
                    preview.tokens.palette.text,
                    preview.tokens.palette.surface_raised,
                  ]"
                  :key="color"
                  class="sf-preview__token"
                  :style="{ background: color }"
                  :title="color"
                />
                <span class="sf-chip">радиус {{ preview.tokens.shape.radius }}px</span>
                <span class="sf-chip">{{ preview.tokens.typography.family }}</span>
                <span class="sf-chip">{{ preview.tokens.motion.duration }} мс</span>
              </div>
            </div>
          </div>

          <ul v-if="previewNotes.length" class="sf-preview__notes">
            <li v-for="(note, index) in previewNotes" :key="index">{{ note }}</li>
          </ul>

          <div class="sf-preview__actions">
            <button class="sf-btn sf-btn--primary" type="button" @click="applyPreview">
              <Check :size="15" />
              Применить
            </button>
            <button class="sf-btn sf-btn--outline" type="button" @click="discardPreview">
              Отмена
            </button>
          </div>
        </motion.div>
      </AnimatePresence>

      <AnimatePresence>
        <motion.div
          v-if="showProvider"
          class="sf-provider"
          :initial="{ opacity: 0, height: 0 }"
          :animate="{ opacity: 1, height: 'auto' }"
          :exit="{ opacity: 0, height: 0 }"
        >
          <p class="sf-card__lead sf-muted">
            Необязательно. Встроенный композитор работает всегда — модель нужна
            только для свободных формулировок. Ключ хранится только в папке данных
            приложения и никогда не попадает в сборку лаунчера.
          </p>

          <div class="sf-presets">
            <span class="sf-label">Готовые</span>
            <div class="sf-presets__row">
              <button
                v-for="preset in presets"
                :key="preset.id"
                class="sf-btn sf-btn--soft sf-preset"
                :class="{ 'sf-preset--active': activePreset?.id === preset.id }"
                type="button"
                @click="usePreset(preset)"
              >
                {{ preset.label }}
              </button>
            </div>
            <p v-if="activePreset" class="sf-presets__note sf-muted">
              {{ activePreset.note }}
            </p>
          </div>

          <div class="sf-provider__grid">
            <label class="sf-field-row">
              <span class="sf-label">Источник</span>
              <select v-model="provider" class="sf-field">
                <option value="local">Встроенный композитор (офлайн)</option>
                <option value="openai_compatible">Своя модель</option>
              </select>
            </label>
            <label class="sf-field-row">
              <span class="sf-label">Адрес (https или localhost)</span>
              <input
                v-model="baseUrl"
                class="sf-field"
                placeholder="https://…/v1"
                :disabled="provider === 'local'"
                autocomplete="off"
                spellcheck="false"
                @change="loadModels"
              />
            </label>

            <label class="sf-field-row">
              <span class="sf-label">
                Модель
                <span
                  v-if="freeModels.length"
                  class="sf-provider__badge"
                  title="Провайдер указал нулевую цену за токены"
                >
                  бесплатных: {{ freeModels.length }}
                </span>
              </span>
              <select
                v-if="!typedModel"
                v-model="model"
                class="sf-field"
                :disabled="provider === 'local' || loadingModels"
              >
                <option value="" disabled>
                  {{ loadingModels ? 'Загружаю список…' : 'Выберите модель' }}
                </option>
                <option v-if="savedModelMissing" :value="model">
                  {{ model }} (сохранена)
                </option>
                <optgroup v-if="freeModels.length" label="Бесплатные">
                  <option v-for="m in freeModels" :key="m.id" :value="m.id">
                    {{ m.label }}
                  </option>
                </optgroup>
                <optgroup v-if="otherModels.length" label="Остальные">
                  <option v-for="m in otherModels" :key="m.id" :value="m.id">
                    {{ m.label }}
                  </option>
                </optgroup>
              </select>
              <input
                v-else
                v-model="model"
                class="sf-field"
                placeholder="точное имя модели"
                :disabled="provider === 'local'"
                autocomplete="off"
                spellcheck="false"
              />
            </label>

            <div class="sf-provider__row">
              <button
                class="sf-btn sf-btn--ghost sf-btn--small"
                type="button"
                :disabled="provider === 'local' || loadingModels"
                @click="loadModels"
              >
                <Loader v-if="loadingModels" :size="14" class="sf-spin" />
                <RefreshCw v-else :size="14" />
                {{ loadingModels ? 'Обновляю…' : 'Обновить список' }}
              </button>
              <button
                class="sf-btn sf-btn--ghost sf-btn--small"
                type="button"
                @click="typedModel = !typedModel"
              >
                {{ typedModel ? 'Выбрать из списка' : 'Вписать вручную' }}
              </button>
            </div>

            <p v-if="modelProblem" class="sf-provider__problem" role="status">
              {{ modelProblem }}
            </p>

            <label class="sf-field-row">
              <span class="sf-label">Ключ {{ hasKey ? '(сохранён)' : '' }}</span>
              <input
                v-model="apiKey"
                class="sf-field"
                type="password"
                placeholder="оставьте пустым, чтобы не менять"
                :disabled="provider === 'local'"
                autocomplete="off"
              />
            </label>
            <label class="sf-field-row">
              <span class="sf-label">Температура {{ temperature.toFixed(1) }}</span>
              <input v-model.number="temperature" type="range" min="0" max="1.4" step="0.1" />
            </label>
          </div>

          <div class="sf-provider__actions">
            <button class="sf-btn sf-btn--soft" type="button" @click="saveProvider">
              Сохранить
            </button>
            <button
              class="sf-btn sf-btn--outline"
              type="button"
              :disabled="testing"
              @click="testConnection"
            >
              <Loader v-if="testing" :size="14" class="sf-spin" />
              <PlugZap v-else :size="14" />
              {{ testing ? 'Проверяю…' : 'Проверить связь' }}
            </button>
          </div>
          <p class="sf-card__note">
            На бесплатном тарифе OpenRouter — до 50 запросов в сутки. Если модель
            занята, тема всё равно будет создана локально.
          </p>
        </motion.div>
      </AnimatePresence>
    </section>

    <!-- ------------------------------------------------------------- gallery -->
    <section class="sf-card">
      <header class="sf-card__head">
        <h2 class="sf-card__title">
          <Palette :size="16" />
          Темы
        </h2>
        <div class="sf-card__actions">
          <button class="sf-btn sf-btn--ghost" type="button" :disabled="!canUndo" @click="undo">
            <Undo2 :size="15" />
            Отменить
          </button>
          <button class="sf-btn sf-btn--ghost" type="button" @click="reset">
            <RotateCcw :size="15" />
            Сброс
          </button>
        </div>
      </header>

      <div class="sf-gallery">
        <button
          v-for="theme in catalog"
          :key="theme.id"
          class="sf-theme"
          type="button"
          :class="{ 'sf-theme--active': theme.id === activeId }"
          @click="applyById(theme.id)"
        >
          <span
            class="sf-theme__chip"
            :style="{ background: theme.background, '--swatch': theme.accent }"
          />
          <span class="sf-theme__name">{{ theme.name }}</span>
          <span v-if="!theme.builtin" class="sf-theme__badge sf-muted">своя</span>
          <span
            v-if="!theme.builtin"
            class="sf-theme__delete"
            role="button"
            tabindex="-1"
            title="Удалить тему"
            @click.stop="remove(theme)"
          >
            <Trash2 :size="13" />
          </span>
        </button>
      </div>

      <p v-if="customThemes.length" class="sf-card__foot sf-muted">
        Своих тем: {{ customThemes.length }}. Они сохранены в папке данных лаунчера.
      </p>
    </section>

    <!-- ---------------------------------------------------------- backgrounds -->
    <section class="sf-card">
      <header class="sf-card__head">
        <h2 class="sf-card__title">
          <ImageIcon :size="16" />
          Фон
        </h2>
      </header>

      <div class="sf-bg-list">
        <button
          v-for="theme in catalog.filter((t) => t.builtin)"
          :key="theme.id"
          class="sf-bg-swatch"
          type="button"
          :title="theme.description"
          :aria-label="theme.name"
          :aria-pressed="theme.id === presetId && !imageUrl"
          :class="{ 'sf-bg-swatch--active': theme.id === presetId && !imageUrl }"
          :style="{ background: theme.background }"
          @click="imageUrl = null; emit('preset-changed', theme.id)"
        >
          <span>{{ theme.name }}</span>
        </button>
      </div>

      <hr class="sf-divider" />

      <div class="sf-bg-upload">
        <button class="sf-btn sf-btn--soft" type="button" :disabled="picking" @click="pickImage">
          <ImagePlus :size="15" />
          {{ picking ? 'Копируем…' : 'Выбрать изображение' }}
        </button>
        <span class="sf-muted">PNG, JPG, WebP · до 48 МБ · копируется внутрь лаунчера</span>
      </div>

      <ul v-if="images.length" class="sf-bg-images">
        <li v-for="image in images" :key="image.id" class="sf-bg-image">
          <button class="sf-bg-image__pick" type="button" @click="useImage(image)">
            <img :src="`asset:localhost/backgrounds/${image.id}`" :alt="image.fileName" />
            <span class="sf-bg-image__meta">
              <span>{{ image.fileName }}</span>
              <span class="sf-muted">{{ formatBytes(image.sizeBytes) }}</span>
            </span>
          </button>
          <button class="sf-btn sf-btn--icon" type="button" title="Удалить" @click="removeImage(image)">
            <Trash2 :size="15" />
          </button>
        </li>
      </ul>

      <p v-if="imageUrl" class="sf-card__foot sf-muted">
        Своё изображение активно. Выберите пресет выше, чтобы вернуть градиент.
      </p>
    </section>
  </div>
</template>

<style scoped>
@import './panel.css';

.sf-designer__input {
  resize: vertical;
  min-height: 4.5rem;
  line-height: 1.5;
}

.sf-designer__examples {
  display: flex;
  flex-wrap: wrap;
  gap: 0.3rem;
  margin-top: 0.5rem;
}

.sf-designer__examples .sf-chip {
  cursor: pointer;
  text-transform: none;
  letter-spacing: 0;
  font-weight: var(--sf-weight);
  transition:
    border-color var(--sf-dur-fast) var(--sf-ease),
    color var(--sf-dur-fast) var(--sf-ease);
}

.sf-designer__examples .sf-chip:hover {
  color: var(--sf-accent);
  border-color: color-mix(in srgb, var(--sf-accent) 45%, transparent);
}

.sf-designer__actions {
  display: flex;
  align-items: center;
  gap: 0.7rem;
  margin-top: 0.7rem;
}

.sf-designer__hint {
  font-size: 0.68rem;
}

/* ---------------------------------------------------------------- preview */

.sf-preview {
  overflow: hidden;
  margin-top: 0.8rem;
}

.sf-preview__card {
  display: flex;
  gap: 0.7rem;
  padding: 0.6rem;
  border-radius: var(--sf-radius);
  background: color-mix(in srgb, var(--sf-text) 5%, transparent);
  border: var(--sf-border-width) solid var(--sf-border);
}

.sf-preview__swatch {
  width: 7rem;
  flex: none;
  border-radius: var(--sf-radius-sm);
  border: 1px solid var(--sf-border);
  display: flex;
  align-items: flex-end;
  padding: 0.4rem;
}

.sf-preview__name {
  font-size: 0.68rem;
  font-weight: var(--sf-weight-bold);
  color: #fff;
  text-shadow: 0 1px 3px rgb(0 0 0 / 0.6);
}

.sf-preview__body {
  flex: 1;
  min-width: 0;
}

.sf-preview__desc {
  margin: 0 0 0.45rem;
  font-size: 0.76rem;
}

.sf-preview__tokens {
  display: flex;
  flex-wrap: wrap;
  gap: 0.3rem;
  align-items: center;
}

.sf-preview__token {
  width: 1.1rem;
  height: 1.1rem;
  border-radius: 6px;
  border: 1px solid var(--sf-border);
}

.sf-preview__notes {
  margin: 0.5rem 0 0;
  padding-left: 1.1rem;
  font-size: 0.72rem;
  color: var(--sf-text-muted);
}

.sf-preview__actions {
  display: flex;
  gap: 0.4rem;
  margin-top: 0.7rem;
}

/* --------------------------------------------------------------- provider */

.sf-provider {
  overflow: hidden;
  margin-top: 0.8rem;
  padding-top: 0.8rem;
  border-top: var(--sf-border-width) solid var(--sf-border);
  display: flex;
  flex-direction: column;
  gap: 0.6rem;
  align-items: flex-start;
}

.sf-provider__grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(13rem, 1fr));
  gap: 0.5rem;
  width: 100%;
}

.sf-field-row {
  display: flex;
  flex-direction: column;
  gap: 0.22rem;
}

.sf-field-row input[type='range'] {
  accent-color: var(--sf-accent);
}

/* --------------------------------------------------------------- presets */

.sf-presets {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
  width: 100%;
}

.sf-presets__row {
  display: flex;
  flex-wrap: wrap;
  gap: 0.35rem;
}

.sf-preset {
  padding: 0.3rem 0.7rem;
  font-size: 0.74rem;
}

.sf-preset--active {
  border-color: var(--sf-accent);
  color: var(--sf-accent);
  background: color-mix(in srgb, var(--sf-accent) 14%, transparent);
}

.sf-presets__note {
  margin: 0;
  font-size: 0.7rem;
  line-height: 1.4;
  max-width: 42rem;
}

.sf-provider__badge {
  margin-left: 0.35rem;
  padding: 0 0.35rem;
  border-radius: var(--sf-radius-sm);
  background: color-mix(in srgb, var(--sf-success) 20%, transparent);
  color: var(--sf-success);
  font-size: 0.64rem;
  font-weight: var(--sf-weight-bold);
}

.sf-provider__row {
  display: flex;
  flex-wrap: wrap;
  gap: 0.35rem;
}

.sf-provider__problem {
  margin: 0;
  flex-basis: 100%;
  font-size: 0.72rem;
  line-height: 1.4;
  color: var(--sf-warning);
}

.sf-provider__actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.4rem;
  width: 100%;
}

/* ---------------------------------------------------------------- gallery */

.sf-gallery {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(9rem, 1fr));
  gap: 0.45rem;
}

.sf-theme {
  position: relative;
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.45rem 0.55rem;
  border-radius: var(--sf-radius);
  border: 1px solid var(--sf-border);
  background: color-mix(in srgb, var(--sf-text) 4%, transparent);
  font-size: 0.78rem;
  text-align: left;
  transition:
    border-color var(--sf-dur-fast) var(--sf-ease),
    background-color var(--sf-dur-fast) var(--sf-ease);
}

.sf-theme:hover {
  border-color: var(--sf-border-strong);
}

.sf-theme--active {
  border-color: var(--sf-accent);
  background: var(--sf-accent-a);
}

.sf-theme__chip {
  width: 1.4rem;
  height: 1.4rem;
  border-radius: 7px;
  border: 1px solid var(--swatch);
  flex: none;
}

.sf-theme__name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.sf-theme__badge {
  font-size: 0.6rem;
}

.sf-theme__delete {
  color: var(--sf-text-muted);
  opacity: 0;
  transition: opacity var(--sf-dur-fast) var(--sf-ease);
}

.sf-theme:hover .sf-theme__delete {
  opacity: 1;
}

.sf-theme__delete:hover {
  color: var(--sf-danger);
}

/* ------------------------------------------------------------ background */

.sf-bg-list {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(7rem, 1fr));
  gap: 0.4rem;
}

.sf-bg-swatch {
  aspect-ratio: 16 / 10;
  border-radius: var(--sf-radius-sm);
  border: 2px solid transparent;
  display: flex;
  align-items: flex-end;
  padding: 0.3rem;
  overflow: hidden;
  transition:
    border-color var(--sf-dur-fast) var(--sf-ease),
    transform var(--sf-dur-fast) var(--sf-ease);
}

.sf-bg-swatch:hover {
  transform: translateY(calc(var(--sf-hover-lift) * -1));
}

.sf-bg-swatch--active {
  border-color: var(--sf-accent);
}

.sf-bg-swatch span {
  font-size: 0.66rem;
  font-weight: var(--sf-weight-bold);
  color: #fff;
  text-shadow: 0 1px 3px rgb(0 0 0 / 0.65);
}

.sf-bg-upload {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  flex-wrap: wrap;
  margin-top: 0.6rem;
  font-size: 0.72rem;
}

.sf-bg-images {
  list-style: none;
  margin: 0.6rem 0 0;
  padding: 0;
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(11rem, 1fr));
  gap: 0.35rem;
}

.sf-bg-image {
  display: flex;
  align-items: center;
  gap: 0.3rem;
  padding: 0.25rem;
  border-radius: var(--sf-radius-sm);
  background: color-mix(in srgb, var(--sf-text) 5%, transparent);
}

.sf-bg-image__pick {
  display: flex;
  align-items: center;
  gap: 0.45rem;
  flex: 1;
  min-width: 0;
  text-align: left;
}

.sf-bg-image__pick img {
  width: 3.4rem;
  height: 2.2rem;
  border-radius: 6px;
  object-fit: cover;
  flex: none;
  border: 1px solid var(--sf-border);
}

.sf-bg-image__meta {
  display: flex;
  flex-direction: column;
  min-width: 0;
  font-size: 0.7rem;
}

.sf-bg-image__meta span:first-child {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>