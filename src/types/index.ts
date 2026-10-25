/**
 * Shared types.
 *
 * These mirror the Rust types exactly. `ThemeSpec` in particular is a closed
 * schema: every field is a colour, a number or an enum, so a theme can never
 * carry code into the renderer. `theme/validate.ts` re-checks the shape at
 * runtime before anything is applied — the Rust side already guarantees it, but
 * the renderer should not have to trust the other side of the bridge.
 */

/** `#rrggbb` or `#rrggbbaa`. Produced only by `crate::theme::Color`. */
export type HexColor = string

export type ReleaseType = 'release' | 'snapshot' | 'old_beta' | 'old_alpha' | 'other'

export type FontFamily =
  | 'system'
  | 'segoe'
  | 'georgia'
  | 'palatino'
  | 'trebuchet'
  | 'verdana'
  | 'tahoma'
  | 'impact'
  | 'consolas'
  | 'cascadia'
  | 'courier'
  | 'times'

export const FONT_FAMILIES: FontFamily[] = [
  'system',
  'segoe',
  'georgia',
  'palatino',
  'trebuchet',
  'verdana',
  'tahoma',
  'impact',
  'consolas',
  'cascadia',
  'courier',
  'times',
]

/** Human labels for the font stacks, so the editor does not show raw ids. */
export const FONT_LABELS: Record<FontFamily, string> = {
  system: 'Системный',
  segoe: 'Segoe UI',
  georgia: 'Georgia',
  palatino: 'Palatino',
  trebuchet: 'Trebuchet MS',
  verdana: 'Verdana',
  tahoma: 'Tahoma',
  impact: 'Impact',
  consolas: 'Consolas',
  cascadia: 'Cascadia Code',
  courier: 'Courier New',
  times: 'Times New Roman',
}

export type Easing =
  | 'standard'
  | 'linear'
  | 'decelerate'
  | 'accelerate'
  | 'emphasized'
  | 'spring'

export const EASINGS: Easing[] = [
  'standard',
  'linear',
  'decelerate',
  'accelerate',
  'emphasized',
  'spring',
]

export const EASING_LABELS: Record<Easing, string> = {
  standard: 'Обычная',
  linear: 'Линейная',
  decelerate: 'Замедление',
  accelerate: 'Ускорение',
  emphasized: 'Акцентная',
  spring: 'Пружина',
}

export type ReducedMotionMode = 'system' | 'always' | 'never'
export type BackgroundKind = 'solid' | 'linear' | 'radial' | 'conic' | 'mesh' | 'aurora' | 'image'
export type Pattern = 'none' | 'dots' | 'grid' | 'noise' | 'diagonal' | 'stars' | 'topography'
export type ImageFit = 'cover' | 'contain' | 'stretch' | 'tile'
export type ComponentStyle = 'solid' | 'soft' | 'outline' | 'glass'

export const BACKGROUND_KINDS: BackgroundKind[] = [
  'solid',
  'linear',
  'radial',
  'conic',
  'mesh',
  'aurora',
  'image',
]

export const BACKGROUND_KIND_LABELS: Record<BackgroundKind, string> = {
  solid: 'Однотонный',
  linear: 'Линейный градиент',
  radial: 'Радиальный',
  conic: 'Конический',
  mesh: 'Сетка из пятен',
  aurora: 'Аврора',
  image: 'Картинка',
}

export const PATTERNS: Pattern[] = [
  'none',
  'dots',
  'grid',
  'noise',
  'diagonal',
  'stars',
  'topography',
]

export const PATTERN_LABELS: Record<Pattern, string> = {
  none: 'Без узора',
  dots: 'Точки',
  grid: 'Сетка',
  noise: 'Шум',
  diagonal: 'Диагональ',
  stars: 'Звёзды',
  topography: 'Горизонтали',
}

export const IMAGE_FITS: ImageFit[] = ['cover', 'contain', 'stretch', 'tile']

export const IMAGE_FIT_LABELS: Record<ImageFit, string> = {
  cover: 'Заполнить',
  contain: 'Вместить',
  stretch: 'Растянуть',
  tile: 'Замостить',
}

