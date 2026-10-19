<script setup lang="ts">
/**
 * Version picker.
 *
 * Sits directly under the Play button. The trigger is compact and always shows
 * what will actually launch; the popover is a searchable list grouped by kind
 * (recommended / installed / releases / snapshots / beta / alpha) so ~900
 * entries stay navigable without a complex filter bar.
 *
 * Motion for Vue is used here — and only here — because the popover needs a
 * spring that reacts to the pointer leaving the trigger, which plain CSS
 * transitions express awkwardly.
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { AnimatePresence, motion } from 'motion-v'
import { Check, ChevronDown, CircleSlash, Download, Search, X } from '@lucide/vue'

import { formatReleaseDate } from '@/lib/format'
import type { ReleaseType, VersionRow } from '@/types'

const props = defineProps<{
  rows: VersionRow[]
  sections: Array<{ key: string; label: string; rows: VersionRow[] }>
  selected: VersionRow | null
  query: string
  typeFilter: ReleaseType | 'all'
  loading: boolean
  error: string | null
}>()

const emit = defineEmits<{
  (e: 'update:query', value: string): void
  (e: 'update:typeFilter', value: ReleaseType | 'all'): void
  (e: 'select', row: VersionRow): void
  (e: 'retry'): void
}>()

/** The dropdown owns its open state; the parent only feeds it data. */
const open = ref(false)

const searchInput = ref<HTMLInputElement | null>(null)
const listBox = ref<HTMLElement | null>(null)
const highlighted = ref(0)

const FILTERS: Array<{ value: ReleaseType | 'all'; label: string }> = [
  { value: 'all', label: 'Все' },
  { value: 'release', label: 'Релизы' },
  { value: 'snapshot', label: 'Снапшоты' },
  { value: 'old_beta', label: 'Beta' },
  { value: 'old_alpha', label: 'Alpha' },
]

const flatRows = computed(() => props.sections.flatMap((section) => section.rows))

const empty = computed(() => !props.loading && props.rows.length > 0 && flatRows.value.length === 0)

watch(
  open,
  async (value) => {
    if (value) {
      await nextTick()
      searchInput.value?.focus()
      highlighted.value = Math.max(
        0,
        flatRows.value.findIndex((row) => row.id === props.selected?.id),
      )
    }
  },
)

watch(
  () => [props.query, props.typeFilter],
  () => {
    highlighted.value = 0
  },
)

function close(): void {
  open.value = false
}

function toggle(): void {
  open.value = !open.value
}



function choose(row: VersionRow): void {
  emit('select', row)
  close()
}

function move(delta: number): void {
  const total = flatRows.value.length
  if (!total) return
  highlighted.value = (highlighted.value + delta + total) % total
  scrollHighlightedIntoView()
}

function jumpToEdge(edge: 'first' | 'last'): void {
  const total = flatRows.value.length
  if (!total) return
  highlighted.value = edge === 'first' ? 0 : total - 1
  scrollHighlightedIntoView()
}

function scrollHighlightedIntoView(): void {
  void nextTick(() => {
    const box = listBox.value
    if (!box) return
    const active = box.querySelector<HTMLElement>('[data-highlighted="true"]')
    active?.scrollIntoView({ block: 'nearest' })
  })
}

function onKeydown(event: KeyboardEvent): void {
  switch (event.key) {
    case 'ArrowDown':
      event.preventDefault()
      move(1)
      break
    case 'ArrowUp':
      event.preventDefault()
      move(-1)
      break
    case 'Home':
      event.preventDefault()
      jumpToEdge('first')
      break
    case 'End':
      event.preventDefault()
      jumpToEdge('last')
      break
    case 'Enter': {
      event.preventDefault()
      const row = flatRows.value[highlighted.value]
      if (row) choose(row)
      break
    }
    case 'Escape':
      event.preventDefault()
      close()
      break
  }
}

function onPointerMove(event: PointerEvent): void {
  const id = (event.target as HTMLElement | null)?.closest<HTMLElement>('[data-version-id]')
    ?.dataset.versionId
  if (!id) return
  const index = flatRows.value.findIndex((row) => row.id === id)
  if (index >= 0) highlighted.value = index
}

function onPointerDownOutside(event: PointerEvent): void {
  const target = event.target as HTMLElement | null
  if (target?.closest('.sf-version')) return
  close()
}

onMounted(() => document.addEventListener('pointerdown', onPointerDownOutside))
onBeforeUnmount(() => {
  document.removeEventListener('keydown', onKeydown)
  document.removeEventListener('pointerdown', onPointerDownOutside)
})

watch(
  open,
  (value) => {
    if (value) document.addEventListener('keydown', onKeydown)
    else document.removeEventListener('keydown', onKeydown)
  },
)

function statusChip(row: VersionRow): { text: string; kind: string } {
  if (!row.supported) return { text: 'не поддерживается', kind: 'bad' }
  if (row.installed) return { text: 'установлена', kind: 'ok' }
  return { text: 'нужно загрузить', kind: '' }
}
</script>

