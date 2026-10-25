# Sifora Architecture

This document explains the decisions behind the launcher: how the layers divide
responsibility, and why each boundary was drawn where it was.

---

## 1. Layering

```text
Vue 3 (renderer)          presentation only
   │  typed invoke via @/lib/ipc
   ▼
Rust commands              validation + orchestration, one function per capability
   │
   ├── mc::     Minecraft: network, disk, processes
   ├── theme::  design tokens: schema, validation, storage
   └── state::  settings, caches, HTTP clients, process slot
```

**Rule 1 — the renderer has no capabilities.** It cannot touch the filesystem,
the network, a process, or the Tauri ACL. The CSP (`default-src 'self'`,
`object-src 'none'`, `frame-src 'none'`) means that even if markup escaped a
bug, it could not load or exfiltrate anything.

**Rule 2 — state lives in Rust.** Settings, the theme catalogue, the manifest
cache, the running process slot and the download ledgers are all Rust-side.
The renderer holds transient UI state only.

**Rule 3 — one function per capability.** `commands/mod.rs` contains the
complete, auditable list. `tauri::generate_handler!` resolves command names at
compile time, so a typo is a build error rather than a runtime surprise.

---

## 2. Lightweight by construction

| Concern | Decision | Why |
|---|---|---|
| Shell | Tauri + system WebView2 | No bundled Chromium (~180 MB) |
| Frontend state | Plain `ref`/`reactive` singletons | Pinia would add ~5 KB and a layer of ceremony for one screen |
| Data fetching | One `bootstrap` command | The main screen paints from a single payload instead of five round trips |
| Logging | 400-entry ring buffer in memory | No logging crate, no file handles, no worker threads |
| Date formatting | ~15 lines | A date library for four labels is not worth the bytes |
| MD5 | 60 lines | Needed only for the offline UUID |
| UUIDs | FNV-1a + timestamp | Avoids a `uuid`/`rand` dependency and its entropy source |
| Registry access | `reg.exe` via `std::process` | Avoids a Windows-only crate and its API surface |
| Concurrency | `tokio` task + `AtomicBool` | The process supervisor is one task; cancellation is one flag |

Release profile: `opt-level = "s"`, `lto = true`, `codegen-units = 1`,
`panic = "abort"`, `strip = true`.

---

## 3. The theme security model

This is the part of the system that had to be right by construction rather
than by review.

### 3.1 Why a closed schema

A theme is data. The temptation is to allow arbitrary properties
(`{"tokens": {"anything": "..."}}`) for flexibility. That is precisely what
makes injection possible, because "arbitrary property" eventually becomes
"arbitrary CSS".

So the schema is closed in three independent ways:

1. **Types** — colours are a `Color` struct, numbers are `f32`/`u16`, choices
   are Rust enums. There is no `String` that reaches CSS.
2. **Deserialisation** — `deny_unknown_fields` on every struct. A hand-edited
   file that invents a key gets a hard error, not a silent extension point.
3. **Re-serialisation** — `Color` parses `#rgb`/`#rgba`/`#rrggbb`/`#rrggbbaa`
   and emits canonical lowercase hex. A literal such as
   `"#fff; background:url(http://evil)"` fails to parse and never reaches the
   stylesheet. By the time a value becomes CSS it is structurally incapable of
   carrying punctuation.

### 3.2 Why validation runs three times

| When | Function | Guards against |
|---|---|---|
| On write (IPC) | `ThemeStore::insert` | bad input from the renderer |
| On read (disk) | `ThemeStore::load` | a hand-edited or corrupted theme file |
| Before render | `validate_theme` + `parseTheme` (TS) | a mismatched or replaced bridge |

Validation is not only a filter: it *repairs* what can be repaired (contrast,
radii ramp, NaN, missing gradient stops) and rejects what cannot. The report is
returned to the UI so the user is told what changed.

### 3.3 Why an imported theme is untrusted

There is no generator any more, so the only untrusted theme input is a file the
user chooses. It is treated exactly like network input was:

