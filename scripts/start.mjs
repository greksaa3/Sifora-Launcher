#!/usr/bin/env node
/**
 * Launch the built launcher.
 *
 *   npm start                run the release build
 *   npm start -- --debug     run the debug build
 *
 * If the requested profile has not been built yet, this says exactly which
 * command to run instead of failing with a cryptic ENOENT.
 */
import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { existsSync } from 'node:fs'
import { join, resolve } from 'node:path'

const root = resolve(fileURLToPath(new URL('..', import.meta.url)))
const isWindows = process.platform === 'win32'
const profile = process.argv.includes('--debug') ? 'debug' : 'release'
const exe = join(root, 'src-tauri', 'target', profile, isWindows ? 'sifora-launcher.exe' : 'sifora-launcher')

if (!existsSync(exe)) {
  console.error(`Сборка «${profile}» ещё не готова — нет файла:\n  ${exe}`)
  console.error('\nСоберите его одной командой:\n  npm run release' + (profile === 'debug' ? ' -- --debug' : ''))
  process.exit(1)
}

console.log(`Запуск ${exe}`)
const child = spawn(exe, { cwd: root, detached: true, stdio: 'ignore' })
child.on('error', (error) => {
  console.error(`Не удалось запустить: ${error.message}`)
  process.exit(1)
})
child.unref()
console.log(`pid ${child.pid}`)