export const COMPONENT_STYLES: ComponentStyle[] = ['solid', 'soft', 'outline', 'glass']

export const COMPONENT_STYLE_LABELS: Record<ComponentStyle, string> = {
  solid: 'Плотный',
  soft: 'Мягкий',
  outline: 'Контурный',
  glass: 'Стекло',
}

export const REDUCED_MOTION_MODES: ReducedMotionMode[] = ['system', 'always', 'never']

export const ARTWORK_LABELS: Record<Artwork, string> = {
  none: 'Нет',
  sunburst: 'Лучи',
  halftone: 'Растр',
  blocks: 'Блоки',
  circuit: 'Схема',
  waves: 'Волны',
  confetti: 'Конфетти',
  bubbles: 'Пузыри',
  contours: 'Горизонтали',
}

export const SECTION_LABELS: Record<Section, string> = {
  appearance: 'Оформление',
  instances: 'Сборки',
  mods: 'Моды',
  settings: 'Настройки',
  about: 'О лаунчере',
}

export const SHORTCUT_LABELS: Record<Shortcut, string> = {
  appearance: 'Оформление',
  instances: 'Сборки',
  mods: 'Моды',
  settings: 'Настройки',
  log: 'Журнал',
  'random-theme': 'Случайная тема',
  'toggle-background': 'Сменить фон',
  performance: 'Производительность',
}

export const PLACEMENTS: Placement[] = ['centered', 'left', 'right', 'banner']

export const PLACEMENT_LABELS: Record<Placement, string> = {
  centered: 'По центру',
  left: 'Слева',
  right: 'Справа',
  banner: 'Полоса снизу',
}

export const CORNERS_VALUES: Corners[] = ['sharp', 'rounded', 'pill', 'squircle']

export const CORNERS_LABELS: Record<Corners, string> = {
  sharp: 'Прямые',
  rounded: 'Скруглённые',
  pill: 'Пилюли',
  squircle: 'Скруглённый квадрат',
}

export const MOTION_STYLES: MotionStyle[] = ['still', 'smooth', 'brisk', 'springy', 'sharp']

export const MOTION_STYLE_LABELS: Record<MotionStyle, string> = {
  still: 'Без движения',
  smooth: 'Плавно',
  brisk: 'Быстро',
  springy: 'Пружинисто',
  sharp: 'Резко',
}

export const LABEL_CASES: LabelCase[] = ['as-written', 'upper', 'title']

export const LABEL_CASE_LABELS: Record<LabelCase, string> = {
  'as-written': 'Как написано',
  upper: 'ЗАГЛАВНЫМИ',
  title: 'С Заглавной',
}

export interface ImageRef {
  kind: 'local'
  path: string
}

export interface Palette {
  accent: HexColor
  accent_hover: HexColor
  on_accent: HexColor
  surface: HexColor
  surface_raised: HexColor
  surface_sunken: HexColor
  text: HexColor
  text_muted: HexColor
  border: HexColor
  success: HexColor
  warning: HexColor
  danger: HexColor
  shadow: HexColor
  elevation: number
}

export interface Shape {
  radius_sm: number
  radius: number
  radius_lg: number
  radius_pill: number
  border_width: number
  ring_width: number
}

export interface Typography {
  family: FontFamily
  display_family: FontFamily
  scale: number
  weight_regular: number
  weight_bold: number
  letter_spacing: number
  line_height: number
  uppercase_labels: boolean
}

export interface Motion {
  duration_fast: number
  duration: number
  duration_slow: number
  easing: Easing
  reduced: ReducedMotionMode
  ambient: boolean
  hover_intensity: number
}

export interface Decor {
  grain: number
  vignette: number
  glow: number
  aurora: number
  rim: number
  pattern_scale: number
  pattern: Pattern
  scanlines: boolean
}

/**
 * Large hand-built backdrop drawn behind the window.
 *
 * A closed union on purpose: a theme may *choose* artwork, never supply it.
 * There is no way for a theme file to carry markup or a data URL.
 */
