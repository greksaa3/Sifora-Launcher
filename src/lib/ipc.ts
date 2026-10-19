/**
 * Typed wrapper around Tauri's `invoke`.
 *
 * Every call goes through here so the command surface is visible in one place
 * and so errors arrive as the typed `AppError` object Rust returns rather than
 * as anonymous strings.
 */
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { convertFileSrc } from '@tauri-apps/api/core'

import type {
  AiConfigView,
  BackgroundImage,
  Bootstrap,
  GameSession,
  GameState,
  GenerationResult,
  InstanceSummary,
  InstallResponse,
  JavaRuntime,
  LogRecord,
  ProviderKind,
  Settings,
  ThemeList,
  ThemeSpec,
  ThemeSummary,
  ValidationReport,
  VersionDetails,
  VersionGroups,
  VersionListResponse,
} from '@/types'

/** Shape of the error object produced by `AppError` in Rust. */
export interface CommandError {
  kind: string
  message: string
  detail?: string
  retryable?: boolean
}

export class LaunchError extends Error {
  readonly kind: string
  readonly detail?: string
  readonly retryable: boolean

  constructor(raw: CommandError) {
    super(raw.message)
    this.name = 'LaunchError'
    this.kind = raw.kind
    this.detail = raw.detail
    this.retryable = Boolean(raw.retryable)
  }
}

function isCommandError(value: unknown): value is CommandError {
  return (
    typeof value === 'object' &&
    value !== null &&
    typeof (value as CommandError).kind === 'string' &&
    typeof (value as CommandError).message === 'string'
  )
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args)
  } catch (error) {
    if (isCommandError(error)) throw new LaunchError(error)
    throw new LaunchError({ kind: 'internal', message: String(error) })
  }
}

/* ------------------------------------------------------------------ app */

export const app = {
  bootstrap: () => call<Bootstrap>('bootstrap'),
  getSettings: () => call<Settings>('get_settings'),
  setSettings: (settings: Settings) => call<Settings>('set_settings', { settings }),
  pickPlayerName: (name: string) => call<Settings>('pick_player_name', { name }),
  listJavaRuntimes: () => call<JavaRuntime[]>('list_java_runtimes'),
  tailLog: (limit = 200) => call<LogRecord[]>('tail_log', { limit }),
  rendererLog: (level: string, message: string) =>
    call<void>('renderer_log', { level, message }),
  revealPath: (path: string) => call<void>('reveal_path', { path }),
  showMainWindow: () => call<void>('show_main_window'),
  quit: () => call<void>('quit'),
}

/* -------------------------------------------------------------- versions */

export const versions = {
  load: (forceRefresh = false) =>
    call<VersionListResponse>('load_versions', { forceRefresh }),
  cached: () => call<string[]>('cached_version_ids'),
  groups: () => call<VersionGroups>('summarise_versions'),
  details: (versionId: string) => call<VersionDetails>('version_details', { versionId }),
}

/* ------------------------------------------------------- install / launch */

export const game = {
  install: (request: {
    versionId: string
    ramMb?: number
    width?: number
    height?: number
    name?: string
  }) => call<InstallResponse>('install_version', { request }),
  launch: (request: {
    versionId: string
    instanceSlug?: string
    ramMb?: number
    width?: number
    height?: number
  }) => call<GameSession>('launch_game', { request }),
  status: () => call<GameState>('game_status'),
  session: () => call<GameSession | null>('game_session'),
  listInstances: () => call<InstanceSummary[]>('list_instances'),
  updateInstance: (
    slug: string,
    patch: { ramMb?: number; width?: number; height?: number; name?: string },
  ) =>
    call<InstanceSummary>('update_instance_settings', {
      slug,
      ramMb: patch.ramMb ?? null,
      width: patch.width ?? null,
      height: patch.height ?? null,
      name: patch.name ?? null,
    }),
  deleteInstance: (slug: string) => call<void>('delete_instance', { slug }),
}

/* ---------------------------------------------------------------- themes */

export const themes = {
  list: () => call<ThemeList>('list_themes'),
  get: (themeId?: string) => call<ThemeSpec>('get_theme', { themeId: themeId ?? null }),
  builtins: () => call<ThemeSummary[]>('get_builtin_backgrounds'),
  preview: (theme: ThemeSpec) =>
    call<{ theme: ThemeSpec; report: ValidationReport }>('preview_theme', { theme }),
  clearPreview: () => call<void>('clear_preview'),
  currentPreview: () => call<ThemeSpec | null>('current_preview'),
  apply: (theme: ThemeSpec) => call<ThemeSummary>('apply_theme', { theme }),
  applyById: (themeId: string) => call<ThemeSpec>('apply_theme_by_id', { themeId }),
  undo: () => call<ThemeSpec>('undo_theme'),
  reset: () => call<ThemeSpec>('reset_theme'),
  remove: (themeId: string) => call<void>('delete_theme', { themeId }),
  generate: (description: string) => call<GenerationResult>('generate_theme', { description }),
  aiConfig: () => call<AiConfigView>('get_ai_config'),
  setAiConfig: (config: {
    provider: ProviderKind
    baseUrl: string
    model: string
    apiKey?: string
    timeoutSecs?: number
    temperature?: number
  }) =>
    call<AiConfigView>('set_ai_config', {
      config: {
        provider: config.provider,
        baseUrl: config.baseUrl ?? '',
        model: config.model ?? '',
        apiKey: config.apiKey ?? '',
        timeoutSecs: config.timeoutSecs ?? 60,
        temperature: config.temperature ?? 0.7,
      },
    }),
  validate: (theme: ThemeSpec) => call<ValidationReport>('validate_theme_command', { theme }),
  contrast: (foreground: string, background: string) =>
    call<number | null>('theme_contrast', { foreground, background }),
  defaultSpec: () => call<ThemeSpec>('default_theme_spec'),
  setForceReduceMotion: (value: boolean) =>
    call<Settings>('set_force_reduce_motion', { value }),
  setLightRendering: (value: boolean) =>
    call<Settings>('set_light_rendering', { value }),
}

/* ----------------------------------------------------------- backgrounds */

export const backgrounds = {
  pick: () => call<BackgroundImage | null>('pick_background_image'),
  list: () => call<BackgroundImage[]>('list_backgrounds'),
  remove: (id: string) => call<void>('delete_background', { id }),
}

/* ---------------------------------------------------------------- events */

export function onDownloadEvent(handler: (event: unknown) => void): Promise<UnlistenFn> {
  return listen<unknown>('download://progress', (e) => handler(e.payload))
}

export function onThemeChanged(handler: (theme: ThemeSpec) => void): Promise<UnlistenFn> {
  return listen<ThemeSpec>('theme://changed', (e) => handler(e.payload))
}

/** Convert a path inside the app data folder into a renderer-loadable URL. */
export function toAssetUrl(path: string): string {
  return convertFileSrc(path)
}

/** True when running inside Tauri rather than a plain browser preview. */
export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}