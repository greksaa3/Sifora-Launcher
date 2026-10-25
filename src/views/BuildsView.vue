<script setup lang="ts">
/**
 * Builds.
 *
 * A build is the unit a player plays: its own Minecraft version, mod loader,
 * memory, window size, mods and worlds. Several builds may share one Minecraft
 * version — that is the point, since the difference between two 1.21.1 builds
 * is usually the loader and the mod set.
 *
 * Nothing here is destructive without a second, in-place confirmation: a
 * browser-owned `confirm()` cannot be themed and blocks the whole window.
 */
import { computed, onMounted, ref } from 'vue'
import { Boxes, FolderOpen, Package, Pencil, Play, Plus, Puzzle } from '@lucide/vue'

import BuildEditor from '@/components/BuildEditor.vue'
import { app, builds as buildsApi } from '@/lib/ipc'
import { formatBytes, formatShortDate } from '@/lib/format'
import { deleteBuild, refreshBuilds, selectBuild, useBuilds } from '@/stores/builds'
import { pushToast, reportError } from '@/stores/app'
import { MOD_LOADER_LABELS } from '@/types'
import type { BuildSummary, InstanceState } from '@/types'

const emit = defineEmits<{
  (event: 'open-mods', slug: string): void
  (event: 'played'): void
}>()

const store = useBuilds()
const loading = ref(true)
const editing = ref<BuildSummary | null>(null)
const creating = ref(false)
const installing = ref<string | null>(null)

const STATE_LABEL: Record<InstanceState, string> = {
  empty: 'не установлена',
  partial: 'загрузка не завершена',
  ready: 'готова',
}

const STATE_CHIP: Record<InstanceState, string> = {
  empty: '',
  partial: 'sf-chip--warn',
  ready: 'sf-chip--ok',
}

const editorOpen = computed(() => creating.value || editing.value !== null)

function describe(build: BuildSummary): string {
  const parts = [build.versionId]
  if (build.loader !== 'none') {
    parts.push(`${MOD_LOADER_LABELS[build.loader]} ${build.loaderVersion}`.trim())
  }
  parts.push(formatBytes(build.sizeBytes))
  if (build.ramMb) parts.push(`${build.ramMb} МБ`)
  return parts.join(' · ')
}

async function refresh(): Promise<void> {
  loading.value = true
  try {
    await refreshBuilds()
  } catch (error) {
    reportError(error)
  } finally {
    loading.value = false
  }
}

onMounted(refresh)

function startCreate(): void {
  editing.value = null
  creating.value = true
}

function startEdit(build: BuildSummary): void {
  creating.value = false
  editing.value = build
}

function closeEditor(): void {
  creating.value = false
  editing.value = null
}

function onSaved(build: BuildSummary): void {
  closeEditor()
  void build
}

async function onRemoved(slug: string): Promise<void> {
  closeEditor()
  try {
    await deleteBuild(slug)
    pushToast('info', 'Сборка удалена')
  } catch (error) {
    reportError(error)
  }
}

async function reveal(build: BuildSummary): Promise<void> {
  try {
    await app.revealPath(build.path)
  } catch (error) {
    reportError(error)
  }
}

/** Pre-download everything a build needs, so Play starts instantly later. */
async function install(build: BuildSummary): Promise<void> {
  installing.value = build.slug
  try {
    const result = await buildsApi.install(build.slug)
    pushToast(
      'success',
      `Готово: ${result.libraries} библиотек, ресурсов ${result.assets}`,
    )
    await refresh()
  } catch (error) {
    reportError(error)
  } finally {
    installing.value = null
  }
}

async function play(build: BuildSummary): Promise<void> {
  await selectBuild(build.slug)
  emit('played')
}
</script>

