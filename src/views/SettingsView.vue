<script setup lang="ts">
/**
 * Settings.
 *
 * Every control writes through `set_settings`, which range-checks the value in
 * Rust before persisting it — the UI clamps as well, but the backend is the
 * authority so a hand-edited file cannot put the launcher into a bad state.
 */
import { computed, onMounted, ref } from 'vue'
import { Cpu, FolderOpen, HardDrive, Info, RefreshCw, RotateCcw, User } from '@lucide/vue'

import { app } from '@/lib/ipc'
import { formatBytes } from '@/lib/format'
import { pushToast, reportError } from '@/stores/app'
import type { JavaRuntime, Settings } from '@/types'

const props = defineProps<{ settings: Settings; dataDir: string }>()

const emit = defineEmits<{
  (e: 'changed', settings: Settings): void
  (e: 'log', records: import('@/types').LogRecord[]): void
}>()

const runtimes = ref<JavaRuntime[]>([])
const saving = ref(false)
const scanning = ref(false)

/**
 * Slider ceiling. `navigator.deviceMemory` reports RAM in GiB and is not in
 * every WebView, so we fall back to a safe 8 GiB; Rust clamps the final value
 * regardless.
 */
const totalRam = computed(() => {
  const nav = navigator as Navigator & { deviceMemory?: number }
  return nav.deviceMemory ? nav.deviceMemory * 1024 : 8192
})

/**
 * The Java scan spawns `java -version` once per candidate runtime and consults
 * the Windows registry. Watching the whole `settings` object ran it again on
 * every single edit — toggling one switch meant dozens of child processes and
 * dozens of console flashes. Nothing edited on this screen changes what the
 * scan returns, so it runs once, and afterwards only when asked.
 */
async function loadRuntimes(): Promise<void> {
  scanning.value = true
  try {
    runtimes.value = await app.listJavaRuntimes().catch(() => [])
  } finally {
    scanning.value = false
  }
}

onMounted(loadRuntimes)

async function patch(changes: Partial<Settings>): Promise<void> {
  saving.value = true
  try {
    emit('changed', await app.setSettings({ ...props.settings, ...changes }))
  } catch (error) {
    reportError(error)
  } finally {
    saving.value = false
  }
}

/** The nickname rules, mirrored from the Rust side so the UI can guide. */
const NAME_PATTERN = /^[A-Za-z0-9_]{3,16}$/
const editingName = ref(false)
const nameDraft = ref('')
const nameTouched = ref(false)

const nameProblem = computed(() => {
  const value = nameDraft.value.trim()
  if (!value) return 'Введите ник'
  if (value.length < 3) return 'Минимум 3 символа'
  if (value.length > 16) return 'Максимум 16 символов'
  if (!NAME_PATTERN.test(value)) return 'Только латиница, цифры и подчёркивание'
  return ''
})

function startRename(): void {
  nameDraft.value = props.settings.player_name
  nameTouched.value = false
  editingName.value = true
}

function cancelRename(): void {
  editingName.value = false
  nameTouched.value = false
}

async function commitRename(): Promise<void> {
  nameTouched.value = true
  if (nameProblem.value) return
  const name = nameDraft.value.trim()
  editingName.value = false
  try {
    emit('changed', await app.pickPlayerName(name))
  } catch (error) {
    reportError(error)
  }
}

function useRuntime(runtime: JavaRuntime): void {
  void patch({ java_path: runtime.path })
}

async function openFolder(path: string): Promise<void> {
  try {
    await app.revealPath(path)
  } catch (error) {
    reportError(error)
  }
}

async function showLog(): Promise<void> {
  try {
    const records = await app.tailLog(120)
    emit(
      'log',
      records.map((record) => ({
        ...record,
        message: `${record.target} · ${record.message}`,
      })),
    )
  } catch (error) {
    reportError(error)
  }
}

/**
 * Reset asks for a second click instead of raising a native dialog.
 *
 * `window.confirm` renders a browser-owned modal that looks nothing like the
 * rest of the app, blocks the event loop, and in a Tauri window it floats
 * outside the themed surface entirely.
 */
const confirmingReset = ref(false)
let resetTimer: ReturnType<typeof setTimeout> | null = null

function armReset(): void {
  confirmingReset.value = true
  if (resetTimer) clearTimeout(resetTimer)
  resetTimer = setTimeout(() => {
    confirmingReset.value = false
  }, 6000)
}

function disarmReset(): void {
  confirmingReset.value = false
  if (resetTimer) clearTimeout(resetTimer)
}

