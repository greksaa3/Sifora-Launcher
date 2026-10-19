<script setup lang="ts">
/**
 * Background chooser.
 *
 * The user's own picture is the only image that can be used, and it is copied
 * into the app data folder by Rust before it ever reaches this component — so
 * the renderer only ever holds a managed path.
 */
import { ref } from 'vue'
import { AnimatePresence, motion } from 'motion-v'
import { Image, ImagePlus, Trash2 } from '@lucide/vue'

import { convertFileSrc } from '@tauri-apps/api/core'

import { backgrounds } from '@/lib/ipc'
import { formatBytes } from '@/lib/format'
import { pushToast, reportError } from '@/stores/app'
import type { BackgroundImage, ThemeSummary } from '@/types'

const props = defineProps<{
  open: boolean
  builtins: ThemeSummary[]
  images: BackgroundImage[]
  activePresetId: string | null
  activeImageId: string | null
}>()

const emit = defineEmits<{
  (e: 'update:open', value: boolean): void
  (e: 'choosePreset', id: string): void
  (e: 'chooseImage', image: BackgroundImage | null): void
}>()

const busy = ref(false)

/** Renderer-loadable URL for a managed background file. */
function toAssetUrl(path: string): string {
  try {
    return convertFileSrc(path)
  } catch {
    return ''
  }
}

async function pick(): Promise<void> {
  busy.value = true
  try {
    const picked = await backgrounds.pick()
    if (!picked) return
    emit('chooseImage', picked)
    emit('update:open', false)
    pushToast('success', 'Фон загружен', picked.fileName)
  } catch (error) {
    reportError(error)
  } finally {
    busy.value = false
  }
}

async function remove(image: BackgroundImage): Promise<void> {
  try {
    await backgrounds.remove(image.id)
    emit('chooseImage', null)
    emit('update:open', false)
    pushToast('info', 'Фон удалён')
  } catch (error) {
    reportError(error)
  }
}
</script>

<template>
  <AnimatePresence>
    <motion.div
      v-if="open"
      class="sf-bg__sheet sf-panel"
      :initial="{ opacity: 0, y: -10, scale: 0.98 }"
      :animate="{ opacity: 1, y: 0, scale: 1 }"
      :exit="{ opacity: 0, y: -8, scale: 0.99 }"
      :transition="{ type: 'spring', stiffness: 380, damping: 30 }"
      role="dialog"
      aria-label="Выбор фона"
    >
      <header class="sf-bg__header">
        <span class="sf-label">Фон</span>
        <button class="sf-btn sf-btn--outline" type="button" @click="emit('update:open', false)">
          Готово
        </button>
      </header>

      <section class="sf-bg__section">
        <p class="sf-label">Встроенные</p>
        <div class="sf-bg__grid">
          <button
            v-for="preset in builtins"
            :key="preset.id"
            class="sf-bg__swatch"
            type="button"
            :title="preset.description"
            :aria-label="preset.name"
            :aria-pressed="activePresetId === preset.id"
            :class="{ 'sf-bg__swatch--active': activePresetId === preset.id }"
            :style="{
              '--swatch-accent': preset.accent,
              '--swatch-bg': preset.background,
            }"
            @click="emit('choosePreset', preset.id)"
          >
            <span class="sf-bg__swatch-label">{{ preset.name }}</span>
          </button>
        </div>
      </section>

      <section class="sf-bg__section">
        <p class="sf-label">Своё изображение</p>
        <button
          class="sf-bg__upload"
          type="button"
          :disabled="busy"
          @click="pick"
        >
          <ImagePlus :size="18" />
          <span>{{ busy ? 'Копируем…' : 'Выбрать файл' }}</span>
          <span class="sf-bg__upload-hint sf-muted">PNG, JPG, WebP · до 48 МБ</span>
        </button>

        <ul v-if="images.length" class="sf-bg__list">
          <li v-for="image in images" :key="image.id" class="sf-bg__item">
            <img class="sf-bg__thumb" :src="toAssetUrl(image.path)" :alt="image.fileName" />
            <span class="sf-bg__item-meta">
              <span class="sf-bg__item-name">{{ image.fileName }}</span>
              <span class="sf-muted">{{ formatBytes(image.sizeBytes) }}</span>
            </span>
            <button
              class="sf-btn sf-btn--icon"
              type="button"
              :aria-label="`Выбрать фон ${image.fileName}`"
              :aria-pressed="activeImageId === image.id"
              @click="emit('chooseImage', image)"
            >
              <Image :size="16" />
            </button>
            <button
              class="sf-btn sf-btn--icon"
              type="button"
              :aria-label="`Удалить ${image.fileName}`"
              @click="remove(image)"
            >
              <Trash2 :size="16" />
            </button>
          </li>
        </ul>
      </section>
    </motion.div>
  </AnimatePresence>
