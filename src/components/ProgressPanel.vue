<script setup lang="ts">
/**
 * Progress readout shown while an install or a launch is in flight.
 *
 * It reports what Rust reports: aggregate counters plus the last human-readable
 * note. No progress is simulated.
 */
import { computed } from 'vue'
import { AnimatePresence, motion } from 'motion-v'
import { Download, X } from '@lucide/vue'

import { formatBytes } from '@/lib/format'
import type { AggregateProgress } from '@/types'

const props = defineProps<{
  progress: AggregateProgress | null
  label: string | null
  error: string | null
}>()

const emit = defineEmits<{ (e: 'dismiss'): void }>()

const percent = computed(() => {
  const p = props.progress
  if (!p) return 0
  if (p.totalFiles > 0) return Math.round((p.doneFiles / p.totalFiles) * 100)
  if (p.totalBytes > 0) return Math.round((p.bytes / p.totalBytes) * 100)
  return 0
})

const visible = computed(() => Boolean(props.label || props.progress || props.error))
</script>

<template>
  <AnimatePresence>
    <motion.aside
      v-if="visible"
      class="sf-progress-panel sf-panel"
      :initial="{ opacity: 0, y: 16, scale: 0.97 }"
      :animate="{ opacity: 1, y: 0, scale: 1 }"
      :exit="{ opacity: 0, y: 12, scale: 0.98 }"
      :transition="{ type: 'spring', stiffness: 380, damping: 32 }"
      role="status"
      aria-live="polite"
    >
      <div class="sf-progress-panel__head">
        <Download :size="15" />
        <span class="sf-progress-panel__label">{{ label ?? 'Загрузка' }}</span>
        <span v-if="progress && percent" class="sf-progress-panel__percent sf-mono">{{ percent }}%</span>
        <button class="sf-btn sf-btn--ghost sf-progress-panel__close" type="button" aria-label="Скрыть" @click="emit('dismiss')">
          <X :size="14" />
        </button>
      </div>

      <div class="sf-progress">
        <div class="sf-progress__bar" :style="{ width: `${percent}%` }" />
      </div>

      <p v-if="error" class="sf-progress-panel__error">{{ error }}</p>
      <p v-else-if="progress" class="sf-progress-panel__note sf-muted">
        <template v-if="progress.note">{{ progress.note }}</template>
        <template v-else-if="progress.totalBytes > 0">
          {{ formatBytes(progress.bytes) }} / {{ formatBytes(progress.totalBytes) }}
        </template>
        <template v-else-if="progress.totalFiles > 0">
          {{ progress.doneFiles }} / {{ progress.totalFiles }} файлов
        </template>
      </p>
    </motion.aside>
  </AnimatePresence>
</template>

<style scoped>
.sf-progress-panel {
  position: fixed;
  left: 50%;
  bottom: 1.1rem;
  transform: translateX(-50%);
  width: min(28rem, calc(100vw - 3rem));
  padding: 0.7rem 0.9rem;
  display: flex;
  flex-direction: column;
  gap: 0.45rem;
  z-index: 50;
}

.sf-progress-panel__head {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  font-size: 0.8rem;
  color: var(--sf-text-muted);
}

.sf-progress-panel__label {
  flex: 1;
  color: var(--sf-text);
  font-weight: var(--sf-weight-bold);
}

.sf-progress-panel__percent {
  font-size: 0.78rem;
}

.sf-progress-panel__close {
  padding: 0.15rem;
  border-radius: var(--sf-radius-sm);
}

.sf-progress-panel__note,
.sf-progress-panel__error {
  margin: 0;
  font-size: 0.74rem;
}

.sf-progress-panel__error {
  color: var(--sf-danger);
}
</style>