<template>
  <div class="sf-view">
    <section class="sf-card">
      <header class="sf-card__head">
        <h2 class="sf-card__title"><Boxes :size="16" /> Сборки</h2>
        <div class="sf-card__actions">
          <button class="sf-btn sf-btn--small" type="button" @click="startCreate">
            <Plus :size="14" /> Новая
          </button>
          <button class="sf-btn sf-btn--ghost sf-btn--small" type="button" :disabled="loading" @click="refresh">
            Обновить
          </button>
        </div>
      </header>

      <p class="sf-card__lead sf-muted">
        У каждой сборки своя версия, загрузчик, память и моды. Миры, настройки и
        моды не смешиваются между сборками, а библиотеки и ресурсы Minecraft
        хранятся один раз на всех.
      </p>

      <p v-if="loading" class="sf-muted">Загружаем список…</p>

      <p v-else-if="!store.list.value.length" class="sf-empty">
        Сборок пока нет. Создайте первую — выберите версию Minecraft и загрузчик.
      </p>

      <ul v-else class="sf-builds">
        <li v-for="build in store.list.value" :key="build.slug" class="sf-build">
          <span class="sf-build__icon" aria-hidden="true">
            <img v-if="build.iconUrl" :src="build.iconUrl" alt="" />
            <Boxes v-else :size="17" />
          </span>

          <div class="sf-build__main">
            <span class="sf-build__name">{{ build.name }}</span>
            <span class="sf-muted sf-build__meta">
              {{ describe(build) }}
              <template v-if="build.lastPlayedAt">
                · играли {{ formatShortDate(build.lastPlayedAt) }}
              </template>
            </span>
          </div>

          <span class="sf-chip" :class="STATE_CHIP[build.state]">
            {{ STATE_LABEL[build.state] }}
          </span>

          <div class="sf-build__actions">
            <button
              class="sf-btn sf-btn--icon"
              type="button"
              title="Играть"
              @click="play(build)"
            >
              <Play :size="15" />
            </button>
            <button
              class="sf-btn sf-btn--icon"
              type="button"
              title="Скачать файлы заранее"
              :disabled="installing === build.slug"
              @click="install(build)"
            >
              <Package :size="15" />
            </button>
            <button
              class="sf-btn sf-btn--icon"
              type="button"
              title="Моды"
              @click="emit('open-mods', build.slug)"
            >
              <Puzzle :size="15" />
            </button>
            <button
              class="sf-btn sf-btn--icon"
              type="button"
              title="Открыть папку"
              @click="reveal(build)"
            >
              <FolderOpen :size="15" />
            </button>
            <button
              class="sf-btn sf-btn--icon"
              type="button"
              title="Настройки сборки"
              @click="startEdit(build)"
            >
              <Pencil :size="15" />
            </button>
          </div>
        </li>
      </ul>
    </section>

    <BuildEditor
      v-if="editorOpen"
      :build="creating ? null : editing"
      @saved="onSaved"
      @cancel="closeEditor"
      @removed="onRemoved"
    />

    <section v-if="!editorOpen" class="sf-card">
      <header class="sf-card__head">
        <h2 class="sf-card__title"><Package :size="16" /> Что хранится отдельно</h2>
      </header>
      <p class="sf-card__lead sf-muted">
        Общее для всех сборок: клиентские файлы, библиотеки и ресурсы — они
        идентичны и лежат в одном кэше. Своё у каждой сборки: миры, настройки,
        ресурспаки, шейдеры, моды, natives и логи.
      </p>
      <p class="sf-card__foot sf-muted">
        Моды устанавливаются из Modrinth и ограничены версией и загрузчиком
        сборки, поэтому в список попадают только совместимые файлы.
      </p>
    </section>
  </div>
</template>

<style scoped>
@import './panel.css';

.sf-empty {
  margin: 0;
  padding: 1.2rem 0.6rem;
  text-align: center;
  color: var(--sf-text-muted);
  font-size: 0.82rem;
}

.sf-builds {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 0.3rem;
  max-height: 24rem;
  overflow-y: auto;
}

.sf-build {
  display: flex;
  align-items: center;
  gap: 0.55rem;
  padding: 0.45rem 0.5rem;
  border-radius: var(--sf-radius);
  background: color-mix(in srgb, var(--sf-text) 5%, transparent);
  transition: background-color var(--sf-dur-fast) var(--sf-ease);
}

.sf-build:hover {
  background: color-mix(in srgb, var(--sf-text) 9%, transparent);
}

.sf-build__icon {
  flex: none;
  display: grid;
  place-items: center;
  width: 2rem;
  height: 2rem;
  border-radius: var(--sf-radius-sm);
  overflow: hidden;
  background: color-mix(in srgb, var(--sf-accent) 16%, transparent);
  color: var(--sf-accent);
}

.sf-build__icon img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.sf-build__main {
  display: flex;
  flex-direction: column;
  gap: 0.05rem;
  min-width: 0;
  flex: 1;
}

.sf-build__name {
  font-size: 0.86rem;
  font-weight: var(--sf-weight-bold);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.sf-build__meta {
  font-size: 0.7rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.sf-build__actions {
  display: flex;
  gap: 0.1rem;
  flex: none;
}
</style>
