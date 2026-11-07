<div align="center">

<br />

<img src="assets/logo.png" alt="RL Swapper" width="108" />

# RL Swapper

Minimalist client-side cosmetic swapper and custom texture injector for Rocket League.

[![Downloads](https://img.shields.io/github/downloads/c1yq/RL-Swapper/total?style=flat-square&label=downloads&labelColor=111111&color=2563eb)](https://github.com/c1yq/RL-Swapper/releases)
![Platform](https://img.shields.io/badge/platform-windows-262626?style=flat-square&labelColor=111111)
![Python](https://img.shields.io/badge/python-3.10+-262626?style=flat-square&labelColor=111111)
![Discord](https://img.shields.io/badge/discord-c1ya-262626?style=flat-square&labelColor=111111)
[![License](https://img.shields.io/badge/license-MIT-262626?style=flat-square&labelColor=111111)](LICENSE)

<br />

[Download Latest `.exe`](https://github.com/c1yq/RL-Swapper/releases/latest) &nbsp;·&nbsp; [Features](#features) &nbsp;·&nbsp; [Usage](#usage) &nbsp;·&nbsp; [Development](#development)

<br />

</div>

---

## Features

- **Universal Item Swapper** — Swap any item with any item in the same category across all 12 cosmetic slots (`8,798+` verified items: Bodies, Decals, Wheels, Boosts, Goal Explosions, Trails, Toppers, Antennas, Paint Finishes, Engine Audio, Banners, and Avatar Borders), including shared-container packages and CTR-encrypted items.
- **Custom Profile Pictures & Banners** — Inject PNG or JPG images directly into avatar borders (`1:1` crop with native frame compositing; recommended border: *XP Level 25*) and player banners (`14:3` crop without vertical stretching; recommended banner: *Topographic*).
- **Bulk Randomizer** — Populate every player banner and avatar border in the game client from any local image folder. Works well alongside [Pinterest Bulk Downloader](https://github.com/c1yq/Pinterest-bulk-downloader).
- **Pure OLED Interface** — Clean `#000000` sidebar layout with instant search, per-category selection memory, and car-specific Decal filter pills.
- **Pristine Backups & Silent Update Sync** — Re-swapping an already modified item automatically restores its original `.bak` baseline first. Game updates are detected on launch and silently reset outdated swaps.

---

## Usage

1. Download [`RL_Swapper.exe`](https://github.com/c1yq/RL-Swapper/releases/latest) from the [Releases](https://github.com/c1yq/RL-Swapper/releases) page.
2. Launch `RL_Swapper.exe` (run as Administrator if Rocket League is installed under `C:\Program Files`).
3. Your Epic Games or Steam `CookedPCConsole` directory is detected automatically.
4. Pick the item you **want** on the left, select the item you have **equipped** on the right, and click **Swap**.

Use **Reset Swaps** in the sidebar at any time to restore all modified packages to vanilla.

---

## Development

### Run from Source

```bash
git clone https://github.com/c1yq/RL-Swapper.git
cd RL-Swapper
pip install -r requirements.txt
python SwapperGUI.py
```

### Build Standalone Executable

```bash
pyinstaller --noconfirm RL_Swapper.spec
```

The single-file executable is written to `dist/RL_Swapper.exe`.

---

## Structure

```text
RL-Swapper/
├── assets/              # Application icon & logo
├── SwapperGUI.py        # Pure OLED desktop interface
├── rl_asset_swapper.py  # Surgical ExportTable rewiring & swap engine
├── rl_upk_editor.py     # UPK binary parser, zlib chunker & AES crypto
├── items.json           # Verified Rocket League cosmetic catalog
├── keys_map.json        # Package-to-AES key mapping table
├── keys.txt             # Indexed AES-256 decryption key list
└── RL_Swapper.spec      # PyInstaller build configuration
```

---

## Contact & Notes

For questions or help, message **`c1ya`** on Discord.

All modifications are strictly local and client-side—only your client renders swapped cosmetics, while gameplay physics, hitboxes, memory, and network packets remain untouched. Released under the [MIT License](LICENSE).
