<script setup lang="ts">
/**
 * Build picker.
 *
 * The centre of the home screen. A *build* is the unit a player plays — its own
 * version, loader, memory and mods — so this is what the Play button acts on.
 * There is deliberately no bare "version" selector anywhere: playing 1.12.2
 * means having a build that uses 1.12.2, so two builds on one version can differ
 * in loader, memory and mods.
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { ChevronDown, Layers, Plus } from '@lucide/vue'

import { formatBytes, formatShortDate } from '@/lib/format'
import { selectBuild, useBuilds } from '@/stores/builds'
import { MOD_LOADER_LABELS } from '@/types'
import type { BuildSummary, InstanceState } from '@/types'

const emit = defineEmits<{
  (event: 'create'): void
  (event: 'manage'): void
}>()

const store = useBuilds()
const open = ref(false)
const root = ref<HTMLElement | null>(null)

const STATE_LABEL: Record<InstanceState, string> = {
  empty: 'не установлена',
  partial: 'нужно докачать',
  ready: 'готова',
}

const STATE_CHIP: Record<InstanceState, string> = {
  empty: '',
  partial: 'sf-chip--warn',
  ready: 'sf-chip--ok',
}

const current = computed(() => store.selected.value)

function describe(build: BuildSummary): string {
  const parts = [build.versionId]
  if (build.loader !== 'none') {
    const version = build.loaderVersion ? ` ${build.loaderVersion}` : ''
    parts.push(`${MOD_LOADER_LABELS[build.loader]}${version}`)
  }
  if (build.ramMb) parts.push(`${build.ramMb} МБ`)
  return parts.join(' · ')
}

async function choose(build: BuildSummary): Promise<void> {
  open.value = false
  await selectBuild(build.slug)
}

function onDocumentClick(event: MouseEvent): void {
  if (!open.value) return
  const target = event.target as Node | null
  if (target && root.value && !root.value.contains(target)) open.value = false
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === 'Escape') open.value = false
}

onMounted(() => {
  document.addEventListener('click', onDocumentClick)
  document.addEventListener('keydown', onKeydown)
})

onBeforeUnmount(() => {
  document.removeEventListener('click', onDocumentClick)
  document.removeEventListener('keydown', onKeydown)
})
</script>

<template>
  <div ref="root" class="sf-picker">
    <button
      v-if="current"
      class="sf-picker__current"
      type="button"
      :aria-expanded="open"
      aria-haspopup="listbox"
      @click="open = !open"
    >
      <span class="sf-picker__icon" aria-hidden="true">
        <img v-if="current.iconUrl" :src="current.iconUrl" alt="" />
        <Layers v-else :size="18" />
      </span>
      <span class="sf-picker__text">
        <span class="sf-picker__name">{{ current.name }}</span>
        <span class="sf-picker__meta sf-muted">
          {{ describe(current) }}
          <span v-if="current.lastPlayedAt">
            · {{ formatShortDate(current.lastPlayedAt) }}
          </span>
        </span>
      </span>
      <span class="sf-chip" :class="STATE_CHIP[current.state]">
        {{ STATE_LABEL[current.state] }}
      </span>
      <ChevronDown :size="16" class="sf-picker__chevron" :class="{ 'is-open': open }" />
    </button>

    <button v-else class="sf-picker__empty" type="button" @click="emit('create')">
      <Plus :size="16" />
      <span>Создать первую сборку</span>
    </button>

    <div v-if="open && store.list.value.length" class="sf-picker__menu" role="listbox">
      <button
        v-for="build in store.list.value"
        :key="build.slug"
        class="sf-picker__option"
        type="button"
        role="option"
        :aria-selected="build.slug === current?.slug"
        @click="choose(build)"
      >
        <span class="sf-picker__icon sf-picker__icon--sm" aria-hidden="true">
          <img v-if="build.iconUrl" :src="build.iconUrl" alt="" />
          <Layers v-else :size="14" />
        </span>
        <span class="sf-picker__text">
          <span class="sf-picker__name">{{ build.name }}</span>
          <span class="sf-picker__meta sf-muted">
            {{ describe(build) }} · {{ formatBytes(build.sizeBytes) }}
          </span>
        </span>
      </button>
      <div class="sf-picker__menu-foot">
        <button class="sf-link" type="button" @click="emit('manage')">
          Управление сборками
        </button>
        <button class="sf-link" type="button" @click="emit('create')">Новая сборка</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.sf-picker {
  position: relative;
  width: min(30rem, 100%);
}

.sf-picker__current,
.sf-picker__empty {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  width: 100%;
  padding: 0.5rem 0.7rem;
  border-radius: var(--sf-radius);
  border: var(--sf-border-width) solid var(--sf-border);
  background: color-mix(in srgb, var(--sf-surface-a) 82%, transparent);
  color: var(--sf-text);
  cursor: pointer;
  text-align: left;
  transition:
    background-color var(--sf-dur-fast) var(--sf-ease),
    border-color var(--sf-dur-fast) var(--sf-ease);
}

.sf-picker__current:hover,
.sf-picker__empty:hover {
  background: color-mix(in srgb, var(--sf-text) 8%, transparent);
  border-color: color-mix(in srgb, var(--sf-accent) 40%, var(--sf-border));
}

.sf-picker__empty {
  justify-content: center;
  color: var(--sf-text-muted);
  font-size: 0.85rem;
}

.sf-picker__icon {
  flex: none;
  display: grid;
  place-items: center;
  width: 2.1rem;
  height: 2.1rem;
  border-radius: var(--sf-radius-sm);
  overflow: hidden;
  background: color-mix(in srgb, var(--sf-accent) 18%, transparent);
  color: var(--sf-accent);
}

.sf-picker__icon--sm {
  width: 1.6rem;
  height: 1.6rem;
}

.sf-picker__icon img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.sf-picker__text {
  display: flex;
  flex-direction: column;
  gap: 0.05rem;
  min-width: 0;
  flex: 1;
}

.sf-picker__name {
  font-size: 0.9rem;
  font-weight: var(--sf-weight-bold);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.sf-picker__meta {
  font-size: 0.7rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.sf-picker__chevron {
  flex: none;
  transition: transform var(--sf-dur-fast) var(--sf-ease);
}

.sf-picker__chevron.is-open {
  transform: rotate(180deg);
}

.sf-picker__menu {
  position: absolute;
  z-index: 40;
  inset-inline: 0;
  top: calc(100% + 0.3rem);
  padding: 0.3rem;
  border-radius: var(--sf-radius);
  border: var(--sf-border-width) solid var(--sf-border);
  background: color-mix(in srgb, var(--sf-surface-a) 96%, #000 4%);
  backdrop-filter: blur(22px) saturate(140%);
  box-shadow: var(--sf-shadow-lg);
  max-height: 18rem;
  overflow-y: auto;
}

.sf-picker__option {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  width: 100%;
  padding: 0.35rem 0.45rem;
  border: 0;
  border-radius: var(--sf-radius-sm);
  background: transparent;
  color: inherit;
  cursor: pointer;
  text-align: left;
}

.sf-picker__option:hover,
.sf-picker__option[aria-selected='true'] {
  background: color-mix(in srgb, var(--sf-text) 9%, transparent);
}

.sf-picker__menu-foot {
  display: flex;
  justify-content: space-between;
  gap: 0.5rem;
  margin-top: 0.25rem;
  padding: 0.35rem 0.45rem 0.15rem;
  border-top: var(--sf-border-width) solid var(--sf-border);
}

.sf-link {
  border: 0;
  background: none;
  padding: 0;
  color: var(--sf-accent);
  font-size: 0.72rem;
  cursor: pointer;
}

.sf-link:hover {
  text-decoration: underline;
}
</style>