function resetAll(): void {
  disarmReset()
  void patch({
    player_name: 'Player',
    ram_mb: 2048,
    java_path: null,
    auto_provision_java: true,
    concurrent_downloads: 8,
    close_on_launch: false,
    minimise_to_tray_on_close: false,
    force_reduce_motion: false,
  })
  pushToast('info', 'Настройки сброшены')
}
</script>

<template>
  <div class="sf-view">
    <!-- Player -->
    <section class="sf-card">
      <header class="sf-card__head">
        <h2 class="sf-card__title"><User :size="16" /> Игрок</h2>
      </header>
      <div class="sf-row">
        <span class="sf-row__label">
          <span class="sf-row__title">Ник</span>
          <span class="sf-row__hint">
            Автономный режим: учётная запись Mojang не требуется и никуда не передаётся.
          </span>
        </span>
        <span v-if="!editingName" class="sf-row__control">
          <span class="sf-mono sf-muted">{{ settings.player_name }}</span>
          <button class="sf-btn sf-btn--soft" type="button" @click="startRename">Изменить</button>
        </span>

        <span v-else class="sf-row__control sf-row__control--edit">
          <label class="sf-field">
            <input
              v-model="nameDraft"
              class="sf-input sf-input--inline"
              type="text"
              maxlength="16"
              autocomplete="off"
              spellcheck="false"
              autofocus
              :aria-invalid="nameTouched && nameProblem ? 'true' : 'false'"
              @keydown.enter.prevent="commitRename"
              @keydown.esc.prevent="cancelRename"
              @blur="cancelRename"
            />
          </label>
          <button class="sf-btn sf-btn--primary" type="button" @click="commitRename">ОК</button>
          <button class="sf-btn sf-btn--soft" type="button" @click="cancelRename">Отмена</button>
          <span v-if="nameTouched && nameProblem" class="sf-field__error">{{ nameProblem }}</span>
        </span>
      </div>
    </section>

    <!-- Performance -->
    <section class="sf-card">
      <header class="sf-card__head">
        <h2 class="sf-card__title"><Cpu :size="16" /> Производительность</h2>
      </header>

      <div class="sf-row">
        <span class="sf-row__label">
          <span class="sf-row__title">Память игры</span>
          <span class="sf-row__hint">Больше памяти помогает с модами и сборками.</span>
        </span>
        <span class="sf-row__control">
          <input
            type="range"
            min="512"
            :max="Math.min(32768, Math.max(2048, totalRam))"
            step="256"
            :value="settings.ram_mb"
            :disabled="saving"
            @change="patch({ ram_mb: Number(($event.target as HTMLInputElement).value) })"
          />
          <span class="sf-mono sf-row__value">{{ formatBytes(settings.ram_mb * 1024 * 1024) }}</span>
        </span>
      </div>

      <div class="sf-row">
        <span class="sf-row__label">
          <span class="sf-row__title">Параллельных загрузок</span>
          <span class="sf-row__hint">Больше — быстрее, но выше риск ограничений от Mojang.</span>
        </span>
        <span class="sf-row__control">
          <input
            type="range"
            min="1"
            max="16"
            step="1"
            :value="settings.concurrent_downloads"
            :disabled="saving"
            @change="patch({ concurrent_downloads: Number(($event.target as HTMLInputElement).value) })"
          />
          <span class="sf-mono sf-row__value">{{ settings.concurrent_downloads }}</span>
        </span>
      </div>

      <div class="sf-row">
        <span class="sf-row__label">
          <span class="sf-row__title">Скачивать Java автоматически</span>
          <span class="sf-row__hint">
            Если подходящей версии нет, ставим Temurin в папку лаунчера.
          </span>
        </span>
        <span class="sf-row__control">
          <button
            class="sf-switch"
            type="button"
            role="switch"
            :aria-checked="settings.auto_provision_java"
            :disabled="saving"
            @click="patch({ auto_provision_java: !settings.auto_provision_java })"
          />
        </span>
      </div>

      <div class="sf-row">
        <span class="sf-row__label">
          <span class="sf-row__title">Своя папка с данными</span>
          <span class="sf-row__hint sf-mono">{{ dataDir }}</span>
        </span>
        <span class="sf-row__control">
          <button class="sf-btn sf-btn--soft" type="button" @click="openFolder(dataDir)">
            <FolderOpen :size="14" />
            Открыть
          </button>
        </span>
      </div>
    </section>

    <!-- Java -->
    <section class="sf-card">
      <header class="sf-card__head">
        <h2 class="sf-card__title"><HardDrive :size="16" /> Java</h2>
        <span class="sf-card__actions">
          <button
            class="sf-btn sf-btn--ghost"
            type="button"
            :disabled="scanning"
            @click="loadRuntimes"
          >
            <RefreshCw :size="14" :class="{ 'sf-spin': scanning }" />
            {{ scanning ? 'Поиск…' : 'Пересканировать' }}
          </button>
          <button class="sf-btn sf-btn--ghost" type="button" @click="showLog">Журнал</button>
        </span>
      </header>

      <p class="sf-card__lead sf-muted">
        Лаунчер сам подбирает среду с подходящей версией. Здесь можно зафиксировать конкретную.
      </p>

      <div class="sf-row">
        <span class="sf-row__label">
          <span class="sf-row__title">Используется</span>
          <span class="sf-row__hint sf-mono">
            {{ settings.java_path || 'автоматический подбор' }}
          </span>
        </span>
        <span class="sf-row__control">
          <button
            v-if="settings.java_path"
            class="sf-btn sf-btn--soft"
            type="button"
            @click="patch({ java_path: null })"
          >
            Авто
          </button>
        </span>
      </div>

      <ul v-if="runtimes.length" class="sf-java-list">
        <li v-for="runtime in runtimes" :key="runtime.path" class="sf-java">
          <span class="sf-java__meta">
            <span class="sf-java__version">{{ runtime.vendor }} {{ runtime.version }}</span>
            <span class="sf-java__path sf-mono">{{ runtime.path }}</span>
          </span>
          <button class="sf-btn sf-btn--soft" type="button" @click="useRuntime(runtime)">
            Выбрать
          </button>
        </li>
      </ul>
    </section>

    <!-- Behaviour -->
    <section class="sf-card">
      <header class="sf-card__head">
        <h2 class="sf-card__title"><Info :size="16" /> Поведение</h2>
      </header>

      <div class="sf-row">
        <span class="sf-row__label">
          <span class="sf-row__title">Меньше движения</span>
          <span class="sf-row__hint">Отключает анимации независимо от темы и системной настройки.</span>
        </span>
        <span class="sf-row__control">
          <button
            class="sf-switch"
            type="button"
            role="switch"
            :aria-checked="settings.force_reduce_motion"
            :disabled="saving"
            @click="patch({ force_reduce_motion: !settings.force_reduce_motion })"
          />
        </span>
      </div>

      <div class="sf-row">
        <span class="sf-row__label">
          <span class="sf-row__title">Облегчённая отрисовка</span>
          <span class="sf-row__hint">
            Убирает размытие стекла за панелями. Оставьте включённым, если лаунчер
            нагружает компьютер: без видеокарты этот эффект считается процессором.
          </span>
        </span>
        <span class="sf-row__control">
          <button
            class="sf-switch"
            type="button"
            role="switch"
            :aria-checked="settings.light_rendering"
            :disabled="saving"
            @click="patch({ light_rendering: !settings.light_rendering })"
          />
        </span>
      </div>

      <div class="sf-row">
        <span class="sf-row__label">
          <span class="sf-row__title">Сворачивать в фоне</span>
          <span class="sf-row__hint">Закрытие окна прячет лаунчер, пока идёт игра.</span>
        </span>
        <span class="sf-row__control">
          <button
            class="sf-switch"
            type="button"
            role="switch"
            :aria-checked="settings.minimise_to_tray_on_close"
            :disabled="saving"
            @click="patch({ minimise_to_tray_on_close: !settings.minimise_to_tray_on_close })"
          />
        </span>
      </div>

      <div class="sf-row">
        <span class="sf-row__control">
          <template v-if="!confirmingReset">
            <button class="sf-btn sf-btn--outline" type="button" @click="armReset">
              <RotateCcw :size="14" />
              Сбросить настройки
            </button>
          </template>
          <template v-else>
            <span class="sf-row__hint">Сбросить всё к значениям по умолчанию?</span>
            <button class="sf-btn sf-btn--danger" type="button" @click="resetAll">Да, сбросить</button>
            <button class="sf-btn sf-btn--soft" type="button" @click="disarmReset">Отмена</button>
          </template>
        </span>
      </div>
    </section>
  </div>
</template>

<style scoped>
@import './panel.css';

.sf-row__control input[type='range'] {
  width: 9rem;
}

.sf-row__value {
  min-width: 4.4rem;
  text-align: right;
  font-size: 0.78rem;
}

.sf-java-list {
  list-style: none;
  margin: 0.5rem 0 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
  max-height: 13rem;
  overflow-y: auto;
}

.sf-java {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 0.6rem;
  padding: 0.35rem 0.5rem;
  border-radius: var(--sf-radius-sm);
  background: color-mix(in srgb, var(--sf-text) 5%, transparent);
}

.sf-java__meta {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.sf-java__version {
  font-size: 0.78rem;
  font-weight: var(--sf-weight-bold);
}

.sf-java__path {
  font-size: 0.66rem;
  color: var(--sf-text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>