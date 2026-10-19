<script setup lang="ts">
/**
 * The primary action.
 *
 * Deliberately the only large control on screen. It shows the real state of
 * the launcher: idle, preparing, launching, or running — a launcher that
 * pretends to be ready while it downloads a 200 MB jar is worse than one that
 * says so.
 */
import { computed } from 'vue'
import { Play, Loader } from '@lucide/vue'

const props = defineProps<{
  disabled: boolean
  busy: boolean
  running: boolean
  label?: string
}>()

defineEmits<{ (event: 'activate'): void }>()

const text = computed(() => {
  if (props.running) return 'Игра запущена'
  if (props.busy) return 'Подготовка…'
  return props.label ?? 'Играть'
})
</script>

<template>
  <button
    class="sf-play sf-no-drag"
    type="button"
    :disabled="disabled || running"
    :aria-busy="busy || running"
    @click="$emit('activate')"
  >
    <Loader v-if="busy" :size="20" :stroke-width="2.4" class="animate-spin" />
    <Play v-else :size="20" :stroke-width="2.4" fill="currentColor" />
    <span>{{ text }}</span>
  </button>
</template>