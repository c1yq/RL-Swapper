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

[Download Latest `.exe`](https://github.com/c1yq/RL-Swapper/releases/latest) &nbsp;·&nbsp; [Features](#features) &nbsp;·&nbsp; [Usage](#usage) &nbsp;·&nbsp; [Development](#development) &nbsp;·&nbsp; [Contact](#contact--notes)

<br />

</div>

---

## Features

- **Universal Item Swapper** — Swaps any cosmetic item in your inventory with any desired item across all 12 categories (`8,798+` items including Bodies, Decals, Wheels, Boosts, Goal Explosions, Trails, Toppers, Antennas, Paint Finishes, Engine Audio, Banners, and Avatar Borders) through surgical cooked package export rewiring.
- **Direct UPK Paint Programming** — Bakes all 14 official Rocket League painted variants (`Titanium White`, `Black`, `Crimson`, `Cobalt`, `Sky Blue`, `Gold`, and more) directly into swapped package material instances and particle parameters even if your equipped donor item is unpainted.
- **Custom Hex Color Injection** — Lets you choose any arbitrary 24-bit sRGB color via an interactive palette or hexadecimal input (`#RRGGBB`) to program custom linear color vectors into cosmetic shader networks.
- **Auto-Resetting Color System** — Automatically resets your active color selection back to `Default` immediately following every successful swap so subsequent swaps preserve their authentic appearance without manual re-selection.
- **Custom Profile Picture Injection** — Injects any 1:1 image file into equipped avatar border packages using byte-for-byte cooked zlib compression that guarantees zero in-game crashes.
- **Custom Player Banner Injection** — Crops and patches custom 14:3 high-resolution artwork into player banner packages so your personalized graphics show up across scoreboards, goal replays, and post-game podiums.
- **Bulk Image Randomizer** — Loops through any local folder of images to automatically randomize custom artwork across every installed avatar border or player banner package in a single batch operation.
- **Pure OLED Interface** — Features a pitch-black `#000000` aesthetic with instant fuzzy search, per-category selection memory, live previews, and native Windows 10/11 dark titlebars.
- **Car-Specific Decal Filtering** — Identifies decal body compatibility automatically and provides responsive chassis filter pills (`All`, `Universal`, `Octane`, `Fennec`, `Dominus`, and more) to prevent incompatible cross-chassis swaps.
- **Silent Game Update Detection** — Validates package headers on startup against known base revisions, automatically alerting you to game updates from Epic Games or Steam and safely restoring stale mods before conflicts arise.
- **Pristine Baseline Backups** — Automatically creates untouched `.bak` backups before modifying any game file and silently restores the original baseline before applying chained modifications to prevent package degradation.
- **Dedicated Reset Swaps Manager** — Provides an integrated, branded management window that displays every currently active package swap and allows restoring individual items or all files back to vanilla in one click.
- **Automatic Texture Cache Cleaner** — Clears stale cooked texture cache files from your Rocket League user directory immediately after swapping so your newly injected cosmetics appear instantly in-game without requiring a computer restart.
- **Standalone Portable Executable** — Bundles the full cosmetic catalog, decryption keys, and binary editor into a single self-contained executable that requires zero external dependencies or Python installations to run.

---

## Usage

1. Download [`RL_Swapper.exe`](https://github.com/c1yq/RL-Swapper/releases/latest) from the [Releases](https://github.com/c1yq/RL-Swapper/releases) page.
2. Launch `RL_Swapper.exe` (run as Administrator if Rocket League is installed under `C:\Program Files`).
3. Your Epic Games or Steam `CookedPCConsole` directory is detected automatically.
4. Select the item you **want** on the left, pick the item you have **equipped** on the right, pick an optional color, and click **Apply**.

Use **Reset Swaps** in the sidebar at any time to inspect your active modifications or restore original game files.

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

The single-file executable is generated at `dist/RL_Swapper.exe`.

---

## Structure

```text
RL-Swapper/
├── assets/              # Application icon & brand logo
├── SwapperGUI.py         # Pure OLED desktop interface
├── rl_asset_swapper.py   # Surgical ExportTable rewiring & swap engine
├── rl_color_injector.py  # Binary UObject paint & custom RGB color engine
├── rl_upk_editor.py      # UPK binary parser, zlib chunker & AES crypto
├── items.json           # Verified Rocket League cosmetic catalog
├── keys_map.json        # Package-to-AES key mapping table
├── keys.txt             # Indexed AES-256 decryption key list
└── RL_Swapper.spec      # PyInstaller build configuration
```

---

## Contact & Notes

For questions or help, message **`c1ya`** on Discord.

All modifications are strictly local and client-side—only your client renders swapped cosmetics, while gameplay physics, hitboxes, memory, and network packets remain untouched. Released under the [MIT License](LICENSE).
