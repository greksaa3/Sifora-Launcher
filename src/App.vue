<script setup lang="ts">
/**
 * Sifora Launcher — shell.
 *
 * Stage 2 layout: the launch screen stays the centre of gravity, and the
 * menu opens views around it rather than replacing it. Every privileged action
 * is a Rust command; this file decides only *when* to call one and what to show
 * while it runs.
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { ChevronLeft, Palette, User } from '@lucide/vue'

import AppMenu, { type ViewId } from '@/components/AppMenu.vue'
import BackdropLayer from '@/components/BackdropLayer.vue'
import BackgroundPicker from '@/components/BackgroundPicker.vue'
import PlayButton from '@/components/PlayButton.vue'
import ProgressPanel from '@/components/ProgressPanel.vue'
import ShortcutBar from '@/components/ShortcutBar.vue'
import ToastHost from '@/components/ToastHost.vue'
import VersionSelector from '@/components/VersionSelector.vue'
import AboutView from '@/views/AboutView.vue'
import AppearanceView from '@/views/AppearanceView.vue'
import InstancesView from '@/views/InstancesView.vue'
import SettingsView from '@/views/SettingsView.vue'

import { app, game, themes as themeApi, themes } from '@/lib/ipc'
import {
  activeTheme,
  bootstrap,
  busyLabel,
  customImages,
  disposeApp,
  fatal,
  initialise,
  progress,
  pushToast,
  ready,
  reportError,
  resetProgress,
  session,
  settings,
  themeCatalog,
  toasts,
  watchSystemMotionPreference,
} from '@/stores/app'
import {
  error,
  loadVersions,
  query,
  rows,
  sections,
  selectVersion,
  selected,
  selectedId,
  setTypeFilter,
  typeFilter,
  loading,
} from '@/stores/versions'
import { applyTheme } from '@/theme/apply'
import type { BackgroundImage, ReleaseType, Shortcut, ThemeSummary, VersionRow } from '@/types'

/* ------------------------------------------------------------------ state */

const view = ref<ViewId>('home')
const menuOpen = ref(false)
const pickerOpen = ref(false)
const launchError = ref<string | null>(null)
const instances = ref<number>(0)
const backgroundImage = ref<BackgroundImage | null>(null)
const activeImageId = ref<string | null>(null)

/* --------------------------------------------------------------- computed */

const playerName = computed(() => settings.value?.player_name ?? 'Player')
const isRunning = computed(() => session.value?.state === 'running')
const isBusy = computed(() => busyLabel.value !== null)
const canPlay = computed(() => Boolean(selectedId.value) && !isBusy.value && !isRunning.value)
const builtinThemes = computed(() => themeCatalog.value.filter((t: ThemeSummary) => t.builtin))
const customThemeCount = computed(() => themeCatalog.value.filter((t: ThemeSummary) => !t.builtin).length)

/* ------------------------------------------------------------- lifecycle */

/**
 * Freeze every ambient animation while the window is hidden or unfocused.
 *
 * A launcher spends nearly all its life behind other windows. A drifting
 * backdrop and the glass panels stacked on top of it otherwise keep the
 * compositor working at full frame rate for nobody to see.
 */
function watchWindowIdle(): () => void {
  const root = document.documentElement
  const update = () => {
    root.dataset.idle = document.hidden || !document.hasFocus() ? 'true' : 'false'
  }
  document.addEventListener('visibilitychange', update)
  window.addEventListener('blur', update)
  window.addEventListener('focus', update)
  update()
  return () => {
    document.removeEventListener('visibilitychange', update)
    window.removeEventListener('blur', update)
    window.removeEventListener('focus', update)
  }
}

onMounted(() => watchWindowIdle())

onMounted(async () => {
  watchSystemMotionPreference()
  await initialise()
  if (!ready.value) return

  await loadVersions()
  const saved = settings.value?.selected_version ?? null
  const fallback =
    rows.value.find((r) => r.is_latest_release)?.id ??
    rows.value.find((r) => r.installed)?.id ??
    null
  const chosen = saved && rows.value.some((r) => r.id === saved) ? saved : fallback
  selectVersion(chosen)
  if (chosen !== settings.value?.selected_version) void persistSelection(chosen)

  const images = customImages.value
  if (images.length) {
    backgroundImage.value = images[0]
    activeImageId.value = images[0].id
  }
  instances.value = (await game.listInstances().catch(() => [])).length
})