export type Artwork =
  | 'none'
  | 'sunburst'
  | 'halftone'
  | 'blocks'
  | 'circuit'
  | 'waves'
  | 'confetti'
  | 'bubbles'
  | 'contours'

export const ARTWORK_VALUES: Artwork[] = [
  'none',
  'sunburst',
  'halftone',
  'blocks',
  'circuit',
  'waves',
  'confetti',
  'bubbles',
  'contours',
]

export interface Background {
  kind: BackgroundKind
  colors: HexColor[]
  angle: number
  image: ImageRef | null
  image_fit: ImageFit
  image_opacity: number
  image_blur: number
  dim: number
  artwork: Artwork
  artwork_strength: number
}

/**
 * A section of the launcher a theme may reorder, hide or promote.
 *
 * A closed list: a theme arranges capabilities that already exist and can never
 * introduce one. There is no field for an invented action, markup or a script.
 */
export type Section = 'appearance' | 'instances' | 'mods' | 'settings' | 'about'

export const SECTION_VALUES: Section[] = ['appearance', 'instances', 'mods', 'settings', 'about']

/** An existing capability that may be surfaced on the home screen. */
export type Shortcut =
  | 'appearance'
  | 'instances'
  | 'mods'
  | 'settings'
  | 'log'
  | 'random-theme'
  | 'toggle-background'
  | 'performance'

export const SHORTCUT_VALUES: Shortcut[] = [
  'appearance',
  'instances',
  'mods',
  'settings',
  'log',
  'random-theme',
  'toggle-background',
  'performance',
]

export type Placement = 'centered' | 'left' | 'right' | 'banner'
export type Corners = 'sharp' | 'rounded' | 'pill' | 'squircle'
export type MotionStyle = 'still' | 'smooth' | 'brisk' | 'springy' | 'sharp'
export type LabelCase = 'as-written' | 'upper' | 'title'

export interface Layout {
  scale: number
  density: number
  order: Section[]
  hidden: Section[]
  shortcuts: Shortcut[]
  placement: Placement
  corners: Corners
  motion: MotionStyle
  motion_scale: number
  label_case: LabelCase
  icon_only: boolean
}

export interface Components {
  button: ComponentStyle
  input: ComponentStyle
  card: ComponentStyle
  translucency: number
  density: number
}

export interface Tokens {
  palette: Palette
  shape: Shape
  typography: Typography
  motion: Motion
  decor: Decor
  background: Background
  components: Components
  layout: Layout
}

export interface ThemeMeta {
  id: string
  name: string
  author: string
  description: string
  prompt: string | null
  created_at: string
  builtin: boolean
  schema: number
}

export interface ThemeSpec {
  meta: ThemeMeta
  tokens: Tokens
}

export interface ThemeSummary {
  id: string
  name: string
  description: string
  author: string
  builtin: boolean
  created_at: string
  accent: string
  background: string
}

export interface WindowState {
  width: number
  height: number
  maximized: boolean
}

export interface Settings {
  schema: number
  player_name: string
  ram_mb: number
  game_data_dir: string | null
  java_path: string | null
  auto_provision_java: boolean
  selected_build: string | null
  last_background: string | null
  concurrent_downloads: number
  close_on_launch: boolean
  minimise_to_tray_on_close: boolean
  window: WindowState
  theme_id: string
  previous_theme_id: string | null
  force_reduce_motion: boolean
  debug_overlay: boolean
  light_rendering: boolean
}

export interface Bootstrap {
  version: string
  settings: Settings
  themes: ThemeSummary[]
  activeTheme: ThemeSpec
  defaultDataDir: string
  dataDir: string
  isFirstRun: boolean
}

export interface ThemeList {
  themes: ThemeSummary[]
  activeId: string
  canUndo: boolean
  defaultId: string
  customCount: number
}

export interface VersionRow {
  id: string
  release_type: ReleaseType
  type_label: string
  release_time: string
  is_latest_release: boolean
  is_latest_snapshot: boolean
  installed: boolean
  supported: boolean
  unsupported_reason: string
  java_major: number
  instance_id: string | null
}

