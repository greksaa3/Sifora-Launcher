<script setup lang="ts">
/**
 * Promoted capabilities on the launch screen.
 *
 * A theme decides which of the launcher's existing features deserve a direct
 * place on the home screen. The list comes from the closed `Shortcut` enum —
 * there is no way for a theme to name an action the application does not have.
 */
import { Boxes, Gauge, Image, Palette, Puzzle, ScrollText, Settings, Shuffle } from '@lucide/vue'

import { computed, type Component } from 'vue'

import type { Shortcut } from '@/types'

const props = defineProps<{ shortcuts: Shortcut[] }>()
const emit = defineEmits<{ (e: 'run', shortcut: Shortcut): void }>()

const ENTRIES: Record<Shortcut, { label: string; icon: Component }> = {
  appearance: { label: 'Оформление', icon: Palette },
  instances: { label: 'Сборки', icon: Boxes },
  mods: { label: 'Моды', icon: Puzzle },
  settings: { label: 'Настройки', icon: Settings },
  log: { label: 'Журнал', icon: ScrollText },
  'random-theme': { label: 'Случайная тема', icon: Shuffle },
  'toggle-background': { label: 'Сменить фон', icon: Image },
  performance: { label: 'Производительность', icon: Gauge },
}

const items = computed(() =>
  props.shortcuts
    .filter((s): s is Shortcut => s in ENTRIES)
    .map((id) => ({ id, ...ENTRIES[id] })),
)
</script>

<template>
  <div v-if="items.length" class="sf-shortcuts">
    <button
      v-for="item in items"
      :key="item.id"
      class="sf-btn sf-btn--soft sf-shortcut"
      type="button"
      :title="item.label"
      @click="emit('run', item.id)"
    >
      <component :is="item.icon" :size="16" />
      <span class="sf-shortcut__label">{{ item.label }}</span>
    </button>
  </div>
</template>

<style scoped>
.sf-shortcuts {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 0.4rem;
  margin-top: 1rem;
}

.sf-shortcut {
  gap: 0.4rem;
  padding: 0.4rem 0.75rem;
  font-size: 0.74rem;
}

.sf-shortcut__label {
  white-space: nowrap;
}
</style>