onBeforeUnmount(disposeApp)

/* ---------------------------------------------------------------- actions */

async function persistSelection(versionId: string | null): Promise<void> {
  if (!settings.value) return
  try {
    settings.value = await app.setSettings({ ...settings.value, selected_version: versionId })
  } catch (e) {
    reportError(e)
  }
}

function onSelect(row: VersionRow): void {
  selectVersion(row.id)
  void persistSelection(row.id)
  if (!row.supported) pushToast('warning', `${row.id}: ${row.unsupported_reason}`)
}

async function onPlay(): Promise<void> {
  const versionId = selectedId.value
  if (!versionId) {
    pushToast('warning', 'Сначала выберите версию Minecraft')
    return
  }

  launchError.value = null
  resetProgress()
  busyLabel.value = 'Проверяем файлы версии…'

  try {
    const list = await game.listInstances().catch(() => [])
    const instance = list.find((i) => i.versionId === versionId)
    instances.value = list.length

    if (!instance || instance.state !== 'ready') {
      busyLabel.value = `Загружаем Minecraft ${versionId}…`
      const installed = await game.install({ versionId })
      pushToast(
        'success',
        `Minecraft ${versionId} готов`,
        `${installed.libraries} библиотек · ${installed.assets} ресурсов · Java ${installed.javaVersion}`,
      )
      instances.value = (await game.listInstances().catch(() => list)).length
    }

    busyLabel.value = 'Запускаем…'
    const started = await game.launch({
      versionId,
      instanceSlug: instance?.slug,
      ramMb: settings.value?.ram_mb,
    })
    session.value = started
    pushToast('success', 'Minecraft запущен', started.versionId)
  } catch (e) {
    const err = e as { message?: string; detail?: string }
    reportError(e)
    launchError.value = err?.message ?? 'Не удалось запустить игру'
    pushToast('error', 'Запуск не удался', err?.detail)
  } finally {
    busyLabel.value = null
  }
}

async function onPreset(id: string): Promise<void> {
  try {
    const theme = await themeApi.applyById(id)
    applyTheme(theme)
    backgroundImage.value = null
    activeImageId.value = null
    if (settings.value) settings.value = { ...settings.value, last_background: null }
  } catch (e) {
    reportError(e)
  }
}

async function onImage(image: BackgroundImage | null): Promise<void> {
  backgroundImage.value = image
  activeImageId.value = image?.id ?? null
  pickerOpen.value = false
  if (!settings.value) return
  try {
    settings.value = await app.setSettings({ ...settings.value, last_background: image?.id ?? null })
  } catch (e) {
    reportError(e)
  }
}

function onThemeApplied(theme: Parameters<typeof applyTheme>[0]): void {
  applyTheme(theme)
}

function onFilter(value: ReleaseType | 'all'): void {
  setTypeFilter(value)
}

function dismissProgress(): void {
  resetProgress()
  busyLabel.value = null
  launchError.value = null
}

const gameDataDir = computed(() => `${bootstrap.value?.dataDir ?? ''}/game`)

/* --------------------------------------------------------------- shortcuts */

/**
 * Capabilities the active theme promoted onto the launch screen.
 *
 * Every entry is an action this launcher already performs; the theme decides
 * which ones are worth surfacing, not what they do.
 */
const shortcuts = computed<Shortcut[]>(() => activeTheme.value?.tokens.layout.shortcuts ?? [])

