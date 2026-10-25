<script setup lang="ts">
/** Diagnostics: what the launcher is made of, where things live, recent log. */
import { ref } from 'vue'
import { Boxes, Cpu, FileText, Shield } from '@lucide/vue'

import { app } from '@/lib/ipc'
import type { LogRecord } from '@/types'

defineProps<{ version: string; dataDir: string; gameData: string }>()

const records = ref<LogRecord[]>([])
const loading = ref(false)

async function loadLog(): Promise<void> {
  loading.value = true
  try {
    records.value = await app.tailLog(150)
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <div class="sf-view">
    <section class="sf-card">
      <header class="sf-card__head">
        <h2 class="sf-card__title"><Shield :size="16" /> О лаунчере</h2>
      </header>

      <p class="sf-card__lead">
        Sifora — лёгкий лаунчер Minecraft на Tauri 2 и Rust. Интерфейс на Vue 3,
        вся установка, загрузка файлов и запуск игры выполняются в Rust.
        Electron и постоянно работающий Node.js здесь нет.
      </p>

      <div class="sf-grid-2">
        <div class="sf-row">
          <span class="sf-row__label">
            <span class="sf-row__title">Версия</span>
            <span class="sf-row__hint">{{ version }}</span>
          </span>
        </div>
        <div class="sf-row">
          <span class="sf-row__label">
            <span class="sf-row__title">Данные</span>
            <span class="sf-row__hint sf-mono">{{ dataDir }}</span>
          </span>
        </div>
        <div class="sf-row">
          <span class="sf-row__label">
            <span class="sf-row__title">Игровые файлы</span>
            <span class="sf-row__hint sf-mono">{{ gameData }}</span>
          </span>
        </div>
      </div>
    </section>

    <section class="sf-card">
      <header class="sf-card__head">
        <h2 class="sf-card__title"><Cpu :size="16" /> Приватность</h2>
      </header>
      <ul class="sf-facts">
        <li><Boxes :size="14" /> Учётные данные Mojang не запрашиваются: игра запускается в автономном режиме.</li>
        <li><Shield :size="14" /> Интерфейсу недоступны файловая система, сеть и процессы — только вызовы команд Rust.</li>
        <li><Shield :size="14" /> Темы меняют только внешний вид и не могут отключить функции лаунчера.</li>
        <li><Shield :size="14" /> Моды скачиваются только с проверенных адресов, имя файла обязано быть простым именем <span class="sf-mono">.jar</span>.</li>
      </ul>
    </section>

    <section class="sf-card">
      <header class="sf-card__head">
        <h2 class="sf-card__title"><FileText :size="16" /> Журнал</h2>
        <button class="sf-btn sf-btn--ghost" type="button" :disabled="loading" @click="loadLog">
          {{ loading ? 'Читаем…' : 'Показать' }}
        </button>
      </header>

      <p v-if="!records.length" class="sf-muted">
        Журнал появляется после загрузки. Он хранится в памяти и не пишется на диск.
      </p>

      <pre v-else class="sf-log sf-mono">{{
        records.map((record) => `[${record.level}] ${record.message}`).join('\n')
      }}</pre>
    </section>
  </div>
</template>

<style scoped>
@import './panel.css';

.sf-facts {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 0.45rem;
}

.sf-facts li {
  display: flex;
  align-items: flex-start;
  gap: 0.5rem;
  font-size: 0.78rem;
  color: var(--sf-text-muted);
  line-height: 1.45;
}

.sf-facts svg {
  flex: none;
  margin-top: 0.15rem;
  color: var(--sf-accent);
}

.sf-log {
  margin: 0;
  max-height: 16rem;
  overflow: auto;
  padding: 0.6rem;
  border-radius: var(--sf-radius);
  background: color-mix(in srgb, var(--sf-text) 6%, transparent);
  font-size: 0.68rem;
  line-height: 1.5;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  user-select: text;
}
</style>