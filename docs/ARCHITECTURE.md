# Sifora Architecture

This document explains the decisions behind the first stage: how the layers
divide responsibility, and why each boundary was drawn where it was.

---

## 1. Layering

```text
Vue 3 (renderer)          presentation only
   │  typed invoke via @/lib/ipc
   ▼
Rust commands              validation + orchestration, one function per capability
   │
   ├── mc::     Minecraft: network, disk, processes
   ├── theme::  design tokens: schema, validation, composition
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
2. **Deserialisation** — `deny_unknown_fields` on every struct. A model that
   invents a key gets a hard error, not a silent extension point.
3. **Re-serialisation** — `Color` parses `#rgb`/`#rgba`/`#rrggbb`/`#rrggbbaa`
   and emits canonical lowercase hex. A literal such as
   `"#fff; background:url(http://evil)"` fails to parse and never reaches the
   stylesheet. By the time a value becomes CSS it is structurally incapable of
   carrying punctuation.

### 3.2 Why validation runs three times

| When | Function | Guards against |
|---|---|---|
| On write (IPC) | `ThemeStore::insert` | bad input from the renderer or a provider |
| On read (disk) | `ThemeStore::load` | a hand-edited or corrupted theme file |
| Before render | `validate_theme` + `parseTheme` (TS) | a mismatched or replaced bridge |

Validation is not only a filter: it *repairs* what can be repaired (contrast,
radii ramp, NaN, missing gradient stops) and rejects what cannot. The report is
returned to the UI so the user is told what changed.

### 3.3 Why the remote model is untrusted

```rust
// system prompt: a constant in this file, never built from user input
let request = serde_json::json!({ "messages": [
    { "role": "system", "content": SYSTEM_PROMPT },
    { "role": "user",   "content": user_message(description) },
]});
```

- The description is wrapped in `<request>` tags and sanitised, so an injected
  `</request>` cannot terminate the framing. `sanitize_text` also strips `<`
  and `>`, which makes tag injection impossible regardless of framing.
- The reply is parsed as JSON, then fed through `coerce_into_schema`, which
  merges only keys that already exist in a known token group and requires the
  merged object to deserialise into the exact Rust type.
- Remote background images are explicitly dropped: `background.image = None`.
- Anything that fails falls back to the offline composer, and the result
  reports `fell_back: true`.

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

### Instance isolation

```
<game-data>/instances/<slug>/
├─ instance.json     metadata (version, RAM, per-instance mods profile)
├─ saves/ config/ resourcepacks/ shaderpacks/ screenshots/ logs/
├─ natives/          extracted native libraries
├─ assets/           object store links / virtual / resources
└─ mods/             stage 2
```

Shared and immutable: `libraries/`, `assets/objects/`, `versions/`.
Mutable and per-instance: everything else. Two versions can never corrupt each
other's worlds, options or mod configuration.

---

## 5. Known limitations of stage 1

Stated plainly so nothing is over-claimed:

- No mod loaders (Fabric, Forge, NeoForge, Quilt, OptiFine) and no mod UI.
- No Microsoft account authentication — offline mode only.
- `stop_game` does not kill the process; Minecraft manages its own shutdown,
  and the launcher reports that rather than pretending otherwise.
- The menu button is a placeholder that explains what stage 2 will add.
- `rd-*` and `c0.0.*` alpha builds are listed but marked unsupported: they
  predate the launch wrapper and cannot be started from a normal classpath.