async function runShortcut(shortcut: Shortcut): Promise<void> {
  switch (shortcut) {
    case 'appearance':
      view.value = 'themes'
      break
    case 'settings':
      view.value = 'settings'
      break
    case 'instances':
      view.value = 'instances'
      break
    case 'log':
      view.value = 'about'
      break
    case 'toggle-background':
      pickerOpen.value = !pickerOpen.value
      break
    case 'random-theme': {
      const pool = themeCatalog.value.filter((t) => t.id !== activeTheme.value?.meta.id)
      if (!pool.length) return
      const pick = pool[Math.floor(Math.random() * pool.length)]
      try {
        onThemeApplied(await themeApi.get(pick.id))
      } catch (e) {
        reportError(e)
      }
      break
    }
    case 'performance':
      try {
        settings.value = await themes.setForceReduceMotion(
          !settings.value?.force_reduce_motion,
        )
      } catch (e) {
        reportError(e)
      }
      break
  }
}
</script>

<template>
  <div class="sf-app" :data-placement="activeTheme?.tokens?.layout?.placement ?? 'centered'">
    <BackdropLayer
      :theme="activeTheme"
      :image-path="backgroundImage?.path ?? null"
      :motion="activeTheme?.tokens.motion.ambient ? 'on' : 'off'"
    />

    <header class="sf-app__top sf-no-drag">
      <div class="sf-app__top-left">
        <button
          class="sf-btn sf-btn--icon sf-app__glass"
          type="button"
          :aria-label="menuOpen ? 'Закрыть меню' : 'Открыть меню'"
          @click="menuOpen = !menuOpen"
        >
          <ChevronLeft v-if="menuOpen" :size="18" class="sf-app__flip" />
          <span v-else class="sf-app__bars" aria-hidden="true"><i /><i /><i /></span>
        </button>

        <button
          v-if="view === 'home'"
          class="sf-btn sf-btn--icon sf-app__glass"
          type="button"
          aria-label="Выбрать фон"
          title="Фон"
          @click="pickerOpen = !pickerOpen"
        >
          <Palette :size="17" />
        </button>

        <BackgroundPicker
          :open="pickerOpen"
          :builtins="builtinThemes"
          :images="customImages"
          :active-preset-id="settings?.theme_id ?? null"
          :active-image-id="activeImageId"
          @update:open="pickerOpen = $event"
          @choose-preset="onPreset"
          @choose-image="onImage"
        />
      </div>

      <span class="sf-status sf-app__player" :title="`Игрок: ${playerName}`">
        <User :size="13" />
        <span>{{ playerName }}</span>
      </span>
    </header>

    <!-- Boot / fatal -->
    <main v-if="fatal" class="sf-app__center">
      <div class="sf-panel sf-fatal">
        <h1 class="sf-title">Не удалось запустить лаунчер</h1>
        <p class="sf-muted">{{ fatal }}</p>
      </div>
    </main>

    <main v-else-if="!ready" class="sf-app__center">
      <div class="sf-boot">
        <span class="sf-boot__mark" />
        <p class="sf-muted">Подготовка Sifora…</p>
      </div>
    </main>

    <!-- Views -->
    <main v-else-if="view !== 'home'" class="sf-app__pane">
      <div class="sf-app__pane-inner">
        <SettingsView
          v-if="view === 'settings'"
          :settings="settings!"
          :data-dir="bootstrap?.dataDir ?? ''"
          @changed="settings = $event"
        />
        <AppearanceView
          v-else-if="view === 'themes'"
          :catalog="themeCatalog"
          :active-id="activeTheme?.meta.id ?? settings?.theme_id ?? ''"
          :can-undo="false"
          :images="customImages"
          :preset-id="settings?.theme_id ?? null"
          :image-id="activeImageId"
          @applied="onThemeApplied"
          @preset-changed="onPreset"
          @image-changed="onImage"
          @images-changed="customImages = $event"
        />
        <InstancesView v-else-if="view === 'instances'" />
        <AboutView
          v-else
          :version="bootstrap?.version ?? ''"
          :data-dir="bootstrap?.dataDir ?? ''"
          :game-data="gameDataDir"
        />
      </div>
    </main>

    <!-- Launch screen -->
    <main v-else class="sf-app__center">
      <div class="sf-app__stack">
        <PlayButton
          :disabled="!canPlay"
          :busy="isBusy"
          :running="isRunning"
          @activate="onPlay"
        />
        <VersionSelector
          :rows="rows"
          :sections="sections"
          :selected="selected"
          :query="query"
          :type-filter="typeFilter"
          :loading="loading"
          :error="error"
          @update:query="query = $event"
          @update:type-filter="onFilter"
          @select="onSelect"
          @retry="loadVersions(true)"
        />

        <ShortcutBar :shortcuts="shortcuts" @run="runShortcut" />
      </div>
    </main>

    <p v-if="launchError" class="sf-app__error" role="alert">{{ launchError }}</p>

    <ProgressPanel
      :progress="progress"
      :label="busyLabel"
      :error="launchError"
      @dismiss="dismissProgress"
    />

    <AppMenu
      :open="menuOpen"
      :view="view"
      :instance-count="instances"
      :custom-theme-count="customThemeCount"
      :layout="activeTheme?.tokens.layout ?? null"
      :version="bootstrap?.version ?? '0.1.0'"
      @update:open="menuOpen = $event"
      @select="view = $event"
    />

    <ToastHost :toasts="toasts" />
  </div>
