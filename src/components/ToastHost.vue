<script setup lang="ts">
/** Transient notifications. Nothing here is persistent or blocking. */
import { AnimatePresence, motion } from 'motion-v'
import { AlertTriangle, CheckCircle2, Info, X, XCircle } from '@lucide/vue'

import { dismissToast, type Toast } from '@/stores/app'

defineProps<{ toasts: Toast[] }>()

const ICONS = {
  info: Info,
  success: CheckCircle2,
  warning: AlertTriangle,
  error: XCircle,
} as const
</script>

<template>
  <div class="sf-toasts sf-no-drag" role="status" aria-live="polite">
    <AnimatePresence>
      <motion.div
        v-for="toast in toasts"
        :key="toast.id"
        class="sf-toast sf-panel"
        :data-kind="toast.kind"
        :initial="{ opacity: 0, y: 12, scale: 0.97 }"
        :animate="{ opacity: 1, y: 0, scale: 1 }"
        :exit="{ opacity: 0, y: 6, scale: 0.98 }"
        :transition="{ type: 'spring', stiffness: 460, damping: 34 }"
        layout
      >
        <component :is="ICONS[toast.kind]" :size="16" class="sf-toast__icon" />
        <div class="sf-toast__body">
          <p class="sf-toast__message">{{ toast.message }}</p>
          <p v-if="toast.detail" class="sf-toast__detail sf-muted">{{ toast.detail }}</p>
        </div>
        <button class="sf-btn sf-btn--ghost sf-toast__close" type="button" aria-label="Закрыть" @click="dismissToast(toast.id)">
          <X :size="14" />
        </button>
      </motion.div>
    </AnimatePresence>
  </div>
</template>

<style scoped>
.sf-toasts {
  position: fixed;
  right: 1rem;
  bottom: 1rem;
  z-index: 60;
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
  width: min(22rem, 60vw);
  pointer-events: none;
}

.sf-toast {
  pointer-events: auto;
  display: flex;
  align-items: flex-start;
  gap: 0.55rem;
  padding: 0.6rem 0.7rem;
  border-radius: var(--sf-radius);
}

.sf-toast__icon {
  flex: none;
  margin-top: 0.1rem;
}

.sf-toast[data-kind='success'] .sf-toast__icon {
  color: var(--sf-success);
}
.sf-toast[data-kind='warning'] .sf-toast__icon {
  color: var(--sf-warning);
}
.sf-toast[data-kind='error'] .sf-toast__icon {
  color: var(--sf-danger);
}
.sf-toast[data-kind='info'] .sf-toast__icon {
  color: var(--sf-accent);
}

.sf-toast__body {
  flex: 1;
  min-width: 0;
}

.sf-toast__message {
  margin: 0;
  font-size: 0.82rem;
  font-weight: var(--sf-weight-bold);
}

.sf-toast__detail {
  margin: 0.1rem 0 0;
  font-size: 0.72rem;
  overflow-wrap: anywhere;
}

.sf-toast__close {
  flex: none;
  padding: 0.2rem;
  border-radius: var(--sf-radius-sm);
}
</style>