export interface VersionListResponse {
  versions: VersionRow[]
  latestRelease: string
  latestSnapshot: string
  fromCache: boolean
  loadedAt: string
}

export interface VersionGroups {
  latestRelease: string | null
  latestSnapshot: string | null
  releases: number
  snapshots: number
  betas: number
  alphas: number
  installed: number
}

export interface VersionDetails {
  id: string
  mainClass: string
  javaMajor: number
  releaseType: string
  releaseTime: string
  libraryCount: number
  nativeCount: number
  hasAssets: boolean
  assetIndex: string
  clientSize: number
  supported: boolean
  reason: string
}

export interface InstallResponse {
  instanceId: string
  slug: string
  instancePath: string
  libraries: number
  assets: number
  clientSize: number
  javaVersion: number
  javaPath: string
  state: string
}

export type GameState =
  | 'idle'
  | 'preparing'
  | 'downloading'
  | 'starting'
  | 'running'
  | 'exited'
  | 'failed'

export interface GameSession {
  instanceId: string
  versionId: string
  state: GameState
  pid: number | null
  exitCode: number | null
  startedAt: string
  finishedAt: string | null
  message: string
}

export type InstanceState = 'empty' | 'partial' | 'ready'

export interface InstanceSummary {
  id: string
  slug: string
  name: string
  versionId: string
  state: InstanceState
  path: string
  sizeBytes: number
  ramMb: number | null
  createdAt: string
  updatedAt: string
}

export type Severity = 'fixed' | 'warning' | 'rejected'

export interface ThemeIssue {
  severity: Severity
  field: string
  message: string
}

export interface ValidationReport {
  errors: ThemeIssue[]
  warnings: ThemeIssue[]
  fixes: ThemeIssue[]
}

export interface JavaRuntime {
  path: string
  version: string
  major: number
  vendor: string
  kind: 'jdk' | 'jre'
  source: string
}

export interface BackgroundImage {
  id: string
  path: string
  assetUrl: string
  fileName: string
  sizeBytes: number
}

export interface LogRecord {
  seq: number
  level: 'info' | 'warn' | 'error'
  target: string
  message: string
}

/** Progress events pushed from Rust. */
export type DownloadEvent =
  | { kind: 'file'; id: string; bytes: number; total: number; done: boolean; failed: boolean }
  | {
      kind: 'aggregate'
      job: string
      doneFiles: number
      totalFiles: number
      bytes: number
      totalBytes: number
    }
  | { kind: 'note'; job: string; message: string }

export interface AggregateProgress {
  job: string
  doneFiles: number
  totalFiles: number
  bytes: number
  totalBytes: number
  note: string
}

// ---------------------------------------------------------------------------
// Builds
// ---------------------------------------------------------------------------

/**
 * A mod loader a build may use.
 *
 * `none` is vanilla. `quilt` and `opti_fine` are named so the UI can explain
 * why they are unavailable instead of silently omitting them.
 */
export type ModLoader = 'none' | 'fabric' | 'forge' | 'quilt' | 'neo_forge' | 'opti_fine'

export const MOD_LOADER_VALUES: ModLoader[] = [
  'none',
  'fabric',
  'forge',
  'neo_forge',
  'quilt',
  'opti_fine',
]

export const MOD_LOADER_LABELS: Record<ModLoader, string> = {
  none: 'Vanilla',
  fabric: 'Fabric',
  forge: 'Forge',
  neo_forge: 'NeoForge',
  quilt: 'Quilt',
  opti_fine: 'OptiFine',
}

/** A build: an isolated game directory with its own version, loader and mods. */
export interface BuildSummary {
  id: string
  slug: string
  name: string
  versionId: string
  launchId: string | null
  loader: ModLoader
  loaderVersion: string
  state: InstanceState
  path: string
  sizeBytes: number
  ramMb: number | null
  width: number | null
  height: number | null
  jvmArgs: string[]
  gameArgs: string[]
  iconUrl: string | null
  createdAt: string
  updatedAt: string
  lastPlayedAt: string | null
}

