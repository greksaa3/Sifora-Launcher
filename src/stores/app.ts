import { computed, reactive, readonly, ref, shallowRef, watch } from 'vue'

import { app, backgrounds, onDownloadEvent, onThemeChanged, themes } from '@/lib/ipc'
import { applyTheme, setLightRendering, setReducedMotion } from '@/theme/apply'
import type {
  AggregateProgress,
  BackgroundImage,
  Bootstrap,
  DownloadEvent,
  GameSession,
  GameState,
  Settings,
  ThemeSpec,
  ThemeSummary,
} from '@/types'

/* ------------------------------------------------------------------ toasts */

export interface Toast {
  id: number
  kind: 'info' | 'success' | 'warning' | 'error'
  message: string
  detail?: string
}

/* ------------------------------------------------------------------ stores */

const ready = ref(false)
const fatal = ref<string | null>(null)
const bootstrap = shallowRef<Bootstrap | null>(null)
const activeTheme = shallowRef<ThemeSpec | null>(null)
const themeCatalog = ref<ThemeSummary[]>([])
const customImages = ref<BackgroundImage[]>([])
const session = ref<GameSession | null>(null)
const settings = ref<Settings | null>(null)
const progress = ref<AggregateProgress | null>(null)
const toasts = ref<Toast[]>([])
const busyLabel = ref<string | null>(null)

let toastSeq = 0
let unlistenDownload: (() => void) | null = null
let unlistenTheme: (() => void) | null = null

export const useAppStore = () => ({
  ready: readonly(ready),
  fatal: readonly(fatal),
  bootstrap: readonly(bootstrap),
  activeTheme: readonly(activeTheme),
  themeCatalog: readonly(themeCatalog),
  customImages: readonly(customImages),
  session: readonly(session),
  settings: readonly(settings),
  progress: readonly(progress),
  busyLabel: readonly(busyLabel),
  toasts: readonly(toasts),
  gameState: computed<GameState>(() => session.value?.state ?? 'idle'),
  isBusy: computed(() => busyLabel.value !== null),
})

/* ------------------------------------------------------------------- toasts */

export function pushToast(kind: Toast['kind'], message: string, detail?: string): void {
  const toast: Toast = { id: ++toastSeq, kind, message, detail }
  toasts.value = [...toasts.value, toast]
  const ttl = kind === 'error' ? 9000 : 4200
  window.setTimeout(() => dismissToast(toast.id), ttl)
}

export function dismissToast(id: number): void {
  toasts.value = toasts.value.filter((t) => t.id !== id)
}

export function reportError(error: unknown): void {
  const e = error as { message?: string; kind?: string; detail?: string }
  pushToast('error', e?.message ?? String(error), e?.detail)
}

/* ------------------------------------------------------------------- boot */

export async function initialise(): Promise<void> {
  if (ready.value) return
  try {
    const data = await app.bootstrap()
    bootstrap.value = data
    settings.value = data.settings
    themeCatalog.value = data.themes
    activeTheme.value = data.activeTheme

    applyTheme(data.activeTheme)
    setReducedMotion(data.settings.force_reduce_motion)
    setLightRendering(data.settings.light_rendering)
    applyMotionPreference()

    customImages.value = await backgrounds.list().catch(() => [])

    unlistenDownload = await onDownloadEvent((payload) => handleDownloadEvent(payload as DownloadEvent))
    unlistenTheme = await onThemeChanged((theme) => {
      activeTheme.value = theme
      applyTheme(theme)
    })

    ready.value = true
    void app.showMainWindow()
  } catch (error) {
    // The window is created hidden; it must still be revealed so the failure
    // is visible instead of looking like a silent crash.
    fatal.value = (error as Error)?.message ?? 'Не удалось инициализировать лаунчер'
    void app.showMainWindow().catch(() => undefined)
  }
}

export function disposeApp(): void {
  unlistenDownload?.()
  unlistenTheme?.()
  unlistenDownload = null
  unlistenTheme = null
}

/* ------------------------------------------------------------------ motion */

const systemReducedMotion = ref(false)

export function applyMotionPreference(): void {
  const forced = Boolean(settings.value?.force_reduce_motion)
  const off = forced || systemReducedMotion.value
  document.documentElement.dataset.motion = off ? 'off' : 'on'
  setReducedMotion(off)
}

export function watchSystemMotionPreference(): void {
  if (typeof window.matchMedia !== 'function') return
  const query = window.matchMedia('(prefers-reduced-motion: reduce)')
  systemReducedMotion.value = query.matches
  const listener = (event: MediaQueryListEvent) => {
    systemReducedMotion.value = event.matches
    applyMotionPreference()
  }
  query.addEventListener('change', listener)
}

export const motionState = () => ({
  system: readonly(systemReducedMotion),
  isReduced: computed(
    () => systemReducedMotion.value || Boolean(settings.value?.force_reduce_motion),
  ),
})

/* ----------------------------------------------------------------- updates */

export async function refreshThemeCatalog(): Promise<void> {
  const list = await themes.list().catch(() => null)
  if (list) themeCatalog.value = list.themes
}

export async function persistSettings(next: Settings): Promise<void> {
  settings.value = next
  applyMotionPreference()
  setLightRendering(next.light_rendering)
}

export function setBusy(label: string | null): void {
  busyLabel.value = label
}

/* -------------------------------------------------------------- progress */

function handleDownloadEvent(event: DownloadEvent): void {
  if (event.kind === 'aggregate') {
    progress.value = {
      job: event.job,
      doneFiles: event.doneFiles,
      totalFiles: event.totalFiles,
      bytes: event.bytes,
      totalBytes: event.totalBytes,
      note: progress.value?.job === event.job ? progress.value.note : '',
    }
    return
  }
  if (event.kind === 'note') {
    if (event.job === 'launch') {
      pushToast('info', event.message)
      return
    }
    const current = progress.value
    if (current && current.job === event.job) {
      current.note = event.message
      progress.value = { ...current }
    }
    return
  }
  // Individual file events are only interesting when they fail; the aggregate
  // view already covers the happy path and updating 900 rows would thrash.
  if (event.failed) {
    pushToast('warning', 'Не удалось загрузить файл', event.id)
  }
}

export function resetProgress(): void {
  progress.value = null
}

/* ---------------------------------------------------------------- session */

export function setSession(next: GameSession | null): void {
  session.value = next
}

watch(session, (next, previous) => {
  if (previous && next && previous.state !== next.state) {
    if (next.state === 'exited' || next.state === 'failed') {
      pushToast(next.state === 'exited' ? 'success' : 'error', next.message)
      resetProgress()
    }
  }
})

/* ---------------------------------------------------------------- exports */

export function useThemeControls() {
  return reactive({
    active: activeTheme,
    catalog: themeCatalog,
    images: customImages,
  })
}

export {
  activeTheme,
  bootstrap,
  busyLabel,
  customImages,
  fatal,
  progress,
  ready,
  session,
  settings,
  themeCatalog,
  toasts,
}