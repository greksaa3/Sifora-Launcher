<script setup lang="ts">
/**
 * Top-left menu button.
 *
 * Stage 1 ships the entry point only. It is intentionally *not* a dead button:
 * clicking it reports that the section is on the roadmap instead of doing
 * nothing, and the command surface it will drive already exists in Rust.
 */
import { ref } from 'vue'
import { Menu } from '@lucide/vue'

import { pushToast } from '@/stores/app'

const open = ref(false)
let timer: number | undefined

function handleClick(): void {
  open.value = true
  pushToast(
    'info',
    'Разделы лаунчера появятся на втором этапе',
    'Настройки, загрузка модов и управление сборками уже подготовлены в бэкенде.',
  )
  window.clearTimeout(timer)
  timer = window.setTimeout(() => (open.value = false), 1600)
}
</script>

<template>
  <button
    class="sf-btn sf-btn--icon sf-menu sf-no-drag"
    type="button"
    aria-label="Меню лаунчера"
    :title="open ? 'Готовится' : 'Меню'"
    @click="handleClick"
  >
    <Menu :size="18" />
  </button>
</template>

<style scoped>
.sf-menu {
  background: var(--sf-surface-a);
  border: var(--sf-border-width) solid var(--sf-border);
  backdrop-filter: blur(14px);
  box-shadow: var(--sf-shadow-sm);
}
</style>