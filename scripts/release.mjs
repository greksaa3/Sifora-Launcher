#!/usr/bin/env node
/**
 * One-command release pipeline for Sifora Launcher.
 *
 *   node scripts/release.mjs              full build: check → test → app → installer
 *   node scripts/release.mjs --run        ... and then launch the built launcher
 *   node scripts/release.mjs --fast       app only, no installer
 *   node scripts/release.mjs --skip-tests skip the Rust unit tests
 *   node scripts/release.mjs --debug      app only, debug profile
 *
 * The point is that a newcomer should never have to know the order of
 * `vite build` → `cargo build --features custom-protocol` → NSIS, or which
 * artifact ended up where.
 */
import { spawn, spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { existsSync, readdirSync, statSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { setTimeout as sleep } from 'node:timers/promises'

const root = resolve(fileURLToPath(new URL('..', import.meta.url)))
const args = process.argv.slice(2)
const flag = (name) => args.includes(`--${name}`)

const skipTests = flag('skip-tests')
const fast = flag('fast')
const debug = flag('debug')
const run = flag('run')
const profile = debug ? 'debug' : 'release'
const tauriDir = join(root, 'src-tauri')
const targetDir = join(tauriDir, 'target', profile)

const isWindows = process.platform === 'win32'
const exeName = isWindows ? 'sifora-launcher.exe' : 'sifora-launcher'

/* ------------------------------------------------------------------ output */

const tty = process.stdout.isTTY
const style = (code, text) => (tty ? `[${code}m${text}[0m` : text)
const dim = (t) => style('2', t)
const bold = (t) => style('1', t)
const green = (t) => style('32', t)
const red = (t) => style('31', t)
const yellow = (t) => style('33', t)
const cyan = (t) => style('36', t)

let stepNumber = 0
function step(title) {
  stepNumber += 1
  console.log(`\n${cyan(`[${stepNumber}]`)} ${bold(title)}`)
}
function ok(message) {
  console.log(`    ${green('✓')} ${message}`)
}
function info(message) {
  console.log(`    ${dim(message)}`)
}
function warn(message) {
  console.log(`    ${yellow('!')} ${message}`)
}
function fail(message) {
  console.error(`    ${red('✗')} ${message}`)
}

/* -------------------------------------------------------------------- run */

function runStep(label, command, commandArgs, options = {}) {
  return new Promise((resolvePromise, rejectPromise) => {
    const started = Date.now()
    const child = spawn(command, commandArgs, {
      cwd: options.cwd ?? root,
      stdio: 'inherit',
      shell: isWindows,
      env: { ...process.env, ...options.env },
    })
    child.on('error', (error) => {
      fail(`${label}: ${error.message}`)
      rejectPromise(error)
    })
    child.on('exit', (code) => {
      const seconds = ((Date.now() - started) / 1000).toFixed(1)
      if (code === 0) {
        info(`${label} — ${seconds}s`)
        resolvePromise()
      } else {
        fail(`${label} завершилась с кодом ${code}`)
        rejectPromise(new Error(`${label} failed (${code})`))
      }
    })
  })
}

function have(command) {
  const probe = spawnSync(command, ['--version'], { shell: isWindows, stdio: 'ignore' })
  return probe.status === 0
}

function humanSize(bytes) {
  const units = ['B', 'KB', 'MB', 'GB']
  let value = bytes
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit += 1
  }
  return `${value.toFixed(value >= 10 || unit === 0 ? 0 : 1)} ${units[unit]}`
}

function findInstaller() {
  const bundle = join(targetDir, 'bundle')
  if (!existsSync(bundle)) return null
  const found = []
  const walk = (dir, depth = 0) => {
    if (depth > 3) return
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const full = join(dir, entry.name)
      if (entry.isDirectory()) walk(full, depth + 1)
      else if (/\.(exe|msi|dmg|AppImage|deb|rpm|appimage)$/i.test(entry.name)) found.push(full)
    }
  }
  walk(bundle)
  return found.length ? found : null
}

/* ----------------------------------------------------------------- stages */

async function preflight() {
  step('Проверка окружения')
  const problems = []

  if (!have(isWindows ? 'node' : 'node')) problems.push('Node.js не найден (нужен 20.19+ или 22.12+)')
  if (!have('cargo')) problems.push('Rust не найден — установите через https://rustup.rs')
  if (!existsSync(join(root, 'node_modules'))) problems.push('Зависимости не установлены — выполните npm install')

  if (problems.length) {
    for (const problem of problems) fail(problem)
    throw new Error('preflight failed')
  }

  ok(`Node ${process.version}`)
  const cargo = spawnSync('cargo', ['--version'], { shell: isWindows, encoding: 'utf8' })
  ok(cargo.stdout.trim())
  ok(`профиль: ${profile}${fast ? ' (без установщика)' : ''}`)
}

async function checks() {
  step('Проверки')
  await runStep('Проверка типов (vue-tsc)', isWindows ? 'npx.cmd' : 'npx', ['vue-tsc', '--noEmit', '-p', 'tsconfig.json'])

  if (skipTests) {
    warn('тесты Rust пропущены (--skip-tests)')
    return
  }
  await runStep('Тесты Rust', 'cargo', ['test', '--lib'], { cwd: tauriDir })
}

async function buildApp() {
  if (fast) {
    // `--fast` builds the app by hand so the installer step is skipped. The
    // full path below lets the Tauri CLI drive the whole chain, because it
    // already knows the correct order and would otherwise re-run `vite build`.
    step('Сборка интерфейса')
    await runStep('Vite production build', isWindows ? 'npx.cmd' : 'npx', ['vite', 'build'])

    step('Сборка приложения (Rust)')
    const cargoArgs =
      profile === 'release'
        ? ['build', '--release', '--features', 'tauri/custom-protocol']
        : ['build', '--features', 'tauri/custom-protocol']
    await runStep(`cargo ${cargoArgs.join(' ')}`, 'cargo', cargoArgs, { cwd: tauriDir })
    return []
  }

  step('Сборка приложения и установщика')
  await runStep('tauri build', isWindows ? 'npx.cmd' : 'npx', ['tauri', 'build'])

  const installers = findInstaller()
  if (!installers) {
    warn('установщик не найден в target/bundle — возможно, нужен доступ в интернет для NSIS')
    return []
  }
  for (const path of installers) ok(`${path} (${humanSize(statSync(path).size)})`)
  return installers
}

async function launch() {
  step('Запуск')
  const exe = join(targetDir, exeName)
  if (!existsSync(exe)) {
    fail(`не найден ${exe}`)
    return
  }
  const child = spawn(exe, { cwd: root, detached: true, stdio: 'ignore' })
  child.unref()
  ok(`запущен ${exe} (pid ${child.pid})`)
}

/* ------------------------------------------------------------------- main */

async function main() {
  console.log(bold('Sifora Launcher — сборка'))
  await preflight()
  await checks()

  const installers = await buildApp()
  if (fast) warn('установщик пропущен (--fast)')

  console.log(`\n${bold('Готово')}`)
  const exe = join(targetDir, exeName)
  if (existsSync(exe)) ok(`приложение: ${exe} (${humanSize(statSync(exe).size)})`)
  if (installers.length) ok(`установщики: ${installers.length}`)
  console.log(dim('\nЗапуск собранного приложения: npm start'))

  if (run) {
    await sleep(400)
    await launch()
  }
}

main().catch((error) => {
  console.error(`\n${red('Сборка не завершена')}: ${error.message}`)
  process.exit(1)
})