<script setup lang="ts">
/**
 * Instances.
 *
 * Every Minecraft version the user has downloaded gets its own directory with
 * its own worlds, options, resource packs and — from stage 3 — its own mods and
 * loader. Nothing mutable is shared, so two versions can never corrupt each
 * other.
 */
import { onMounted, ref } from 'vue'
import { Boxes, FolderOpen, HardDrive, Trash2 } from '@lucide/vue'

import { app, game } from '@/lib/ipc'
import { formatBytes, formatShortDate } from '@/lib/format'
import { pushToast, reportError } from '@/stores/app'
import type { InstanceSummary } from '@/types'

const instances = ref<InstanceSummary[]>([])
const loading = ref(true)

const STATE_LABEL: Record<InstanceSummary['state'], string> = {
  empty: 'не установлена',
  partial: 'загрузка не завершена',
  ready: 'готова',
}

const STATE_CHIP: Record<InstanceSummary['state'], string> = {
  empty: '',
  partial: 'sf-chip--warn',
  ready: 'sf-chip--ok',
}

async function refresh(): Promise<void> {
  loading.value = true
  try {
    instances.value = await game.listInstances()
  } catch (error) {
    reportError(error)
  } finally {
    loading.value = false
  }
}

onMounted(refresh)

async function reveal(instance: InstanceSummary): Promise<void> {
  try {
    await app.revealPath(instance.path)
  } catch (error) {
    reportError(error)
  }
}

/**
 * Deleting an instance is destructive, so it asks twice in place rather than
 * raising `window.confirm` — a browser-owned modal looks out of place in a
 * themed desktop window and blocks the whole UI while it is up.
 */
const pendingRemoval = ref<string | null>(null)
let removalTimer: ReturnType<typeof setTimeout> | null = null

function armRemoval(slug: string): void {
  pendingRemoval.value = slug
  if (removalTimer) clearTimeout(removalTimer)
  removalTimer = setTimeout(() => {
    pendingRemoval.value = null
  }, 6000)
}

function disarmRemoval(): void {
  pendingRemoval.value = null
  if (removalTimer) clearTimeout(removalTimer)
}

async function remove(instance: InstanceSummary): Promise<void> {
  disarmRemoval()
  try {
    await game.deleteInstance(instance.slug)
    pushToast('info', `Сборка ${instance.versionId} удалена`)
    await refresh()
  } catch (error) {
    reportError(error)
  }
}

defineExpose({ refresh })
</script>

<template>
  <div class="sf-view">
    <section class="sf-card">
      <header class="sf-card__head">
        <h2 class="sf-card__title"><Boxes :size="16" /> Сборки</h2>
        <button class="sf-btn sf-btn--ghost" type="button" :disabled="loading" @click="refresh">
          Обновить
        </button>
      </header>

      <p class="sf-card__lead sf-muted">
        Каждая версия Minecraft устанавливается в отдельную папку со своими мирами,
        настройками и ресурсами. Общие кэши библиотек и ресурсов не дублируются.
      </p>

      <p v-if="loading" class="sf-muted">Загружаем список…</p>

      <p v-else-if="!instances.length" class="sf-empty">
        Пока нет установленных версий. Выберите версию на главном экране и нажмите «Играть».
      </p>

      <ul v-else class="sf-instances">
        <li v-for="instance in instances" :key="instance.id" class="sf-instance">
          <div class="sf-instance__main">
            <span class="sf-instance__id sf-mono">{{ instance.versionId }}</span>
            <span class="sf-chip" :class="STATE_CHIP[instance.state]">
              {{ STATE_LABEL[instance.state] }}
            </span>
            <span class="sf-muted sf-instance__meta">
              {{ formatBytes(instance.sizeBytes) }}
              <template v-if="instance.ramMb"> · {{ instance.ramMb }} МБ</template>
              · создан�� {{ formatShortDate(instance.createdAt) }}
            </span>
          </div>
          <div class="sf-instance__actions">
            <button class="sf-btn sf-btn--icon" type="button" title="Открыть папку" @click="reveal(instance)">
              <FolderOpen :size="15" />
            </button>
            <template v-if="pendingRemoval === instance.slug">
              <span class="sf-instance__confirm">
                Удалить сборку и её миры?
              </span>
              <button class="sf-btn sf-btn--danger" type="button" @click="remove(instance)">
                Да
              </button>
              <button class="sf-btn sf-btn--soft" type="button" @click="disarmRemoval">
                Нет
              </button>
            </template>
            <button
              v-else
              class="sf-btn sf-btn--icon"
              type="button"
              title="Удалить сборку"
              :disabled="instance.state === 'empty'"
              @click="armRemoval(instance.slug)"
            >
              <Trash2 :size="15" />
            </button>
          </div>
        </li>
      </ul>
    </section>

    <section class="sf-card">
      <header class="sf-card__head">
        <h2 class="sf-card__title"><HardDrive :size="16" /> Как устроены сборки</h2>
      </header>
      <p class="sf-card__lead sf-muted">
        Общие для всех версий: клиентские файлы, библиотеки и ресурсы — они
        идентичны и хранятся один раз. Свои для каждой сборки: worlds, options,
        resourcepacks, shaderpacks, natives и logs.
      </p>
      <p class="sf-card__foot sf-muted">
        Загрузчики модов (Fabric, Forge, NeoForge, Quilt, OptiFine) и управление
        модами входят в следующий этап — интерфейс для них уже подготовлен в
        данных сборки, но пока не используется лаунчером.
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

.sf-instances {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 0.3rem;
  max-height: 22rem;
  overflow-y: auto;
}

.sf-instance {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.6rem;
  padding: 0.45rem 0.5rem 0.45rem 0.7rem;
  border-radius: var(--sf-radius);
  background: color-mix(in srgb, var(--sf-text) 5%, transparent);
  transition: background-color var(--sf-dur-fast) var(--sf-ease);
}

.sf-instance:hover {
  background: color-mix(in srgb, var(--sf-text) 9%, transparent);
}

.sf-instance__main {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  min-width: 0;
  flex-wrap: wrap;
}

.sf-instance__id {
  font-size: 0.86rem;
  font-weight: var(--sf-weight-bold);
}

.sf-instance__meta {
  font-size: 0.7rem;
}

.sf-instance__actions {
  display: flex;
  gap: 0.1rem;
  flex: none;
}
</style>