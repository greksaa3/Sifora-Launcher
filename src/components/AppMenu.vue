<script setup lang="ts">
/**
 * Slide-in navigation.
 *
 * Pure navigation: the drawer owns no state beyond its own open flag, and the
 * selected view is decided by the parent. Motion for Vue drives the entrance
 * because a drawer that springs out feels materially better than a CSS slide,
 * and it is the second of only two places this library is used.
 */
import { AnimatePresence, motion } from 'motion-v'
import { Boxes, Info, Palette, Settings, X } from '@lucide/vue'

import type { Component } from 'vue'

export type ViewId = 'home' | 'themes' | 'instances' | 'settings' | 'about'

const props = defineProps<{
  open: boolean
  view: ViewId
  instanceCount: number
  customThemeCount: number
  version: string
}>()

const emit = defineEmits<{
  (e: 'update:open', value: boolean): void
  (e: 'select', view: ViewId): void
}>()

const ITEMS: Array<{ id: ViewId; label: string; icon: Component; hint?: (p: typeof props) => string }> = [
  { id: 'home', label: 'Запуск', icon: Boxes },
  { id: 'themes', label: 'Оформление', icon: Palette },
  { id: 'settings', label: 'Настройки', icon: Settings },
  { id: 'about', label: 'О лаунчере', icon: Info },
]

function go(view: ViewId): void {
  emit('select', view)
  emit('update:open', false)
}
</script>

<template>
  <AnimatePresence>
    <motion.div
      v-if="open"
      class="sf-drawer"
      :initial="{ opacity: 0 }"
      :animate="{ opacity: 1 }"
      :exit="{ opacity: 0 }"
      :transition="{ duration: 0.18 }"
      @click="emit('update:open', false)"
    />

    <motion.aside
      v-if="open"
      class="sf-drawer__panel sf-panel"
      :initial="{ x: '-100%' }"
      :animate="{ x: '0%' }"
      :exit="{ x: '-100%' }"
      :transition="{ type: 'spring', stiffness: 420, damping: 38, mass: 0.8 }"
      aria-label="Навигация"
    >
      <header class="sf-drawer__head">
        <div class="sf-drawer__brand">
          <span class="sf-drawer__mark" />
          <div>
            <p class="sf-drawer__name">Sifora</p>
            <p class="sf-drawer__version sf-muted">v{{ version }}</p>
          </div>
        </div>
        <button
          class="sf-btn sf-btn--icon"
          type="button"
          aria-label="Закрыть меню"
          @click="emit('update:open', false)"
        >
          <X :size="17" />
        </button>
      </header>

      <nav class="sf-drawer__nav">
        <button
          v-for="item in ITEMS"
          :key="item.id"
          class="sf-drawer__item"
          type="button"
          :class="{ 'sf-drawer__item--active': view === item.id }"
          @click="go(item.id)"
        >
          <component :is="item.icon" :size="17" />
          <span>{{ item.label }}</span>
        </button>
      </nav>

      <footer class="sf-drawer__foot sf-muted">
        <p>Загрузчики модов и менеджер сборок — следующий этап.</p>
        <p v-if="instanceCount || customThemeCount" class="sf-drawer__stats">
          {{ instanceCount }} сборок · {{ customThemeCount }} своих тем
        </p>
      </footer>
    </motion.aside>
  </AnimatePresence>
</template>

<style scoped>
.sf-drawer {
  position: fixed;
  inset: 0;
  background: color-mix(in srgb, var(--sf-text) 22%, transparent);
  backdrop-filter: blur(2px);
  z-index: 70;
}

.sf-drawer__panel {
  position: fixed;
  top: 0.6rem;
  bottom: 0.6rem;
  left: 0.6rem;
  width: min(19rem, 82vw);
  display: flex;
  flex-direction: column;
  padding: 1rem 0.8rem 0.8rem;
  z-index: 71;
  border-radius: var(--sf-radius-lg);
  overflow: hidden;
}

.sf-drawer__head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 0.6rem;
  padding: 0.2rem 0.3rem 0.9rem;
  border-bottom: var(--sf-border-width) solid var(--sf-border);
}

.sf-drawer__brand {
  display: flex;
  align-items: center;
  gap: 0.65rem;
}

.sf-drawer__mark {
  width: 34px;
  height: 34px;
  border-radius: 11px;
  background: linear-gradient(
    140deg,
    var(--sf-accent),
    color-mix(in srgb, var(--sf-accent) 35%, var(--sf-text))
  );
  box-shadow: var(--sf-shadow-sm);
  flex: none;
}

.sf-drawer__name {
  margin: 0;
  font-family: var(--sf-font-display);
  font-size: 1.02rem;
  font-weight: var(--sf-weight-bold);
  letter-spacing: 0.04em;
}

.sf-drawer__version {
  margin: 0;
  font-size: 0.68rem;
}

.sf-drawer__nav {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 0.7rem 0;
  flex: 1;
}

.sf-drawer__item {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  padding: 0.55rem 0.7rem;
  border-radius: var(--sf-radius);
  color: var(--sf-text-muted);
  font-size: 0.88rem;
  font-weight: var(--sf-weight-bold);
  text-align: left;
  transition:
    background-color var(--sf-dur-fast) var(--sf-ease),
    color var(--sf-dur-fast) var(--sf-ease);
}

.sf-drawer__item:hover {
  color: var(--sf-text);
  background: color-mix(in srgb, var(--sf-text) 7%, transparent);
}

.sf-drawer__item--active {
  color: var(--sf-accent);
  background: var(--sf-accent-a);
}

.sf-drawer__foot {
  padding: 0.7rem 0.6rem 0.2rem;
  border-top: var(--sf-border-width) solid var(--sf-border);
  font-size: 0.68rem;
  line-height: 1.4;
}

.sf-drawer__foot p {
  margin: 0 0 0.3rem;
}

.sf-drawer__stats {
  color: var(--sf-accent);
}
</style>