<template>
  <div class="sf-version">
    <!-- Trigger -->
    <button
      class="sf-version__trigger sf-no-drag"
      type="button"
      :aria-expanded="open"
      aria-haspopup="listbox"
      @click="toggle()"
    >
      <span v-if="selected" class="sf-version__id">{{ selected.id }}</span>
      <span v-else class="sf-version__id sf-muted">Выберите версию</span>

      <span v-if="selected" class="sf-chip" :class="{ 'sf-chip--ok': selected.installed }">
        {{ selected.type_label }}
      </span>
      <span v-else-if="loading" class="sf-chip">загрузка…</span>

      <ChevronDown
        :size="16"
        class="sf-version__chevron"
        :class="{ 'sf-version__chevron--open': open }"
      />
    </button>

    <p v-if="selected && !selected.supported" class="sf-version__hint">
      {{ selected.unsupported_reason }}
    </p>

    <!-- Popover -->
    <AnimatePresence>
      <motion.div
        v-if="open"
        class="sf-version__popover sf-panel"
        :initial="{ opacity: 0, y: -8, scale: 0.985 }"
        :animate="{ opacity: 1, y: 0, scale: 1 }"
        :exit="{ opacity: 0, y: -6, scale: 0.99 }"
        :transition="{ type: 'spring', stiffness: 420, damping: 32, mass: 0.7 }"
        role="dialog"
        aria-label="Выбор версии Minecraft"
      >
        <div class="sf-version__search">
          <Search :size="15" class="sf-version__search-icon" />
          <input
            ref="searchInput"
            class="sf-field sf-version__input"
            type="search"
            autocomplete="off"
            spellcheck="false"
            placeholder="Поиск версии — например 1.20.4"
            :value="query"
            @input="emit('update:query', ($event.target as HTMLInputElement).value)"
          />
          <button
            v-if="query"
            class="sf-btn sf-btn--ghost sf-version__clear"
            type="button"
            aria-label="Очистить поиск"
            @click="emit('update:query', '')"
          >
            <X :size="14" />
          </button>
        </div>

        <div class="sf-version__filters" role="tablist" aria-label="Тип версии">
          <button
            v-for="filter in FILTERS"
            :key="filter.value"
            class="sf-version__filter"
            type="button"
            role="tab"
            :aria-selected="typeFilter === filter.value"
            :class="{ 'sf-version__filter--active': typeFilter === filter.value }"
            @click="emit('update:typeFilter', filter.value)"
          >
            {{ filter.label }}
          </button>
        </div>

        <div ref="listBox" class="sf-version__list" role="listbox" @pointermove="onPointerMove">
          <div v-if="loading && flatRows.length === 0" class="sf-version__empty">
            Загружаем список версий с Mojang…
          </div>

          <div v-else-if="error" class="sf-version__empty">
            <p>{{ error }}</p>
            <button class="sf-btn sf-btn--outline sf-version__retry" type="button" @click="emit('retry')">
              Повторить
            </button>
          </div>

          <div v-else-if="empty" class="sf-version__empty">
            Ничего не найдено по запросу «{{ query }}»
          </div>

          <template v-for="section in sections" v-else :key="section.key">
            <p v-if="section.rows.length" class="sf-version__section">{{ section.label }}</p>
            <button
              v-for="row in section.rows"
              :key="row.id"
              class="sf-version__row"
              type="button"
              role="option"
              :aria-selected="selected?.id === row.id"
              :data-version-id="row.id"
              :data-highlighted="flatRows[highlighted]?.id === row.id"
              :class="{
                'sf-version__row--active': selected?.id === row.id,
                'sf-version__row--disabled': !row.supported,
              }"
              @click="choose(row)"
            >
              <Check v-if="selected?.id === row.id" :size="15" class="sf-version__check" />
              <span v-else class="sf-version__check sf-version__check--empty" />

              <span class="sf-version__main">
                <span class="sf-version__row-id sf-mono">{{ row.id }}</span>
                <span class="sf-version__row-meta">
                  {{ formatReleaseDate(row.release_time) }}
                  <template v-if="row.java_major"> · Java {{ row.java_major }}</template>
                </span>
              </span>

              <span class="sf-chip" :class="`sf-chip--${statusChip(row).kind || 'plain'}`">
                <Download v-if="!row.installed && row.supported" :size="11" />
                <CircleSlash v-else-if="!row.supported" :size="11" />
                {{ row.type_label }}
              </span>
            </button>
          </template>
        </div>

        <footer class="sf-version__footer sf-muted">
          <span>Список версий — официальный манифест Mojang</span>
          <span>{{ rows.length }} записей</span>
        </footer>
      </motion.div>
    </AnimatePresence>
  </div>
</template>

<style scoped>
.sf-version {
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.4rem;
}