- The bytes are capped at 256 KiB before parsing, so a large document cannot be
  used to stall the app.
- `serde` with `deny_unknown_fields` on every struct means an invented key is a
  hard error rather than a silent extension point.
- `validate_theme` runs before the spec is handed back, so a file is held to the
  same standard as a theme written in the editor.
- The `meta.id` in the file is **discarded**: the store assigns a fresh id and
  marks the theme non-built-in, so an import can never overwrite an existing
  theme or impersonate a built-in one. A colliding name gets a numeric suffix.

Export writes the same envelope the store uses (`{ "spec": … }`), so an exported
file is byte-compatible with `themes/` and can be dropped into place by hand.

### 3.4 Why a theme cannot remove functionality

The renderer and Rust are different trust domains. Hiding a button changes
what is *visible*; the command behind it is registered in `generate_handler!`
at compile time and stays callable. There is no mechanism by which theme data
reaches the capability set, the command list or the window configuration —
those are compile-time constants.

---

## 4. Minecraft pipeline

```text
manifest (network, 6 h cache)
   └─ version document chain (inheritsFrom, depth ≤ 8, cycle-guarded)
        ├─ client jar          1 connection,  SHA-1 verified
        ├─ libraries           4 concurrent, SHA-1 verified, shared cache
        ├─ natives             extracted to <instance>/natives (flattened)
        └─ assets              8 concurrent, content-addressed object store
                               → hard-linked into the instance
```

### Decisions worth recording

- **Inheritance merge** replaces libraries by `group:artifact`, not by the full
  coordinate. A child document that bumps a version must replace the parent's
  entry, not add a second copy to the classpath.
- **Assets use hard links** where the filesystem allows it. An instance costs
  kilobytes instead of the ~250 MB an asset index implies; it falls back to a
  copy automatically on FAT32/exFAT or when links are denied.
- **Three asset layouts** are supported because Mojang changed the storage
  format twice: object store (`assets/objects/xx/hash`), virtual
  (`assets/virtual/<index>/…`, 1.6–1.7.2) and `map_to_resources`
  (`assets/resources/…`, ≤1.5.x).
- **Downloads are resumable**: a partial file lives beside the target with a
  `.part` suffix; a `Range:` request continues it, and a `200` instead of a
  `206` correctly restarts from zero. Cancellation is checked between chunks.
- **Java selection prefers an exact major-version match** over a newer runtime,
  because old Minecraft crashes on modern JVMs, but never returns anything
  older than required.
- **Arguments are a `Vec<String>`**, never a joined command line. Windows
  quoting bugs cannot become argument injection. Dangerous JVM flags are
  dropped by prefix.

### Build isolation

```
<game-data>/instances/<slug>/
├─ instance.json     metadata (version, loader, RAM, Java, window, args, icon)
├─ mods.json         the ledger of mods this build installed, and from where
├─ saves/ config/ mods/ resourcepacks/ shaderpacks/ screenshots/ logs/
├─ natives/          extracted native libraries
└─ assets/           object store links / virtual / resources
```

Shared and immutable: `libraries/`, `assets/objects/`, `versions/`.
Mutable and per-build: everything else. Two builds can never corrupt each other's
worlds, options or mod configuration, and each one's resolved loader profile is
recorded in its own metadata (`launch_id`), so the loader is resolved once rather
than guessed on every launch.

---

## 5. Known limitations

Stated plainly so nothing is over-claimed:

- **Quilt and OptiFine are not supported.** They answer with an explicit
  "unsupported" error rather than a silent failure.
- **CurseForge is not a mod source**; only Modrinth is.
- **No Microsoft account authentication** — offline mode only.
- `stop_game` does not kill the process; Minecraft manages its own shutdown,
  and the launcher reports that rather than pretending otherwise.
- `rd-*` and `c0.0.*` alpha builds are listed but marked unsupported: they
  predate the launch wrapper and cannot be started from a normal classpath.
- **No theme generator.** Themes are edited by hand or imported; an imported file
  is validated exactly like a hand-made one.