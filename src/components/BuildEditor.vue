<script setup lang="ts">
/**
 * Build editor.
 *
 * Creates or edits one build: name, Minecraft version, mod loader, memory,
 * window size and extra JVM/game arguments. Loader versions are fetched from
 * the loader's own metadata for the chosen game version, so the list can only
 * ever contain combinations that exist — Fabric simply offers nothing for
 * 1.12.2, and the panel says so instead of failing at install time.
 */
import { computed, onMounted, ref, watch } from 'vue'
import { ImagePlus, Loader2, Save, Trash2, X } from '@lucide/vue'

import { loadersFor } from '@/stores/builds'
import { createBuild, clearBuildIcon, pickBuildIcon, updateBuild } from '@/stores/builds'
import { pushToast, reportError } from '@/stores/app'
import { loadVersions, rows as versionRows, loading as versionsLoading } from '@/stores/versions'
import { MOD_LOADER_LABELS } from '@/types'
import type { BuildSummary, LoaderCatalog, LoaderOption, ModLoader } from '@/types'

const props = defineProps<{
  /** `null` creates a new build; otherwise the build being edited. */
  build: BuildSummary | null
}>()

const emit = defineEmits<{
  (event: 'saved', build: BuildSummary): void
  (event: 'cancel'): void
  (event: 'removed', slug: string): void
}>()

const creating = computed(() => props.build === null)

const name = ref('')
const versionId = ref('')
const loader = ref<ModLoader>('none')
const loaderVersion = ref('')
const ramMb = ref<number | null>(null)
const width = ref<number | null>(null)
const height = ref<number | null>(null)
const jvmText = ref('')
const gameText = ref('')

const catalog = ref<LoaderCatalog | null>(null)
const catalogLoading = ref(false)
const catalogError = ref<string | null>(null)
const busy = ref(false)
const iconBusy = ref(false)
const armedRemove = ref(false)

/** Loaders the UI offers. Quilt and OptiFine are named but not selectable. */
const SELECTABLE: ModLoader[] = ['none', 'fabric', 'forge', 'neo_forge']
const UNAVAILABLE: ModLoader[] = ['quilt', 'opti_fine']

const options = computed<LoaderOption[]>(() => {
  const list = catalog.value
  if (!list) return []
  if (loader.value === 'fabric') return list.fabric
  if (loader.value === 'forge') return list.forge
  if (loader.value === 'neo_forge') return list.neoforge
  return []
})

const lines = (text: string): string[] =>
  text
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line.length > 0)

/**
 * Coerce a number field to what Rust expects.
 *
 * A cleared `<input type="number">` reports `''`, not `null`, and a raw string
 * would be rejected by the command's `Option<u32>` — so the boundary is
 * normalised here rather than trusted.
 */
const num = (value: unknown): number | null => {
  if (value === null || value === undefined || value === '') return null
  const parsed = typeof value === 'number' ? value : Number(value)
  return Number.isFinite(parsed) ? Math.trunc(parsed) : null
}

function reset(): void {
  const build = props.build
  name.value = build?.name ?? ''
  versionId.value = build?.versionId ?? ''
  loader.value = build?.loader ?? 'none'
  loaderVersion.value = build?.loaderVersion ?? ''
  ramMb.value = build?.ramMb ?? null
  width.value = build?.width ?? null
  height.value = build?.height ?? null
  jvmText.value = (build?.jvmArgs ?? []).join('\n')
  gameText.value = (build?.gameArgs ?? []).join('\n')
  catalog.value = null
  catalogError.value = null
  armedRemove.value = false
}

watch(() => props.build, reset, { immediate: true })

onMounted(() => {
  if (!versionRows.value.length) void loadVersions()
})

async function refreshCatalog(): Promise<void> {
  const version = versionId.value.trim()
  if (!version) {
    catalog.value = null
    return
  }
  catalogLoading.value = true
  catalogError.value = null
  try {
    const result = await loadersFor(version)
    // A slower request for a previous version must not overwrite this one.
    if (versionId.value.trim() !== version) return
    catalog.value = result
    const available = options.value
    // Keep the choice when it is still valid, otherwise take the newest.
    if (!available.some((option) => option.version === loaderVersion.value)) {
      loaderVersion.value = available[0]?.version ?? ''
    }
  } catch (error) {
    catalog.value = null
    catalogError.value = (error as { message?: string })?.message ?? 'Не удалось получить загрузчики'
  } finally {
    catalogLoading.value = false
  }
}

watch(versionId, () => {
  if (loader.value !== 'none') void refreshCatalog()
})

watch(loader, (next) => {
  if (next === 'none') {
    loaderVersion.value = ''
    return
  }
  void refreshCatalog()
})