export interface CreateBuildRequest {
  name: string
  versionId: string
  loader?: ModLoader
  loaderVersion?: string
  ramMb?: number | null
  width?: number | null
  height?: number | null
  jvmArgs?: string[]
  gameArgs?: string[]
}

/** Partial edit of a build. Omitted fields keep their current value. */
export interface BuildPatch {
  name?: string
  ramMb?: number | null
  width?: number | null
  height?: number | null
  jvmArgs?: string[]
  gameArgs?: string[]
}

export interface LoaderOption {
  loader: ModLoader
  /** The exact loader version string; sent back verbatim when installing. */
  version: string
  /** Short form for display. */
  label: string
  stable: boolean
}

export interface LoaderCatalog {
  gameVersion: string
  fabric: LoaderOption[]
  forge: LoaderOption[]
  neoforge: LoaderOption[]
  /** Plain-language reasons a loader has nothing for this version. */
  notes: string[]
}

export interface BuildInstallResult {
  slug: string
  launchId: string
  libraries: number
  assets: number
  javaVersion: number
  javaPath: string
}

export interface BuildPaths {
  root: string
  saves: string
  mods: string
  config: string
  resourcepacks: string
  logs: string
}

// ---------------------------------------------------------------------------
// Mods
// ---------------------------------------------------------------------------

export interface InstalledMod {
  projectId: string
  slug: string
  title: string
  versionId: string
  versionNumber: string
  filename: string
  /** `true` for a jar Sifora found in `mods/` but did not install. */
  manual: boolean
  sizeBytes: number
  installedAt: string
  iconUrl: string | null
}

export interface ModList {
  slug: string
  gameVersion: string
  loader: ModLoader
  loaderVersion: string
  mods: InstalledMod[]
  manualCount: number
  canLoadMods: boolean
}

export interface ModFile {
  url: string
  filename: string
  size: number
  sha1: string | null
  primary: boolean
}

export interface ModDependency {
  projectId: string | null
  versionId: string | null
  dependencyType: string
}

export interface ModVersion {
  id: string
  name: string
  versionNumber: string
  gameVersions: string[]
  loaders: string[]
  versionType: string
  downloads: number
  published: string
  files: ModFile[]
  dependencies: ModDependency[]
}

export interface ModSummary {
  projectId: string
  slug: string
  title: string
  description: string
  author: string
  downloads: number
  follows: number
  categories: string[]
  versions: string[]
  iconUrl: string | null
  updated: string
  license: string
  clientSide: string
  serverSide: string
}

export interface ModSearchPage {
  hits: ModSummary[]
  offset: number
  limit: number
  totalHits: number
}

/** A closed set of sort orders; the Rust side rejects anything else. */
export type ModIndex =
  | 'relevance'
  | 'downloads'
  | 'follows'
  | 'newest'
  | 'updated'

export interface ModSearchQuery {
  /** Free-text query. Empty browses everything. */
  query: string
  index?: ModIndex
  categories?: string[]
  versions?: string[]
  loaders?: string[]
  /** `mod` (default), `modpack`, `resourcepack`, `shader`, `datapack`, `plugin`. */
  projectType?: string
  clientSide?: 'required' | 'optional' | 'unsupported' | 'unknown'
  serverSide?: 'required' | 'optional' | 'unsupported' | 'unknown'
  limit?: number
  offset?: number
}

export interface ModTag {
  name: string
  projectType: string
  header: string
}

export interface ModTagBundle {
  categories: ModTag[]
  loaders: string[]
  gameVersions: string[]
  projectTypes: string[]
  indexes: string[]
}

export interface ModDetails {
  projectId: string
  slug: string
  title: string
  description: string
  body: string
  categories: string[]
  gameVersions: string[]
  loaders: string[]
  downloads: number
  followers: number
  iconUrl: string | null
  updated: string
  published: string
  clientSide: string
  serverSide: string
  projectType: string
  licenseName: string
  sourceUrl: string | null
  issuesUrl: string | null
}

export interface BuildModContext {
  slug: string
  name: string
  gameVersion: string
  loader: ModLoader
  loaderVersion: string
  modrinthLoader: string | null
}