</template>

<style scoped>
.sf-app {
  position: relative;
  display: flex;
  flex-direction: column;
  height: 100%;
  isolation: isolate;
}

.sf-app__top {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  padding: 1rem 1.1rem;
  z-index: 30;
}

.sf-app__top-left {
  position: relative;
  display: flex;
  align-items: center;
  gap: 0.4rem;
}

.sf-app__glass {
  background: var(--sf-surface-a);
  border: var(--sf-border-width) solid var(--sf-border);
  backdrop-filter: blur(14px);
  box-shadow: var(--sf-shadow-sm);
}

.sf-app__bars {
  display: flex;
  flex-direction: column;
  gap: 4px;
  width: 16px;
}

.sf-app__bars i {
  display: block;
  height: 1.6px;
  border-radius: 2px;
  background: currentColor;
  transition: transform var(--sf-dur-fast) var(--sf-ease);
}

.sf-app__flip {
  transform: rotate(180deg);
}

.sf-app__player {
  gap: 0.4rem;
  padding: 0.3rem 0.75rem;
  background: var(--sf-surface-a);
  border: var(--sf-border-width) solid var(--sf-border);
  backdrop-filter: blur(14px);
  box-shadow: var(--sf-shadow-sm);
}

.sf-app__center {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0 1.5rem 2rem;
  z-index: 10;
}

.sf-app__pane {
  flex: 1;
  overflow-y: auto;
  overscroll-behavior: contain;
  padding: 0.2rem 1.4rem 0;
  z-index: 10;
}

.sf-app__pane-inner {
  width: min(48rem, 100%);
  margin: 0 auto;
}

.sf-app__stack {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 1.25rem;
  transform: translateY(-4vh);
}

.sf-app__error {
  position: fixed;
  left: 50%;
  bottom: 1.1rem;
  transform: translateX(-50%);
  max-width: min(40rem, 90vw);
  margin: 0;
  padding: 0.5rem 0.9rem;
  border-radius: var(--sf-radius-pill);
  background: var(--sf-surface-a);
  border: 1px solid color-mix(in srgb, var(--sf-danger) 45%, transparent);
  backdrop-filter: blur(14px);
  color: var(--sf-danger);
  font-size: 0.8rem;
  text-align: center;
  z-index: 50;
}

.sf-boot {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 1rem;
}

.sf-boot__mark {
  width: 42px;
  height: 42px;
  border-radius: 14px;
  background: linear-gradient(
    140deg,
    var(--sf-accent),
    color-mix(in srgb, var(--sf-accent) 40%, var(--sf-text))
  );
  animation: sf-boot 1.6s var(--sf-ease) infinite;
}

@keyframes sf-boot {
  0%,
  100% {
    transform: rotate(0deg) scale(1);
    border-radius: 14px;
  }
  50% {
    transform: rotate(8deg) scale(0.94);
    border-radius: 22px;
  }
}

.sf-fatal {
  max-width: 30rem;
  padding: 1.4rem 1.6rem;
  display: flex;
  flex-direction: column;
  gap: 0.6rem;
}
</style>