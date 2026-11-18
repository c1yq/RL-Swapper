<div align="center">

<br />

<img src="assets/logo.png" alt="RL Swapper" width="96" />

# RL Swapper

**Native Rust Cosmetic Swapper, Custom Studio & Workshop Map Loader for Rocket League**

[![Downloads](https://img.shields.io/github/downloads/c1yq/RL-Swapper/total?style=flat-square&label=downloads&labelColor=111111&color=2563eb)](https://github.com/c1yq/RL-Swapper/releases)
![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11-262626?style=flat-square&labelColor=111111)
![Rust](https://img.shields.io/badge/rust-2021%20edition-262626?style=flat-square&labelColor=111111)
![Discord](https://img.shields.io/badge/discord-c1ya-262626?style=flat-square&labelColor=111111)
[![License](https://img.shields.io/badge/license-MIT-262626?style=flat-square&labelColor=111111)](LICENSE)

[What It Does](#what-rl-swapper-can-do) &nbsp;·&nbsp; [Quick Start](#quick-start) &nbsp;·&nbsp; [Downloads Hub & Presets](#downloadsrl-swapper-hub--presets) &nbsp;·&nbsp; [Building](#building-from-source) &nbsp;·&nbsp; [Open-Source Acknowledgments](#acknowledgments--open-source-credits)

</div>

---

## What RL Swapper Can Do

**RL Swapper** is a lightweight, single-executable (`~14 MB`, `< 105 MB` RAM) client-side tool written in pure Rust that lets you customize Rocket League visuals offline and locally without injecting DLLs into the running game process:

- **Cosmetic Item Swapper (`9,500+` Items across 12 Slots)** — Swap any item you own (`Target`) with any item in Rocket League's catalog (`Donor`) across **Body**, **Decal**, **Wheels**, **Rocket Boost**, **Goal Explosion**, **Trail**, **Topper**, **Antenna**, **Paint Finish**, **Engine Audio**, **Player Banner**, and **Avatar Border**. Includes car hitbox compatibility indicators (`Octane`, `Dominus`, `Hybrid`, `Breakout`, `Plank`, `Merc`), item pinning (`★ Pin`), and **14 official Paint Colors + custom `#RRGGBB` shader tinting**.
- **Custom Decal Studio (AlphaConsole Compatible)** — Import AlphaConsole `.zip`, `.json`, folder, or `.png` decal packs (supporting diffuse skin + mask channels and multi-skin UPKs) with a live 2D UV car preview (`Octane`, `Fennec`, `Dominus`), UV template exporter, and a **Saved Decals** library switcher.
- **Avatar PFP & Player Banner Studios (with Interactive Picture Cutter)** — Load or drag-and-drop any `.png`/`.jpg`/`.webp` image, crop and zoom it with the built-in **1:1 Avatar Cutter** or **14:3 Banner Cutter** (with rule-of-thirds and safe-zone overlays), preview it inside any of Rocket League's **177 Avatar Borders** or **740 Player Banners** (with optional scoreboard name baking), and inject directly into the `.upk` package.
- **Custom Match Ball Studio** — Import AlphaConsole ball packs or custom textures (`Diffuse`, `Normal`, and `Masks` channels) with a 2D spherical orb preview and inject into `GameInfo_Soccar_SF.upk`.
- **Epic & Steam Workshop Map Loader + 1-Click Workshop Textures** — Browse and search **1,000+ community Workshop maps** (aggregated from *Lethamyr*, *BakkesPlugins*, and *Steam Workshop*) or import local `.udk`/`.upk`/`.zip` maps into Free Play (`Labs_Underpass_P.upk`). Includes an automatic UPK dependency scanner and a **1-Click "Download Textures"** button that downloads and installs the shared ~44.5 MB `Workshop Textures` pack directly into `CookedPCConsole` to prevent checkerboard floors.
- **3-Tier Pristine Backup Vault & Auto-Patch Guard** — Automatically backs up untouched `.upk` files in both `<package>.upk.bak` and `Downloads\RL Swapper\Backups\Pristine Vault\`, detects when Rocket League updates (`RocketLeague.exe` size/timestamp guard) so stale backups never overwrite new game patches, and lets you undo (`Ctrl+Z`) or restore all files to vanilla in 1 click.

---

## Quick Start

1. Download **`RL_Swapper.exe`** from [Releases](https://github.com/c1yq/RL-Swapper/releases/latest) (or [build from source](#building-from-source)) and launch it.
2. RL Swapper automatically detects your Epic Games or Steam `TAGame\CookedPCConsole` folder (or click **Folder** in the top bar to select it manually).
3. Close Rocket League before applying swaps so cooked package changes load cleanly on next launch:
   - **Cosmetics**: Pick a **Donor** item on the left, an owned **Target** item on the right, optionally choose a **Paint** color, and click **Swap Item**.
   - **Workshop**: Browse or import a map, click **Download Textures** if prompted for missing shared texture packs, and click **Load in Underpass** (then open *Play → Training → Free Play → Underpass* in-game).
   - **Studio**: Open **Studio** (`Decals`, `Avatar PFP`, `Banner`, or `Match Ball`) to crop images or import AlphaConsole packs.

---

## `Downloads\RL Swapper\` Hub & Presets

Click **Open Hub** in the top bar at any time to open `C:\Users\<you>\Downloads\RL Swapper\` in Windows Explorer. Everything RL Swapper saves is organized in plain sight:

```text
Downloads/RL Swapper/
├── Presets/
│   └── <Preset Name>/          # Each preset has its own folder with editable PNGs + preset.json
│       ├── preset.json
│       ├── decal_skin.png
│       ├── decal_mask.png
│       ├── pfp.png
│       ├── banner.png
│       └── ball_diffuse.png
├── Custom Items/               # Auto-saved library powering the "Saved ..." dropdowns in Studio
│   ├── Decals/
│   ├── PFP/
│   ├── Banners/
│   └── Balls/
├── Workshop Maps/              # Deduplicated local library of downloaded/imported .udk/.upk maps
├── Workshop Textures/          # Cached shared Workshop texture packages (.upk / .tfc)
├── Backups/Pristine Vault/     # Untouched vanilla .upk backups
└── swap_history.log            # Timestamped log of every swap, injection, and restore
```

---

## Building from Source

Requires Windows 10/11 and a Rust toolchain (`cargo` / `rustc`):

```bash
git clone https://github.com/c1yq/RL-Swapper.git
cd RL-Swapper
cargo build --release --bin RL_Swapper
```

The standalone executable is produced at `target/release/RL_Swapper.exe`.

### Running the Test Suite

```bash
cargo test --lib
cargo test --bin RL_Swapper
cargo test --test verify_v4_all_features
cargo test --test verify_v5_exhaustive_options
```

---

## Project Structure

```text
RL-Swapper/
├── Cargo.toml                  # Crate manifest & dependencies
├── build.rs                    # Windows icon & version resource compiler
├── assets/                     # Application icon (icon.ico) & logo (logo.png)
├── src/
│   ├── lib.rs                  # Library module exports
│   ├── upk_engine.rs           # UPK parser, AES-256 ECB/CTR crypto, surgical rewiring, paint & backup vault
│   ├── icon_system.rs          # DXT1/DXT5/BGRA8 codecs, .tfc & _T_SF.upk extractors, MediaWiki fallback
│   ├── templates.rs            # 2D Decal UV, Ball & Banner templates, aspect cropper & previews
│   ├── custom_studio.rs        # AlphaConsole Decal/Ball, Avatar PFP & Banner UPK injectors
│   ├── workshop.rs             # Underpass Workshop loader, dependency scanner, catalog & textures downloader
│   └── bin/
│       └── swapper_gui.rs      # Main eframe/egui GUI binary (RL_Swapper)
├── tests/
│   ├── verify_v4_all_features.rs
│   └── verify_v5_exhaustive_options.rs
├── items.json                  # Embedded item catalog (9,503 entries)
├── keys_map.json               # Package AES key index mapping (28,639 packages)
└── keys.txt                    # Indexed AES-256 UPK keys
```

---

## Acknowledgments & Open-Source Credits

RL Swapper stands on the shoulders of awesome open-source projects, libraries, and community pioneers. Huge thanks to:

### Rocket League Modding & Community Projects
- **[AlphaConsole](https://github.com/AlphaConsole)** & **[BakkesMod](https://bakkesmod.com/)** — For pioneering Rocket League custom decal/ball texture pack formats (`json` + diffuse/mask conventions) and inspiring the community modding ecosystem.
- **[Lethamyr](https://lethamyr.com/)** & **[BakkesPlugins](https://bakkesplugins.com/)** — For hosting community custom Workshop maps and the shared *Workshop Textures* dependency pack for Epic Games & Steam players.
- **[RLUPKTool](https://github.com/LongerWarrior/RLUPKTool)**, **[UEShader](https://github.com/ueshader)** & **[UE Explorer / EliotVU](https://eliotvu.com/portfolio/view/21/ue-explorer)** — For documenting Unreal Engine 3 `.upk` / `.udk` package structures, `NameTable`/`ImportTable`/`ExportTable` serialization, and cooked `Texture2D` / `.tfc` mipmap layouts.
- **[Rocket League Wiki (Fandom)](https://rocketleague.fandom.com/)** — For community-maintained cosmetic item imagery used by the online thumbnail fallback scraper.

### Rust Open-Source Crates
- **[`egui`](https://github.com/emilk/egui) & [`eframe`](https://github.com/emilk/egui/tree/master/crates/eframe)** by Emil Ernerfeldt & contributors — Immediate-mode GPU GUI framework powering RL Swapper's responsive, low-memory interface.
- **[`image`](https://github.com/image-rs/image)** by the `image-rs` team — Pure-Rust PNG, JPEG, and WebP decoding, Lanczos3 resampling, and image compositing.
- **[`aes`](https://github.com/RustCrypto/block-ciphers), [`cipher`](https://github.com/RustCrypto/traits) & [`ctr`](https://github.com/RustCrypto/stream-ciphers)** by the **RustCrypto** project — Pure-Rust AES-256 ECB and CTR encryption/decryption for `.upk` packages.
- **[`flate2`](https://github.com/rust-lang/flate2-rs)** & **[`zip`](https://github.com/zip-rs/zip2)** — Zlib stream compression/decompression for UE3 compressed chunks and `.zip` archive extraction.
- **[`rayon`](https://github.com/rayon-rs/rayon)** — Data-parallelism library used for multi-threaded UPK chunk compression and icon syncing.
- **[`reqwest`](https://github.com/seanmonstar/reqwest)** — HTTP client powering the online Workshop map browser and streaming texture/map downloads.
- **[`serde`](https://github.com/serde-rs/serde) & [`serde_json`](https://github.com/serde-rs/json)** — High-performance JSON serialization for catalogs, state, and preset manifests.
- **[`rfd`](https://github.com/PolyMeilex/rfd), [`walkdir`](https://github.com/BurntSushi/walkdir), [`dirs`](https://github.com/dirs-dev/dirs-rs), [`base64`](https://github.com/marshallpierce/rust-base64), [`chrono`](https://github.com/chronotope/chrono) & [`winapi`](https://github.com/retep998/winapi-rs)** — Native Windows file dialogs, directory traversal, base64 encoding, timestamps, and Win32 process inspection.

---

## Disclaimer & License

- All modifications performed by RL Swapper are strictly local and client-side (`TAGame\CookedPCConsole`); car hitboxes, physics, game memory, and network packets are never modified.
- Not affiliated with or endorsed by Psyonix LLC or Epic Games, Inc.
- Released under the [MIT License](LICENSE). Contact: **`c1ya`** on Discord.