</template>

<style scoped>
.sf-bg__sheet {
  position: absolute;
  top: calc(100% + 0.55rem);
  left: 0;
  width: min(26rem, 84vw);
  max-height: min(30rem, 74vh);
  overflow-y: auto;
  padding: 0.85rem;
  display: flex;
  flex-direction: column;
  gap: 0.9rem;
  z-index: 40;
}

.sf-bg__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.sf-bg__header .sf-btn {
  padding: 0.3rem 0.8rem;
  font-size: 0.78rem;
}

.sf-bg__section {
  display: flex;
  flex-direction: column;
  gap: 0.45rem;
}

.sf-bg__grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(5.5rem, 1fr));
  gap: 0.4rem;
}

.sf-bg__swatch {
  position: relative;
  aspect-ratio: 16 / 10;
  border-radius: var(--sf-radius-sm);
  border: 2px solid transparent;
  background:
    radial-gradient(60% 60% at 30% 20%, var(--swatch-bg), transparent 70%),
    linear-gradient(140deg, var(--swatch-bg), color-mix(in srgb, var(--swatch-accent) 45%, #000));
  overflow: hidden;
  transition:
    border-color var(--sf-dur-fast) var(--sf-ease),
    transform var(--sf-dur-fast) var(--sf-ease);
}

.sf-bg__swatch:hover {
  transform: translateY(calc(var(--sf-hover-lift) * -1));
}

.sf-bg__swatch--active {
  border-color: var(--sf-accent);
}

.sf-bg__swatch-label {
  position: absolute;
  inset: auto 0 0 0;
  padding: 0.25rem 0.4rem;
  font-size: 0.66rem;
  font-weight: var(--sf-weight-bold);
  color: #fff;
  background: rgb(0 0 0 / 0.45);
  text-align: left;
}

.sf-bg__upload {
  display: flex;
  align-items: center;
  gap: 0.55rem;
  padding: 0.7rem 0.85rem;
  border-radius: var(--sf-radius);
  border: 1px dashed var(--sf-border-strong);
  color: var(--sf-text);
  font-size: 0.85rem;
  transition:
    border-color var(--sf-dur-fast) var(--sf-ease),
    background-color var(--sf-dur-fast) var(--sf-ease);
}

.sf-bg__upload:hover:not(:disabled) {
  border-color: var(--sf-accent);
  background: var(--sf-accent-a);
}

.sf-bg__upload-hint {
  margin-left: auto;
  font-size: 0.7rem;
}

.sf-bg__list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 0.3rem;
}

.sf-bg__item {
  display: flex;
  align-items: center;
  gap: 0.55rem;
  padding: 0.3rem;
  border-radius: var(--sf-radius-sm);
  transition: background-color var(--sf-dur-fast) var(--sf-ease);
}

.sf-bg__item:hover {
  background: color-mix(in srgb, var(--sf-text) 7%, transparent);
}

.sf-bg__thumb {
  width: 3.2rem;
  height: 2.1rem;
  border-radius: 6px;
  object-fit: cover;
  flex: none;
  border: 1px solid var(--sf-border);
}

.sf-bg__item-meta {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-width: 0;
  font-size: 0.72rem;
}

.sf-bg__item-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>