const canSave = computed(() => {
  if (!name.value.trim() || busy.value) return false
  if (creating.value) {
    if (!versionId.value.trim()) return false
    if (loader.value !== 'none' && !loaderVersion.value) return false
  }
  return true
})

async function save(): Promise<void> {
  if (!canSave.value) return
  busy.value = true
  try {
    if (creating.value) {
      const created = await createBuild({
        name: name.value.trim(),
        versionId: versionId.value.trim(),
        loader: loader.value,
        loaderVersion: loader.value === 'none' ? '' : loaderVersion.value,
        ramMb: num(ramMb.value),
        width: num(width.value),
        height: num(height.value),
        jvmArgs: lines(jvmText.value),
        gameArgs: lines(gameText.value),
      })
      pushToast('success', `Сборка «${created.name}» создана`)
      emit('saved', created)
    } else {
      const updated = await updateBuild(props.build!.slug, {
        name: name.value.trim(),
        ramMb: num(ramMb.value),
        width: num(width.value),
        height: num(height.value),
        jvmArgs: lines(jvmText.value),
        gameArgs: lines(gameText.value),
      })
      pushToast('success', 'Сборка обновлена')
      emit('saved', updated)
    }
  } catch (error) {
    reportError(error)
  } finally {
    busy.value = false
  }
}

async function chooseIcon(): Promise<void> {
  if (creating.value) return
  iconBusy.value = true
  try {
    const updated = await pickBuildIcon(props.build!.slug)
    if (updated) emit('saved', updated)
  } catch (error) {
    reportError(error)
  } finally {
    iconBusy.value = false
  }
}

async function dropIcon(): Promise<void> {
  if (creating.value) return
  iconBusy.value = true
  try {
    const updated = await clearBuildIcon(props.build!.slug)
    emit('saved', updated)
  } catch (error) {
    reportError(error)
  } finally {
    iconBusy.value = false
  }
}
</script>

