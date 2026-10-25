//! Minecraft subsystem.
//!
//! ```text
//! manifest   — official version list (network, cached)
//! version    — per-version metadata + inheritance resolution
//! download   — resumable, checksum-verified, bounded-concurrency downloads
//! assets     — asset index handling (objects / virtual / legacy layouts)
//! libraries  — Maven-style library resolution and native extraction
//! java       — JDK/JRE discovery and provisioning
//! instances  — isolated per-build game directories
//! loaders    — Fabric / Forge / NeoForge discovery and installation
//! modrinth   — mod browsing, search and installation
//! launcher   — argument construction and process management
//! ```

pub mod assets;
pub mod download;
pub mod instances;
pub mod java;
pub mod launcher;
pub mod libraries;
pub mod loaders;
pub mod manifest;
pub mod modrinth;
pub mod version;