.sf-version__trigger {
  display: inline-flex;
  align-items: center;
  gap: 0.6rem;
  min-width: 15rem;
  justify-content: space-between;
  padding: 0.45rem 0.5rem 0.45rem 0.95rem;
  border-radius: var(--sf-radius-pill);
  background: var(--sf-surface-a);
  border: var(--sf-border-width) solid var(--sf-border);
  backdrop-filter: blur(16px) saturate(120%);
  box-shadow: var(--sf-shadow-sm);
  transition:
    border-color var(--sf-dur-fast) var(--sf-ease),
    background-color var(--sf-dur-fast) var(--sf-ease);
}

.sf-version__trigger:hover {
  border-color: var(--sf-border-strong);
  background: var(--sf-raised-a);
}

.sf-version__id {
  font-weight: var(--sf-weight-bold);
  letter-spacing: 0.01em;
}

.sf-version__chevron {
  flex: none;
  color: var(--sf-text-muted);
  transition: transform var(--sf-dur-fast) var(--sf-ease);
}

.sf-version__chevron--open {
  transform: rotate(180deg);
}

.sf-version__hint {
  font-size: 0.74rem;
  color: var(--sf-warning);
  max-width: 22rem;
  text-align: center;
}

.sf-version__popover {
  position: absolute;
  top: calc(100% + 0.6rem);
  left: 50%;
  width: min(34rem, 90vw);
  transform: translateX(-50%);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  z-index: 40;
  padding: 0.6rem;
  gap: 0.55rem;
}

.sf-version__search {
  position: relative;
  display: flex;
  align-items: center;
}

.sf-version__search-icon {
  position: absolute;
  left: 0.75rem;
  color: var(--sf-text-muted);
  pointer-events: none;
}

.sf-version__input {
  padding-left: 2.1rem;
  padding-right: 2.2rem;
}

.sf-version__input::-webkit-search-cancel-button {
  display: none;
}

.sf-version__clear {
  position: absolute;
  right: 0.2rem;
}

.sf-version__filters {
  display: flex;
  gap: 0.25rem;
  flex-wrap: wrap;
}

.sf-version__filter {
  padding: 0.22rem 0.65rem;
  border-radius: var(--sf-radius-pill);
  font-size: 0.74rem;
  font-weight: var(--sf-weight-bold);
  color: var(--sf-text-muted);
  border: var(--sf-border-width) solid transparent;
  transition:
    background-color var(--sf-dur-fast) var(--sf-ease),
    color var(--sf-dur-fast) var(--sf-ease);
}

.sf-version__filter:hover {
  color: var(--sf-text);
}

.sf-version__filter--active {
  color: var(--sf-accent);
  background: var(--sf-accent-a);
  border-color: color-mix(in srgb, var(--sf-accent) 35%, transparent);
}

.sf-version__list {
  max-height: min(24rem, 46vh);
  overflow-y: auto;
  overscroll-behavior: contain;
  display: flex;
  flex-direction: column;
  gap: 1px;
  scroll-padding: 0.5rem;
}

.sf-version__section {
  position: sticky;
  top: 0;
  z-index: 1;
  margin: 0;
  padding: 0.4rem 0.6rem 0.25rem;
  font-size: 0.66rem;
  font-weight: var(--sf-weight-bold);
  letter-spacing: 0.1em;
  text-transform: uppercase;
  color: var(--sf-text-muted);
  background: linear-gradient(var(--sf-raised-a) 70%, transparent);
  backdrop-filter: blur(6px);
}

.sf-version__row {
  display: flex;
  align-items: center;
  gap: 0.55rem;
  width: 100%;
  padding: 0.42rem 0.6rem;
  border-radius: var(--sf-radius-sm);
  text-align: left;
  transition: background-color var(--sf-dur-fast) var(--sf-ease);
}

.sf-version__row[data-highlighted='true'] {
  background: color-mix(in srgb, var(--sf-text) 8%, transparent);
}

.sf-version__row--active {
  background: var(--sf-accent-a);
}

.sf-version__row--disabled {
  opacity: 0.45;
}

.sf-version__check {
  flex: none;
  color: var(--sf-accent);
}

.sf-version__check--empty {
  width: 15px;
}

.sf-version__main {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-width: 0;
}

.sf-version__row-id {
  font-size: 0.86rem;
  font-weight: var(--sf-weight-bold);
}

.sf-version__row-meta {
  font-size: 0.7rem;
  color: var(--sf-text-muted);
}

.sf-version__empty {
  padding: 1.6rem 0.8rem;
  text-align: center;
  color: var(--sf-text-muted);
  font-size: 0.82rem;
}

.sf-version__retry {
  margin-top: 0.6rem;
  padding: 0.3rem 0.8rem;
  font-size: 0.78rem;
}

.sf-version__footer {
  display: flex;
  justify-content: space-between;
  gap: 1rem;
  padding: 0.35rem 0.6rem 0.1rem;
  font-size: 0.68rem;
  border-top: var(--sf-border-width) solid var(--sf-border);
}
</style>