<template>
  <section class="sf-card">
    <header class="sf-card__head">
      <h2 class="sf-card__title">
        <Loader2 v-if="creating" :size="16" />
        <Save v-else :size="16" />
        {{ creating ? 'Новая сборка' : `Настройки «${build?.name}»` }}
      </h2>
      <button class="sf-btn sf-btn--icon" type="button" title="Закрыть" @click="emit('cancel')">
        <X :size="15" />
      </button>
    </header>

    <div class="sf-grid-2">
      <label class="sf-field-wrap">
        <span class="sf-field-label">Название</span>
        <input v-model="name" class="sf-field" maxlength="48" placeholder="Например: Выживание с модами" />
      </label>

      <label class="sf-field-wrap">
        <span class="sf-field-label">Версия Minecraft</span>
        <input
          v-model="versionId"
          class="sf-field"
          list="sf-version-options"
          :disabled="!creating"
          placeholder="1.21.1"
          autocomplete="off"
        />
        <datalist id="sf-version-options">
          <option v-for="row in versionRows.slice(0, 400)" :key="row.id" :value="row.id">
            {{ row.type_label }}
          </option>
        </datalist>
      </label>

      <label class="sf-field-wrap">
        <span class="sf-field-label">Загрузчик</span>
        <select v-model="loader" class="sf-field" :disabled="!creating">
          <option v-for="value in SELECTABLE" :key="value" :value="value">
            {{ MOD_LOADER_LABELS[value] }}
          </option>
        </select>
      </label>

      <label class="sf-field-wrap">
        <span class="sf-field-label">Версия загрузчика</span>
        <select
          v-model="loaderVersion"
          class="sf-field"
          :disabled="!creating || loader === 'none' || !options.length"
        >
          <option v-if="!options.length" value="">
            {{ loader === 'none' ? '— не нужна —' : 'нет доступных версий' }}
          </option>
          <option v-for="option in options" :key="option.version" :value="option.version">
            {{ option.label }}{{ option.stable ? '' : ' (нестабильная)' }}
          </option>
        </select>
      </label>

      <label class="sf-field-wrap">
        <span class="sf-field-label">Оперативная память, МБ</span>
        <input v-model.number="ramMb" class="sf-field" type="number" min="512" max="32768" step="256" placeholder="по умолчанию" />
      </label>

      <label class="sf-field-wrap">
        <span class="sf-field-label">Размер окна</span>
        <span class="sf-field-pair">
          <input v-model.number="width" class="sf-field" type="number" min="320" max="7680" placeholder="1280" />
          <span class="sf-muted">×</span>
          <input v-model.number="height" class="sf-field" type="number" min="240" max="4320" placeholder="720" />
        </span>
      </label>
    </div>

    <p v-if="versionsLoading" class="sf-muted sf-buildeditor__hint">Загружаем список версий…</p>
    <p v-else-if="catalogLoading" class="sf-muted sf-buildeditor__hint">
      Загружаем версии {{ MOD_LOADER_LABELS[loader] }}…
    </p>
    <p v-else-if="catalogError" class="sf-card__note">{{ catalogError }}</p>
    <template v-else-if="catalog?.notes.length">
      <p v-for="note in catalog.notes" :key="note" class="sf-card__note">{{ note }}</p>
    </template>

    <p v-if="creating" class="sf-card__note">
      Недоступные пока загрузчики:
      <template v-for="(value, index) in UNAVAILABLE" :key="value">
        {{ index ? ', ' : '' }}{{ MOD_LOADER_LABELS[value] }}
      </template>
      — выберите Fabric, Forge или NeoForge. Иконку можно добавить после создания.
    </p>

    <div class="sf-grid-2 sf-buildeditor__args">
      <label class="sf-field-wrap">
        <span class="sf-field-label">Дополнительные аргументы JVM</span>
        <textarea v-model="jvmText" class="sf-field sf-field--area" rows="3" placeholder="-XX:+UseG1GC&#10;-Dsun.rmi.dgc.server.gcInterval=2147483646" />
      </label>
      <label class="sf-field-wrap">
        <span class="sf-field-label">Аргументы игры</span>
        <textarea v-model="gameText" class="sf-field sf-field--area" rows="3" placeholder="--fullscreen" />
      </label>
    </div>

    <div class="sf-buildeditor__foot">
      <div v-if="!creating" class="sf-buildeditor__icon">
        <span class="sf-buildeditor__preview" aria-hidden="true">
          <img v-if="build?.iconUrl" :src="build.iconUrl" alt="" />
          <ImagePlus v-else :size="16" />
        </span>
        <button class="sf-btn sf-btn--small" type="button" :disabled="iconBusy" @click="chooseIcon">
          Выбрать иконку
        </button>
        <button
          v-if="build?.iconUrl"
          class="sf-btn sf-btn--small sf-btn--soft"
          type="button"
          :disabled="iconBusy"
          @click="dropIcon"
        >
          Убрать
        </button>
      </div>

      <div class="sf-buildeditor__actions">
        <template v-if="!creating && armedRemove">
          <span class="sf-instance__confirm">Удалить сборку вместе с мирами?</span>
          <button
            class="sf-btn sf-btn--danger"
            type="button"
            @click="emit('removed', build!.slug)"
          >
            Удалить
          </button>
          <button class="sf-btn sf-btn--soft" type="button" @click="armedRemove = false">
            Отмена
          </button>
        </template>
        <template v-else>
          <button
            v-if="!creating"
            class="sf-btn sf-btn--icon"
            type="button"
            title="Удалить сборку"
            @click="armedRemove = true"
          >
            <Trash2 :size="15" />
          </button>
          <button class="sf-btn sf-btn--soft" type="button" @click="emit('cancel')">Отмена</button>
          <button class="sf-btn" type="button" :disabled="!canSave" @click="save">
            <Save :size="15" />
            {{ creating ? 'Создать' : 'Сохранить' }}
          </button>
        </template>
      </div>
    </div>
  </section>
</template>

<style scoped>
@import '../views/panel.css';

.sf-buildeditor__hint {
  margin: 0.6rem 0 0;
  font-size: 0.74rem;
}

.sf-buildeditor__args {
  margin-top: 0.6rem;
}

.sf-field-wrap {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
  min-width: 0;
}

.sf-field-label {
  font-size: 0.72rem;
  color: var(--sf-text-muted);
}

.sf-field-pair {
  display: flex;
  align-items: center;
  gap: 0.35rem;
}

.sf-field--area {
  resize: vertical;
  font-family: var(--sf-font-mono);
  font-size: 0.74rem;
  line-height: 1.4;
}

.sf-buildeditor__foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.6rem;
  flex-wrap: wrap;
  margin-top: 0.9rem;
  padding-top: 0.7rem;
  border-top: var(--sf-border-width) solid var(--sf-border);
}

.sf-buildeditor__icon,
.sf-buildeditor__actions {
  display: flex;
  align-items: center;
  gap: 0.4rem;
}

.sf-buildeditor__preview {
  display: grid;
  place-items: center;
  width: 2rem;
  height: 2rem;
  border-radius: var(--sf-radius-sm);
  overflow: hidden;
  background: color-mix(in srgb, var(--sf-accent) 16%, transparent);
  color: var(--sf-accent);
}

.sf-buildeditor__preview img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}
</style>
