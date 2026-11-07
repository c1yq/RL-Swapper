import ctypes
import json
import os
import random
import re
import shutil
import sys
import tempfile
import threading
import time
import tkinter as tk
from pathlib import Path
from tkinter import filedialog, messagebox

import customtkinter as ctk
from PIL import Image, ImageTk

import rl_asset_swapper
from rl_asset_swapper import SwapOptions, load_items
import rl_upk_editor

ctk.set_appearance_mode("Dark")

# ── Apple-Inspired Pure OLED Palette ─────────────────────────────────────────
OLED_BLACK = "#000000"
OLED_ELEVATED = "#09090B"
OLED_INPUT = "#0C0C0E"
OLED_HOVER = "#111114"
OLED_SELECTED = "#18181B"
OLED_BORDER = "#141417"
OLED_BORDER_FOCUS = "#27272A"

TEXT_PRIMARY = "#F5F5F7"
TEXT_SECONDARY = "#8E8E93"
TEXT_MUTED = "#52525B"
TEXT_DANGER = "#FF453A"
TEXT_SUCCESS = "#32D74B"

CTA_BG = "#FFFFFF"
CTA_HOVER = "#E5E5EA"
CTA_TEXT = "#000000"

FONT_FAMILY = "Segoe UI"

SLOT_LABELS = [
    ("Body", "Bodies"),
    ("Decal", "Decals"),
    ("Wheels", "Wheels"),
    ("Rocket Boost", "Boosts"),
    ("Goal Explosion", "Goal Explosions"),
    ("Trail", "Trails"),
    ("Topper", "Toppers"),
    ("Antenna", "Antennas"),
    ("Paint Finish", "Paint Finishes"),
    ("Engine Audio", "Engine Audio"),
    ("Player Banner", "Banners"),
    ("Avatar Border", "Avatar Borders"),
]

HITBOX_MAP = {
    'Octane': ['Octane', 'Octane ZSR', 'Fennec', 'Takumi', 'Takumi RX-T', 'Twinzer', 'Bone Shaker', 'Marauder', 'Scarab', 'Zippy', 'Armadillo', 'Grog', 'Triton', 'Proteus', 'Vulcan', 'Fast 4WD', 'Mudcat', 'Mudcat GXT', 'Harbinger', 'Harbinger GXT', 'Jackal', 'Dingo', 'Outlaw', 'Outlaw GXT', 'Nomad', 'Nomad GXT', 'Aston Martin Valhalla', 'BMW M240i', 'Bugatti Centodieci', 'Ford Bronco Raptor', 'Ford F-150 RLE', 'Ford Mustang Mach-E RLE', 'Honda Civic Type R', 'Honda Civic Type R-LE', 'Jurassic Jeep Wrangler', 'Maestro', 'Nissan Silvia', 'Nissan Silvia RLE', 'Primo', 'Redline', 'Sweet Tooth', 'Volkswagen Golf GTI', 'Volkswagen Golf GTI RLE', 'Admiral', 'Mako'],
    'Dominus': ['Dominus', 'Dominus GT', 'Ice Charger', 'Aftershock', 'Masamune', 'Ripper', 'DeLorean Time Machine', 'Batmobile (1989)', 'Fast & Furious Dodge Charger', 'Fast & Furious Nissan Skyline', 'Ecto-1', 'K.I.T.T.', 'McLaren 570S', 'Gazella GT', 'MR11', 'Nemesis', 'Diestro', 'Ronin', 'Ronin GXT', 'Peregrine TT', 'Tyranno', 'Tyranno GXT', 'Mamba', 'Nissan Z Performance', "007's Aston Martin DBS", 'Aston Martin DB5', 'Audi RS 3', 'BMW M4 CSL', 'Bugatti Bolide', 'Chikara', 'Chikara GXT', 'Emperor', 'Emperor II', 'Ferrari 296 GTB', 'Ford Mustang Shelby GT500', 'Formula 1', 'Guardian', 'Guardian GXT', 'Lamborghini Countach LPI 800-4', 'Lamborghini Huracan STO', 'McLaren 765LT', 'Porsche 911 Turbo', 'Porsche 911 Turbo RLE', "Samus' Gunship", 'Maserati Grecale Trofeo', 'Nissan Fairlady Z', 'Fairlady'],
    'Breakout': ['Breakout', 'Breakout Type-S', 'Animus GP', 'Cyclone', 'Samurai', 'Komodo', 'Nexus', 'Nexus SC'],
    'Plank': ['Batmobile (2016)', 'Mantis', 'Paladin', 'Centio', 'Centio V17', 'Artemis', 'Artemis GXT', 'Sentinel'],
    'Hybrid': ['Endo', 'Venom', 'X-Devil', 'X-Devil Mk2', 'Jager 619', 'Jager 619 RS', 'Nimbus', 'Tygris', 'Insidio', 'R3MX', 'R3MX GXT', 'Esper', 'Silvia'],
    'Merc': ['Merc', 'Battle Bus', 'Ford Bronco', 'Nomad']
}


def get_hitbox(name: str) -> str:
    lower = name.lower()
    for k, cars in HITBOX_MAP.items():
        for car in cars:
            cl = car.lower()
            if cl == lower or cl in lower:
                return k
    return "Unknown"


def get_resource_path(relative_path: str) -> str:
    try:
        base_path = sys._MEIPASS
    except Exception:
        base_path = os.path.dirname(os.path.abspath(__file__))
    return os.path.join(base_path, relative_path)


def apply_oled_titlebar(win):
    """Apply pure pitch-black native titlebar on Windows 10/11."""
    try:
        win.update_idletasks()
        hwnd = ctypes.windll.user32.GetParent(win.winfo_id()) or win.winfo_id()
        dark = ctypes.c_int(1)
        ctypes.windll.dwmapi.DwmSetWindowAttribute(hwnd, 20, ctypes.byref(dark), ctypes.sizeof(dark))
        black = ctypes.c_int(0x00000000)
        ctypes.windll.dwmapi.DwmSetWindowAttribute(hwnd, 35, ctypes.byref(black), ctypes.sizeof(black))
    except Exception:
        pass


class CropWindow(ctk.CTkToplevel):
    def __init__(self, parent, image_path, callback, ratio_w=1, ratio_h=1):
        super().__init__(parent)
        self.title("Crop Image")
        self.geometry("760x640")
        self.configure(fg_color=OLED_BLACK)
        self.callback = callback
        self.after(50, lambda: apply_oled_titlebar(self))

        self.original_img = Image.open(image_path).convert("RGBA")

        max_size = 500
        w, h = self.original_img.size
        ratio = min(max_size / w, max_size / h)
        self.disp_w = max(1, int(w * ratio))
        self.disp_h = max(1, int(h * ratio))
        self.ratio = ratio

        self.disp_img = self.original_img.resize((self.disp_w, self.disp_h), Image.LANCZOS)
        self.tk_img = ImageTk.PhotoImage(self.disp_img)

        ctk.CTkLabel(
            self,
            text="Drag inside to position  ·  Drag bottom-right corner to resize",
            font=ctk.CTkFont(family=FONT_FAMILY, size=12),
            text_color=TEXT_SECONDARY,
        ).pack(pady=(18, 8))

        self.canvas = tk.Canvas(
            self, width=self.disp_w, height=self.disp_h,
            bg=OLED_ELEVATED, highlightthickness=1, highlightbackground=OLED_BORDER
        )
        self.canvas.pack(pady=8)
        self.canvas.create_image(0, 0, anchor="nw", image=self.tk_img)

        self.crop_ratio_w = ratio_w
        self.crop_ratio_h = ratio_h

        max_crop_w = self.disp_w
        max_crop_h = int(max_crop_w * (self.crop_ratio_h / self.crop_ratio_w))
        if max_crop_h > self.disp_h:
            max_crop_h = self.disp_h
            max_crop_w = int(max_crop_h * (self.crop_ratio_w / self.crop_ratio_h))

        self.crop_w = int(max_crop_w / 1.4)
        self.crop_h = int(max_crop_h / 1.4)
        self.crop_x = (self.disp_w - self.crop_w) // 2
        self.crop_y = (self.disp_h - self.crop_h) // 2

        self.rect = self.canvas.create_rectangle(
            self.crop_x, self.crop_y, self.crop_x + self.crop_w, self.crop_y + self.crop_h,
            outline="#FFFFFF", width=2
        )
        self.handle = self.canvas.create_rectangle(
            self.crop_x + self.crop_w - 7, self.crop_y + self.crop_h - 7,
            self.crop_x + self.crop_w + 7, self.crop_y + self.crop_h + 7,
            fill="#FFFFFF", outline=OLED_BLACK
        )

        self.canvas.bind("<ButtonPress-1>", self.on_press)
        self.canvas.bind("<B1-Motion>", self.on_drag)
        self.canvas.bind("<Motion>", self.on_hover)

        btn_frame = ctk.CTkFrame(self, fg_color="transparent")
        btn_frame.pack(pady=16)
        ctk.CTkButton(
            btn_frame, text="Cancel", width=100, height=34, corner_radius=8,
            fg_color=OLED_SELECTED, hover_color=OLED_BORDER_FOCUS, text_color=TEXT_PRIMARY,
            font=ctk.CTkFont(family=FONT_FAMILY, size=12, weight="normal"),
            command=self.destroy
        ).pack(side="left", padx=6)
        ctk.CTkButton(
            btn_frame, text="Use Image", width=110, height=34, corner_radius=8,
            fg_color=CTA_BG, hover_color=CTA_HOVER, text_color=CTA_TEXT,
            font=ctk.CTkFont(family=FONT_FAMILY, size=12, weight="bold"),
            command=self.crop_and_close
        ).pack(side="left", padx=6)

        self.start_x = 0
        self.start_y = 0
        self.mode = "move"
        self.grab_set()
        self.focus_set()

    def on_hover(self, event):
        hx1, hy1, hx2, hy2 = self.canvas.coords(self.handle)
        self.canvas.config(cursor="sizing" if (hx1 <= event.x <= hx2 and hy1 <= event.y <= hy2) else "arrow")

    def on_press(self, event):
        self.start_x, self.start_y = event.x, event.y
        hx1, hy1, hx2, hy2 = self.canvas.coords(self.handle)
        self.mode = "resize" if (hx1 <= event.x <= hx2 and hy1 <= event.y <= hy2) else "move"

    def on_drag(self, event):
        dx = event.x - self.start_x
        if self.mode == "move":
            dy = event.y - self.start_y
            self.crop_x = max(0, min(self.disp_w - self.crop_w, self.crop_x + dx))
            self.crop_y = max(0, min(self.disp_h - self.crop_h, self.crop_y + dy))
        else:
            new_w = max(50, self.crop_w + dx)
            new_h = int(new_w * (self.crop_ratio_h / self.crop_ratio_w))
            if self.crop_x + new_w > self.disp_w:
                new_w = self.disp_w - self.crop_x
                new_h = int(new_w * (self.crop_ratio_h / self.crop_ratio_w))
            if self.crop_y + new_h > self.disp_h:
                new_h = self.disp_h - self.crop_y
                new_w = int(new_h * (self.crop_ratio_w / self.crop_ratio_h))
            self.crop_w, self.crop_h = new_w, new_h

        self.start_x, self.start_y = event.x, event.y
        self.canvas.coords(self.rect, self.crop_x, self.crop_y, self.crop_x + self.crop_w, self.crop_y + self.crop_h)
        self.canvas.coords(
            self.handle,
            self.crop_x + self.crop_w - 7, self.crop_y + self.crop_h - 7,
            self.crop_x + self.crop_w + 7, self.crop_y + self.crop_h + 7,
        )

    def crop_and_close(self):
        orig_x = int(self.crop_x / self.ratio)
        orig_y = int(self.crop_y / self.ratio)
        orig_w = int(self.crop_w / self.ratio)
        orig_h = int(self.crop_h / self.ratio)
        cropped = self.original_img.crop((orig_x, orig_y, orig_x + orig_w, orig_y + orig_h))
        out_path = os.path.join(tempfile.gettempdir(), f"cropped_{os.urandom(4).hex()}.png")
        cropped.save(out_path)
        self.callback(out_path)
        self.destroy()


class RLSwapperApp(ctk.CTk):
    @property
    def swaps_log(self) -> str:
        if os.path.exists(self.folder):
            return os.path.join(self.folder, "swaps_log.txt")
        return os.path.join(self.appdata_dir, "swaps_log.txt")

    @property
    def state_file(self) -> Path:
        return Path(self.appdata_dir) / "swapper_state.json"

    def __init__(self):
        super().__init__()
        self.title("RL Swapper")
        self.geometry("1040x700")
        self.minsize(860, 560)
        self.configure(fg_color=OLED_BLACK)
        self.after(60, lambda: apply_oled_titlebar(self))

        icon_path = get_resource_path("icon.ico")
        if not os.path.exists(icon_path):
            icon_path = get_resource_path(os.path.join("assets", "icon.ico"))
        if os.path.exists(icon_path):
            try:
                self.iconbitmap(icon_path)
            except Exception:
                pass

        if getattr(sys, "frozen", False):
            self.app_dir = os.path.dirname(sys.executable)
        else:
            self.app_dir = os.path.dirname(os.path.abspath(__file__))

        self.appdata_dir = os.path.join(os.environ.get("LOCALAPPDATA", self.app_dir), "RLSwapper")
        os.makedirs(self.appdata_dir, exist_ok=True)

        self.app_state = self._load_state()
        saved_folder = self.app_state.get("folder")
        if saved_folder and os.path.exists(saved_folder):
            self.folder = saved_folder
        else:
            self.folder = r"C:\Program Files\Epic Games\rocketleague\TAGame\CookedPCConsole"
            if not os.path.exists(self.folder):
                self.folder = r"C:\Program Files (x86)\Steam\steamapps\common\rocketleague\TAGame\CookedPCConsole"

        try:
            self.items = load_items(Path(get_resource_path("items.json")))
            self.keys_path = Path(get_resource_path("keys.txt"))
        except Exception as e:
            messagebox.showerror("Error", f"Could not load items database: {e}")
            self.destroy()
            return

        self.keys_map = {}
        try:
            map_path = Path(get_resource_path("keys_map.json"))
            if map_path.exists():
                with open(map_path, "r", encoding="utf-8") as f:
                    self.keys_map = json.load(f)
        except Exception:
            pass

        self._load_categorized_items()
        self._load_custom_catalogs()

        # Per-slot UI memory (search queries and selected display strings)
        self.slot_memory = {}
        self.current_nav_key = None
        self.current_mode = "items"  # "items", "custom_pfp", "custom_banner"
        self.nav_buttons = {}
        self.cropped_pfp_path = None
        self.cropped_banner_path = None

        self._build_layout()

        # Run silent Rocket League patch/update check & sync
        undone_on_update = self.sync_game_update_silently()

        # Select initial category
        first_slot = self.ordered_slots[0][0] if self.ordered_slots else None
        if first_slot:
            self.select_slot(first_slot)

        if undone_on_update:
            self.set_status("Rocket League update detected — previous swaps silently reset", subtle=True)

    # ── State & Silent Update Detection ──────────────────────────────────────
    def _load_state(self) -> dict:
        if self.state_file.exists():
            try:
                with open(self.state_file, "r", encoding="utf-8") as f:
                    data = json.load(f)
                    if isinstance(data, dict):
                        data.setdefault("active_swaps", {})
                        return data
            except Exception:
                pass
        return {"folder": None, "last_exe_mtime": None, "last_exe_size": None, "active_swaps": {}}

    def _save_state(self) -> None:
        try:
            self.app_state["folder"] = self.folder
            with open(self.state_file, "w", encoding="utf-8") as f:
                json.dump(self.app_state, f, indent=2)
        except Exception:
            pass

    def _get_rl_exe_path(self) -> Path:
        return Path(self.folder).parent.parent / "Binaries" / "Win64" / "RocketLeague.exe"

    def sync_game_update_silently(self) -> bool:
        """
        Detect if Rocket League updated or replaced modified packages.
        When detected, silently undo all swaps, remove stale .bak files, clear logs & cache.
        """
        folder_path = Path(self.folder)
        if not folder_path.exists():
            return False

        exe_path = self._get_rl_exe_path()
        curr_mtime = None
        curr_size = None
        if exe_path.exists():
            try:
                st = exe_path.stat()
                curr_mtime = st.st_mtime
                curr_size = st.st_size
            except Exception:
                pass

        prev_mtime = self.app_state.get("last_exe_mtime")
        prev_size = self.app_state.get("last_exe_size")
        active_swaps = self.app_state.get("active_swaps", {})

        update_detected = False

        # 1. Executable changed since last session
        if prev_mtime is not None and curr_mtime is not None:
            if abs(curr_mtime - prev_mtime) > 2.0 or (prev_size is not None and curr_size != prev_size):
                update_detected = True

        # 2. A tracked swapped UPK was overwritten externally by Steam/Epic patcher
        if not update_detected and active_swaps:
            for pkg_lower, meta in list(active_swaps.items()):
                upk_file = folder_path / meta.get("pkg_name", pkg_lower)
                swap_time = meta.get("swap_time", 0)
                swapped_size = meta.get("swapped_size")
                if upk_file.exists():
                    try:
                        ust = upk_file.stat()
                        if ust.st_mtime > swap_time + 5.0 or (swapped_size is not None and ust.st_size != swapped_size):
                            update_detected = True
                            break
                    except Exception:
                        pass

        # 3. Pre-existing .bak files older than current RocketLeague.exe while .upk matches patch time
        if not update_detected and prev_mtime is None and curr_mtime is not None:
            for bak in folder_path.glob("*.bak"):
                upk_file = bak.with_suffix("")
                try:
                    if upk_file.exists() and bak.stat().st_mtime < curr_mtime - 60 and upk_file.stat().st_mtime >= curr_mtime - 120:
                        update_detected = True
                        break
                except Exception:
                    pass

        had_artifacts = bool(active_swaps) or any(folder_path.glob("*.bak"))

        if update_detected:
            for bak in list(folder_path.glob("*.bak")):
                upk_file = bak.with_suffix("")
                pkg_key = upk_file.name.lower()
                meta = active_swaps.get(pkg_key, {})
                swap_time = meta.get("swap_time", 0)
                try:
                    # Determine whether Epic/Steam already wrote a fresh patched UPK
                    patched_by_launcher = False
                    if upk_file.exists():
                        ust = upk_file.stat()
                        if swap_time and ust.st_mtime > swap_time + 5.0:
                            patched_by_launcher = True
                        elif curr_mtime and bak.stat().st_mtime < curr_mtime - 60 and ust.st_mtime >= curr_mtime - 120:
                            patched_by_launcher = True

                    if not patched_by_launcher and bak.exists():
                        if upk_file.exists():
                            upk_file.unlink()
                        shutil.copy2(bak, upk_file)
                    bak.unlink()
                except Exception:
                    pass

            self.app_state["active_swaps"] = {}
            self._rewrite_swaps_log()
            self.clear_rl_cache()

        else:
            # Prune any stale log/state entries whose .bak no longer exists on disk
            cleaned = {}
            for pkg_key, meta in active_swaps.items():
                pkg_name = meta.get("pkg_name", pkg_key)
                if (folder_path / f"{pkg_name}.bak").exists():
                    cleaned[pkg_key] = meta
            if cleaned != active_swaps or (not cleaned and os.path.exists(self.swaps_log)):
                self.app_state["active_swaps"] = cleaned
                self._rewrite_swaps_log()

        if curr_mtime is not None:
            self.app_state["last_exe_mtime"] = curr_mtime
            self.app_state["last_exe_size"] = curr_size
        self._save_state()
        return update_detected and had_artifacts

    def record_swap(
        self,
        pkg_name: str,
        donor_label: str,
        target_label: str,
        mode: str = "IN-GAME",
        target_asset_path: str = "",
    ) -> None:
        if mode != "IN-GAME":
            return
        upk_file = Path(self.folder) / pkg_name
        now = time.time()
        swapped_size = None
        swapped_mtime = now
        if upk_file.exists():
            try:
                st = upk_file.stat()
                swapped_size = st.st_size
                swapped_mtime = st.st_mtime
            except Exception:
                pass

        self.app_state.setdefault("active_swaps", {})[pkg_name.lower()] = {
            "pkg_name": pkg_name,
            "donor": donor_label,
            "target": target_label,
            "target_asset_path": target_asset_path,
            "swap_time": swapped_mtime,
            "swapped_size": swapped_size,
        }
        self._save_state()
        self._rewrite_swaps_log()

    def _rewrite_swaps_log(self) -> None:
        active = self.app_state.get("active_swaps", {})
        log_path = self.swaps_log
        try:
            if not active:
                if os.path.exists(log_path):
                    os.remove(log_path)
                return
            with open(log_path, "w", encoding="utf-8") as f:
                for meta in active.values():
                    f.write(f"[IN-GAME] {meta['donor']} -> {meta['target']} [{meta['pkg_name']}]\n")
        except Exception:
            pass

    # ── Data Loading ─────────────────────────────────────────────────────────
    def _scan_disk_files(self) -> dict:
        folder_path = Path(self.folder)
        if not folder_path.is_dir():
            return {}
        try:
            return {fn.lower(): fn for fn in os.listdir(folder_path)}
        except Exception:
            return {}

    def _discover_unlisted_upk_items(self, disk_files: dict, known_pkgs_lower: set) -> list:
        """Auto-discover any keyed *_SF.upk cosmetic packages in CookedPCConsole missing from items.json."""
        if not disk_files:
            return []
        import struct
        folder_path = Path(self.folder)
        rl_asset_swapper._keys_map = self.keys_map
        class_to_slot = {
            "ProductAsset_Body_TA": "Body",
            "ProductAsset_Skin_TA": "Decal",
            "ProductAsset_Wheel_TA": "Wheels",
            "ProductAsset_Boost_TA": "Rocket Boost",
            "ProductAsset_GoalExplosion_TA": "Goal Explosion",
            "ProductAsset_SupersonicTrail_TA": "Trail",
            "ProductAsset_PaintFinish_TA": "Paint Finish",
            "ProductAsset_EngineAudio_TA": "Engine Audio",
            "ProductAsset_PlayerBanner_TA": "Player Banner",
            "ProductAsset_PlayerAvatarBorder_TA": "Avatar Border",
            "ProductAsset_AvatarBorder_TA": "Avatar Border",
        }
        discovered = []
        next_id = 990000
        for fn_low, real_fn in sorted(disk_files.items()):
            if (
                not fn_low.endswith("_sf.upk")
                or fn_low.endswith("_t_sf.upk")
                or fn_low in known_pkgs_lower
                or fn_low.startswith(("park_", "gameinfo_"))
                or "stadium" in fn_low
                or "sfx_" in fn_low
            ):
                continue
            stem = fn_low[:-4]
            if stem not in self.keys_map and stem[:-3] not in self.keys_map:
                continue
            upk_path = rl_asset_swapper.get_pristine_path(folder_path / real_fn)
            key = rl_asset_swapper._get_exact_key(folder_path / real_fn)
            if not key or not upk_path.exists():
                continue
            try:
                with open(upk_path, "rb") as fh:
                    raw = fh.read(65536)
                pfx = rl_asset_swapper._parse_upk_prefix(raw)
                if (pfx.package_flags & 0x8) == 0:
                    continue
                enc_sz = pfx.total_header_size - pfx.garbage_size - pfx.name_offset
                if enc_sz <= 0:
                    continue
                need = pfx.name_offset + ((enc_sz + 15) & ~15)
                if need > len(raw):
                    with open(upk_path, "rb") as fh:
                        raw = fh.read(need)
                eb = raw[pfx.name_offset:need]
                plain = (
                    rl_asset_swapper._aes_ctr_crypt(key, pfx.ctr_nonce, eb)
                    if pfx.ctr_nonce is not None
                    else rl_asset_swapper._aes_ecb_decrypt(key, eb)
                )[:enc_sz]
                c_off = pfx.compressed_chunks_offset
                if c_off < 0 or c_off + 8 > len(plain):
                    continue
                num_c, u0 = struct.unpack_from("<ii", plain, c_off)
                if num_c < 1 or u0 != pfx.depends_offset:
                    continue
                slots_nt = rl_asset_swapper._walk_name_table(plain, pfx.name_count)
                ci = pfx.import_offset - pfx.name_offset
                ce = pfx.export_offset - pfx.name_offset
                imps = [
                    slots_nt[struct.unpack_from("<iiiiiii", plain, ci + i * 28)[5]]["name"]
                    for i in range((ce - ci) // 28)
                ]
                exps = []
                epos = ce
                for _ in range(pfx.export_count):
                    cls_i, _sup, out_i, nm_i, inst_i, _arch = struct.unpack_from("<iiiiii", plain, epos)
                    nc = struct.unpack_from("<i", plain, epos + 48)[0]
                    cn = imps[-cls_i - 1] if (cls_i < 0 and -cls_i - 1 < len(imps)) else "Class"
                    nm = slots_nt[nm_i]["name"]
                    if inst_i > 0:
                        nm = f"{nm}_{inst_i - 1}"
                    exps.append((cn, out_i, nm))
                    epos += 52 + nc * 4 + 20

                def full_path(idx1: int) -> str:
                    pts, seen = [], set()
                    cur = idx1
                    while cur > 0 and cur not in seen and cur <= len(exps):
                        seen.add(cur)
                        _, oi, nm = exps[cur - 1]
                        pts.append(nm)
                        cur = oi
                    pts.reverse()
                    return ".".join(pts)

                for i, (cn, _, _) in enumerate(exps):
                    fp = full_path(i + 1)
                    if fp.lower().startswith("productslots."):
                        continue
                    slot = class_to_slot.get(cn)
                    if cn == "ProductAsset_Attachment_TA":
                        slot = "Antenna" if fp.lower().startswith(("antenna_", "pennant_", "flag_")) else "Topper"
                    if not slot:
                        continue
                    leaf = fp.split(".")[-1]
                    discovered.append(rl_asset_swapper.Item(
                        id=next_id,
                        product=leaf.replace("_", " ").title(),
                        quality="Import",
                        slot=slot,
                        asset_package=real_fn,
                        asset_path=fp,
                    ))
                    next_id += 1
            except Exception:
                continue
        return discovered

    def _load_categorized_items(self) -> None:
        self.categorized_items = {}
        ignored_slots = {
            "Currency", "Crate", "Crates", "Blueprint",
            "Player Title", "Player Anthem", "Esports Team", "Bot", "PremiumInventory"
        }
        disk_files = self._scan_disk_files()
        known_pkgs_lower = {itm.asset_package.lower() for itm in self.items if itm.asset_package}
        extra_items = self._discover_unlisted_upk_items(disk_files, known_pkgs_lower)
        all_source_items = list(self.items) + extra_items

        seen_keys = {}
        seen_assets = {}

        for itm in all_source_items:
            raw_slot = itm.slot
            if not raw_slot or raw_slot in ignored_slots:
                continue
            slot = "Avatar Border" if raw_slot == "PlayerAvatarBorder" else raw_slot

            pkg_low = itm.asset_package.lower()
            if disk_files and pkg_low not in disk_files and f"{pkg_low}.bak" not in disk_files:
                continue

            stem = pkg_low.replace(".upk", "")
            if stem not in self.keys_map and stem.replace("_sf", "") not in self.keys_map:
                continue

            prod_name = str(itm.product)
            stem_bare = pkg_low.replace("_sf.upk", "").replace(".upk", "")
            asset_key = (pkg_low, itm.asset_path.lower())

            if slot not in self.categorized_items:
                self.categorized_items[slot] = []
                seen_keys[slot] = set()
                seen_assets[slot] = set()

            if asset_key in seen_assets[slot]:
                continue
            seen_assets[slot].add(asset_key)

            dedup_key = (prod_name, slot)
            if dedup_key in seen_keys[slot]:
                leaf_hint = itm.asset_path.split(".")[-1] if "." in itm.asset_path else stem_bare
                prod_name = f"{prod_name} ({leaf_hint})"
                dedup_key = (prod_name, slot)
            seen_keys[slot].add(dedup_key)

            display_name = prod_name
            if slot == "Body":
                hb = get_hitbox(prod_name)
                if hb != "Unknown":
                    display_name = f"{prod_name}  ·  {hb}"

            decal_car = None
            if slot == "Decal":
                if ":" in prod_name and "(Universal)" not in prod_name:
                    decal_car = prod_name.split(":", 1)[0].strip()
                else:
                    decal_car = "Universal"

            real_pkg_name = disk_files.get(pkg_low, itm.asset_package) if disk_files else itm.asset_package
            norm_itm = rl_asset_swapper.Item(
                id=itm.id, product=itm.product, quality=itm.quality,
                slot=slot, asset_package=real_pkg_name, asset_path=itm.asset_path
            )

            self.categorized_items[slot].append({
                "display": display_name,
                "product_name": prod_name,
                "item": norm_itm,
                "stem": stem_bare,
                "decal_car": decal_car,
            })

        for slot in self.categorized_items:
            self.categorized_items[slot].sort(key=lambda x: x["display"].lower())

        known_map = dict(SLOT_LABELS)
        self.ordered_slots = [(s, lbl) for s, lbl in SLOT_LABELS if s in self.categorized_items]
        for s in sorted(self.categorized_items.keys()):
            if s not in known_map:
                self.ordered_slots.append((s, s))

    def _load_custom_catalogs(self) -> None:
        try:
            disk_files = self._scan_disk_files()
            with open(get_resource_path("items.json"), "r", encoding="utf-8") as f:
                raw_items = json.load(f).get("Items", [])
            seen_b, seen_p = set(), set()
            self.all_banners = []
            self.all_pfps = []
            for i in raw_items:
                pkg = i.get("AssetPackage")
                if not pkg:
                    continue
                pkg_low = pkg.lower()
                if disk_files and pkg_low not in disk_files and f"{pkg_low}.bak" not in disk_files:
                    continue
                stem = pkg_low.replace(".upk", "")
                if stem not in self.keys_map and stem.replace("_sf", "") not in self.keys_map:
                    continue
                slot = i.get("Slot")
                entry = dict(i)
                if disk_files and pkg_low in disk_files:
                    entry["AssetPackage"] = disk_files[pkg_low]
                if slot == "Player Banner" and pkg_low not in seen_b:
                    seen_b.add(pkg_low)
                    self.all_banners.append(entry)
                elif slot in ("Avatar Border", "PlayerAvatarBorder") and pkg_low not in seen_p:
                    seen_p.add(pkg_low)
                    self.all_pfps.append(entry)
            self.all_banners.sort(key=lambda x: x.get("Product", "").lower())
            self.all_pfps.sort(key=lambda x: x.get("Product", "").lower())
        except Exception:
            self.all_banners = []
            self.all_pfps = []

    # ── Apple-Style Pure OLED Layout ─────────────────────────────────────────
    def _build_layout(self) -> None:
        # Root horizontal container
        self.sidebar = ctk.CTkFrame(self, width=200, fg_color=OLED_BLACK, corner_radius=0)
        self.sidebar.pack(side="left", fill="y")
        self.sidebar.pack_propagate(False)

        # 1px hairline vertical divider
        ctk.CTkFrame(self, width=1, fg_color=OLED_BORDER, corner_radius=0).pack(side="left", fill="y")

        # Main workspace
        self.workspace = ctk.CTkFrame(self, fg_color=OLED_BLACK, corner_radius=0)
        self.workspace.pack(side="left", fill="both", expand=True)

        self._build_sidebar()
        self._build_items_view()
        self._build_custom_pfp_view()
        self._build_custom_banner_view()
        self._build_bottom_bar()

    def _build_sidebar(self) -> None:
        # Brand header
        brand = ctk.CTkFrame(self.sidebar, fg_color="transparent")
        brand.pack(fill="x", padx=18, pady=(22, 16))

        logo_path = get_resource_path("logo.png")
        if not os.path.exists(logo_path):
            logo_path = get_resource_path(os.path.join("assets", "logo.png"))
        if os.path.exists(logo_path):
            try:
                logo_pil = Image.open(logo_path).convert("RGBA")
                self.sidebar_logo = ctk.CTkImage(light_image=logo_pil, dark_image=logo_pil, size=(24, 24))
                ctk.CTkLabel(brand, image=self.sidebar_logo, text="").pack(side="left", padx=(0, 9))
            except Exception:
                pass

        ctk.CTkLabel(
            brand, text="Swapper", text_color=TEXT_PRIMARY,
            font=ctk.CTkFont(family=FONT_FAMILY, size=15, weight="bold")
        ).pack(side="left")

        # Bottom quiet utilities (packed before middle nav so it stays anchored at bottom)
        bottom_util = ctk.CTkFrame(self.sidebar, fg_color="transparent")
        bottom_util.pack(side="bottom", fill="x", padx=12, pady=(8, 16))

        ctk.CTkFrame(bottom_util, height=1, fg_color=OLED_BORDER).pack(fill="x", padx=6, pady=(0, 10))

        ctk.CTkButton(
            bottom_util, text="Game Folder", anchor="w", height=30, corner_radius=8,
            fg_color="transparent", hover_color=OLED_HOVER, text_color=TEXT_MUTED,
            font=ctk.CTkFont(family=FONT_FAMILY, size=12),
            command=self.set_in_dir
        ).pack(fill="x", pady=1)

        ctk.CTkButton(
            bottom_util, text="Reset Swaps", anchor="w", height=30, corner_radius=8,
            fg_color="transparent", hover_color=OLED_HOVER, text_color=TEXT_MUTED,
            font=ctk.CTkFont(family=FONT_FAMILY, size=12),
            command=self.restore_backups
        ).pack(fill="x", pady=1)

        # Scrollable navigation list (invisible scrollbar on pure black unless needed)
        self.nav_scroll = ctk.CTkScrollableFrame(
            self.sidebar, fg_color=OLED_BLACK, corner_radius=0,
            scrollbar_button_color=OLED_BLACK, scrollbar_button_hover_color=OLED_SELECTED
        )
        self.nav_scroll.pack(side="top", fill="both", expand=True, padx=8, pady=0)

        ctk.CTkLabel(
            self.nav_scroll, text="Items", anchor="w", text_color=TEXT_MUTED,
            font=ctk.CTkFont(family=FONT_FAMILY, size=12, weight="bold")
        ).pack(fill="x", padx=10, pady=(2, 4))

        for slot_key, label in self.ordered_slots:
            btn = ctk.CTkButton(
                self.nav_scroll, text=label, anchor="w", height=30, corner_radius=8,
                fg_color="transparent", hover_color=OLED_HOVER, text_color=TEXT_SECONDARY,
                font=ctk.CTkFont(family=FONT_FAMILY, size=13),
                command=lambda s=slot_key: self.select_slot(s)
            )
            btn.pack(fill="x", pady=1)
            self.nav_buttons[f"slot:{slot_key}"] = btn

        ctk.CTkLabel(
            self.nav_scroll, text="Custom", anchor="w", text_color=TEXT_MUTED,
            font=ctk.CTkFont(family=FONT_FAMILY, size=12, weight="bold")
        ).pack(fill="x", padx=10, pady=(16, 4))

        btn_pfp = ctk.CTkButton(
            self.nav_scroll, text="Profile Picture", anchor="w", height=30, corner_radius=8,
            fg_color="transparent", hover_color=OLED_HOVER, text_color=TEXT_SECONDARY,
            font=ctk.CTkFont(family=FONT_FAMILY, size=13),
            command=self.show_custom_pfp
        )
        btn_pfp.pack(fill="x", pady=1)
        self.nav_buttons["custom:pfp"] = btn_pfp

        btn_ban = ctk.CTkButton(
            self.nav_scroll, text="Player Banner", anchor="w", height=30, corner_radius=8,
            fg_color="transparent", hover_color=OLED_HOVER, text_color=TEXT_SECONDARY,
            font=ctk.CTkFont(family=FONT_FAMILY, size=13),
            command=self.show_custom_banner
        )
        btn_ban.pack(fill="x", pady=1)
        self.nav_buttons["custom:banner"] = btn_ban

    def _highlight_nav(self, active_key: str) -> None:
        self.current_nav_key = active_key
        for k, btn in self.nav_buttons.items():
            if k == active_key:
                btn.configure(fg_color=OLED_SELECTED, text_color=TEXT_PRIMARY)
            else:
                btn.configure(fg_color="transparent", text_color=TEXT_SECONDARY)

    def _make_oled_listbox(self, parent):
        frame = ctk.CTkFrame(parent, fg_color=OLED_BLACK, corner_radius=0)
        frame.pack(fill="both", expand=True)

        lb = tk.Listbox(
            frame,
            bg=OLED_BLACK,
            fg=TEXT_SECONDARY,
            font=(FONT_FAMILY, 11),
            selectbackground=OLED_SELECTED,
            selectforeground=TEXT_PRIMARY,
            activestyle="none",
            exportselection=False,
            highlightthickness=0,
            borderwidth=0,
            relief="flat",
        )
        lb.pack(side="left", fill="both", expand=True)

        scroll = ctk.CTkScrollbar(
            frame, command=lb.yview, width=10,
            fg_color=OLED_BLACK, button_color=OLED_BORDER, button_hover_color=OLED_BORDER_FOCUS
        )
        scroll.pack(side="right", fill="y")
        lb.config(yscrollcommand=scroll.set)
        return lb

    def _build_items_view(self) -> None:
        self.items_frame = ctk.CTkFrame(self.workspace, fg_color=OLED_BLACK, corner_radius=0)
        self.items_frame.grid_columnconfigure(0, weight=1)
        self.items_frame.grid_columnconfigure(1, weight=1)
        self.items_frame.grid_rowconfigure(0, weight=0)
        self.items_frame.grid_rowconfigure(1, weight=1)

        # Minimal Apple-OLED Decal Car Filter Bar (shown only when slot == "Decal")
        self.decal_filter = "All"
        self.decal_filter_btns = {}
        self.decal_filter_bar = ctk.CTkFrame(self.items_frame, fg_color=OLED_BLACK, corner_radius=0)
        self.decal_filter_bar.grid(row=0, column=0, columnspan=2, sticky="ew", padx=26, pady=(20, 0))
        for pill in ("All", "Universal", "Octane", "Fennec", "Dominus", "Breakout", "Merc"):
            b = ctk.CTkButton(
                self.decal_filter_bar,
                text=pill,
                width=0,
                height=26,
                corner_radius=7,
                fg_color=OLED_SELECTED if pill == "All" else "transparent",
                hover_color=OLED_HOVER,
                text_color=TEXT_PRIMARY if pill == "All" else TEXT_MUTED,
                font=ctk.CTkFont(family=FONT_FAMILY, size=11),
                command=lambda p=pill: self._set_decal_filter(p),
            )
            b.pack(side="left", padx=(0, 6))
            self.decal_filter_btns[pill] = b
        self.decal_filter_bar.grid_remove()

        # Left column: Replace with (Visual Donor)
        col_left = ctk.CTkFrame(self.items_frame, fg_color=OLED_BLACK, corner_radius=0)
        col_left.grid(row=1, column=0, sticky="nsew", padx=(26, 14), pady=(20, 8))

        ctk.CTkLabel(
            col_left, text="Replace with", anchor="w", text_color=TEXT_SECONDARY,
            font=ctk.CTkFont(family=FONT_FAMILY, size=12, weight="bold")
        ).pack(fill="x", pady=(0, 8))

        self.want_search_var = tk.StringVar()
        self.want_search_entry = ctk.CTkEntry(
            col_left, textvariable=self.want_search_var, placeholder_text="Search",
            height=34, corner_radius=8, border_width=1,
            fg_color=OLED_INPUT, border_color=OLED_BORDER,
            text_color=TEXT_PRIMARY, placeholder_text_color=TEXT_MUTED,
            font=ctk.CTkFont(family=FONT_FAMILY, size=12)
        )
        self.want_search_entry.pack(fill="x", pady=(0, 10))
        self.want_lb = self._make_oled_listbox(col_left)

        # Right column: Equipped item (Sacrifice Target)
        col_right = ctk.CTkFrame(self.items_frame, fg_color=OLED_BLACK, corner_radius=0)
        col_right.grid(row=1, column=1, sticky="nsew", padx=(14, 26), pady=(20, 8))

        ctk.CTkLabel(
            col_right, text="Equipped item", anchor="w", text_color=TEXT_SECONDARY,
            font=ctk.CTkFont(family=FONT_FAMILY, size=12, weight="bold")
        ).pack(fill="x", pady=(0, 8))

        self.have_search_var = tk.StringVar()
        self.have_search_entry = ctk.CTkEntry(
            col_right, textvariable=self.have_search_var, placeholder_text="Search",
            height=34, corner_radius=8, border_width=1,
            fg_color=OLED_INPUT, border_color=OLED_BORDER,
            text_color=TEXT_PRIMARY, placeholder_text_color=TEXT_MUTED,
            font=ctk.CTkFont(family=FONT_FAMILY, size=12)
        )
        self.have_search_entry.pack(fill="x", pady=(0, 10))
        self.have_lb = self._make_oled_listbox(col_right)

        self._suppress_trace = False
        self.want_search_var.trace_add("write", lambda *_: self._on_item_filter())
        self.have_search_var.trace_add("write", lambda *_: self._on_item_filter())
        self.want_lb.bind("<<ListboxSelect>>", lambda _: self._update_selection_summary())
        self.have_lb.bind("<<ListboxSelect>>", lambda _: self._update_selection_summary())

    def _set_decal_filter(self, pill: str) -> None:
        self.decal_filter = pill
        for k, btn in self.decal_filter_btns.items():
            if k == pill:
                btn.configure(fg_color=OLED_SELECTED, text_color=TEXT_PRIMARY)
            else:
                btn.configure(fg_color="transparent", text_color=TEXT_MUTED)
        self._on_item_filter()

    def _build_custom_pfp_view(self) -> None:
        self.pfp_frame = ctk.CTkFrame(self.workspace, fg_color=OLED_BLACK, corner_radius=0)
        self.pfp_frame.grid_columnconfigure(0, weight=1)
        self.pfp_frame.grid_columnconfigure(1, weight=0)
        self.pfp_frame.grid_rowconfigure(0, weight=1)

        left = ctk.CTkFrame(self.pfp_frame, fg_color=OLED_BLACK, corner_radius=0)
        left.grid(row=0, column=0, sticky="nsew", padx=(26, 14), pady=(24, 8))

        ctk.CTkLabel(
            left, text="Equipped avatar border", anchor="w", text_color=TEXT_SECONDARY,
            font=ctk.CTkFont(family=FONT_FAMILY, size=12, weight="bold")
        ).pack(fill="x", pady=(0, 8))

        self.pfp_search_var = tk.StringVar()
        self.pfp_search_var.trace_add("write", self.filter_pfps)
        ctk.CTkEntry(
            left, textvariable=self.pfp_search_var, placeholder_text="Search avatar borders",
            height=34, corner_radius=8, border_width=1,
            fg_color=OLED_INPUT, border_color=OLED_BORDER,
            text_color=TEXT_PRIMARY, placeholder_text_color=TEXT_MUTED,
            font=ctk.CTkFont(family=FONT_FAMILY, size=12)
        ).pack(fill="x", pady=(0, 10))

        self.pfp_listbox = self._make_oled_listbox(left)
        self.pfp_listbox.bind("<<ListboxSelect>>", self.on_pfp_select)

        right = ctk.CTkFrame(self.pfp_frame, width=290, fg_color=OLED_ELEVATED, corner_radius=14, border_width=1, border_color=OLED_BORDER)
        right.grid(row=0, column=1, sticky="ns", padx=(14, 26), pady=(24, 8))
        right.pack_propagate(False)

        ctk.CTkLabel(
            right, text="Profile Picture", text_color=TEXT_PRIMARY,
            font=ctk.CTkFont(family=FONT_FAMILY, size=15, weight="bold")
        ).pack(pady=(28, 4))

        self.pfp_target_var = tk.StringVar(value="")
        self.pfp_name_lbl = ctk.CTkLabel(
            right, text="No border selected", text_color=TEXT_SECONDARY,
            font=ctk.CTkFont(family=FONT_FAMILY, size=12), wraplength=240
        )
        self.pfp_name_lbl.pack(pady=(0, 18))

        preview_wrap = ctk.CTkFrame(right, width=124, height=124, fg_color=OLED_BLACK, corner_radius=12, border_width=1, border_color=OLED_BORDER)
        preview_wrap.pack(pady=6)
        preview_wrap.pack_propagate(False)

        self.lbl_pfp_preview = tk.Label(preview_wrap, bg=OLED_BLACK, fg=TEXT_MUTED, text="1 : 1", font=(FONT_FAMILY, 10))
        self.lbl_pfp_preview.pack(fill="both", expand=True)

        ctk.CTkButton(
            right, text="Choose Image...", width=180, height=34, corner_radius=8,
            fg_color=OLED_SELECTED, hover_color=OLED_BORDER_FOCUS, text_color=TEXT_PRIMARY,
            font=ctk.CTkFont(family=FONT_FAMILY, size=12),
            command=self._choose_pfp_image
        ).pack(pady=(22, 10))

        ctk.CTkButton(
            right, text="Randomize All Borders...", width=180, height=32, corner_radius=8,
            fg_color="transparent", hover_color=OLED_HOVER, text_color=TEXT_MUTED,
            font=ctk.CTkFont(family=FONT_FAMILY, size=12),
            command=self._bulk_pfp
        ).pack(side="bottom", pady=(0, 20))

        self.filter_pfps()

    def _build_custom_banner_view(self) -> None:
        self.banner_frame = ctk.CTkFrame(self.workspace, fg_color=OLED_BLACK, corner_radius=0)
        self.banner_frame.grid_columnconfigure(0, weight=1)
        self.banner_frame.grid_columnconfigure(1, weight=0)
        self.banner_frame.grid_rowconfigure(0, weight=1)

        left = ctk.CTkFrame(self.banner_frame, fg_color=OLED_BLACK, corner_radius=0)
        left.grid(row=0, column=0, sticky="nsew", padx=(26, 14), pady=(24, 8))

        ctk.CTkLabel(
            left, text="Equipped player banner", anchor="w", text_color=TEXT_SECONDARY,
            font=ctk.CTkFont(family=FONT_FAMILY, size=12, weight="bold")
        ).pack(fill="x", pady=(0, 8))

        self.banner_search_var = tk.StringVar()
        self.banner_search_var.trace_add("write", self.filter_banners)
        ctk.CTkEntry(
            left, textvariable=self.banner_search_var, placeholder_text="Search banners",
            height=34, corner_radius=8, border_width=1,
            fg_color=OLED_INPUT, border_color=OLED_BORDER,
            text_color=TEXT_PRIMARY, placeholder_text_color=TEXT_MUTED,
            font=ctk.CTkFont(family=FONT_FAMILY, size=12)
        ).pack(fill="x", pady=(0, 10))

        self.banner_listbox = self._make_oled_listbox(left)
        self.banner_listbox.bind("<<ListboxSelect>>", self.on_banner_select)

        right = ctk.CTkFrame(self.banner_frame, width=290, fg_color=OLED_ELEVATED, corner_radius=14, border_width=1, border_color=OLED_BORDER)
        right.grid(row=0, column=1, sticky="ns", padx=(14, 26), pady=(24, 8))
        right.pack_propagate(False)

        ctk.CTkLabel(
            right, text="Player Banner", text_color=TEXT_PRIMARY,
            font=ctk.CTkFont(family=FONT_FAMILY, size=15, weight="bold")
        ).pack(pady=(28, 4))

        self.banner_target_var = tk.StringVar(value="playerBanner_RLShield_SF.upk")
        self.banner_name_lbl = ctk.CTkLabel(
            right, text="Standard (Default)", text_color=TEXT_SECONDARY,
            font=ctk.CTkFont(family=FONT_FAMILY, size=12), wraplength=240
        )
        self.banner_name_lbl.pack(pady=(0, 18))

        preview_wrap = ctk.CTkFrame(right, width=244, height=62, fg_color=OLED_BLACK, corner_radius=10, border_width=1, border_color=OLED_BORDER)
        preview_wrap.pack(pady=12)
        preview_wrap.pack_propagate(False)

        self.lbl_banner_preview = tk.Label(preview_wrap, bg=OLED_BLACK, fg=TEXT_MUTED, text="14 : 3", font=(FONT_FAMILY, 10))
        self.lbl_banner_preview.pack(fill="both", expand=True)

        ctk.CTkButton(
            right, text="Choose Image...", width=180, height=34, corner_radius=8,
            fg_color=OLED_SELECTED, hover_color=OLED_BORDER_FOCUS, text_color=TEXT_PRIMARY,
            font=ctk.CTkFont(family=FONT_FAMILY, size=12),
            command=self._choose_banner_image
        ).pack(pady=(22, 10))

        ctk.CTkButton(
            right, text="Randomize All Banners...", width=180, height=32, corner_radius=8,
            fg_color="transparent", hover_color=OLED_HOVER, text_color=TEXT_MUTED,
            font=ctk.CTkFont(family=FONT_FAMILY, size=12),
            command=self._bulk_banner
        ).pack(side="bottom", pady=(0, 20))

        self.filter_banners()

    def _build_bottom_bar(self) -> None:
        self.bottom_wrap = ctk.CTkFrame(self.workspace, fg_color=OLED_BLACK, corner_radius=0)
        self.bottom_wrap.pack(side="bottom", fill="x", padx=26, pady=(6, 20))

        ctk.CTkFrame(self.bottom_wrap, height=1, fg_color=OLED_BORDER).pack(fill="x", pady=(0, 14))

        row = ctk.CTkFrame(self.bottom_wrap, fg_color="transparent")
        row.pack(fill="x")

        self.status_lbl = ctk.CTkLabel(
            row, text="Select items to swap", anchor="w", text_color=TEXT_MUTED,
            font=ctk.CTkFont(family=FONT_FAMILY, size=12)
        )
        self.status_lbl.pack(side="left", fill="x", expand=True, padx=(0, 12))

        self.btn_apply = ctk.CTkButton(
            row, text="Apply", width=108, height=34, corner_radius=8,
            fg_color=CTA_BG, hover_color=CTA_HOVER, text_color=CTA_TEXT,
            font=ctk.CTkFont(family=FONT_FAMILY, size=12, weight="bold"),
            command=self._on_primary_apply
        )
        self.btn_apply.pack(side="right")

        self.btn_export = ctk.CTkButton(
            row, text="Export", width=86, height=34, corner_radius=8,
            fg_color=OLED_ELEVATED, hover_color=OLED_SELECTED, text_color=TEXT_SECONDARY,
            border_width=1, border_color=OLED_BORDER,
            font=ctk.CTkFont(family=FONT_FAMILY, size=12),
            command=lambda: self.do_swap(in_game=False)
        )
        self.btn_export.pack(side="right", padx=(0, 8))

    def set_status(self, message: str, subtle: bool = True, color: str = None) -> None:
        c = color if color else (TEXT_SECONDARY if not subtle else TEXT_MUTED)
        self.status_lbl.configure(text=message, text_color=c)

    # ── Navigation Switching ─────────────────────────────────────────────────
    def _save_current_slot_memory(self) -> None:
        if self.current_mode == "items" and getattr(self, "current_slot", None):
            want_sel = self.want_lb.get(self.want_lb.curselection()[0]) if self.want_lb.curselection() else None
            have_sel = self.have_lb.get(self.have_lb.curselection()[0]) if self.have_lb.curselection() else None
            self.slot_memory[self.current_slot] = {
                "q_want": self.want_search_var.get(),
                "q_have": self.have_search_var.get(),
                "want": want_sel,
                "have": have_sel,
            }

    def _hide_all_views(self) -> None:
        self.items_frame.pack_forget()
        self.pfp_frame.pack_forget()
        self.banner_frame.pack_forget()

    def select_slot(self, slot: str) -> None:
        self._save_current_slot_memory()
        self.current_mode = "items"
        self.current_slot = slot
        self._highlight_nav(f"slot:{slot}")
        self._hide_all_views()
        self.items_frame.pack(side="top", fill="both", expand=True)
        self.btn_export.pack(side="right", padx=(0, 8))

        if slot == "Decal":
            self.decal_filter_bar.grid()
        else:
            self.decal_filter_bar.grid_remove()

        mem = self.slot_memory.get(slot, {"q_want": "", "q_have": "", "want": None, "have": None})
        self._suppress_trace = True
        self.want_search_var.set(mem["q_want"])
        self.have_search_var.set(mem["q_have"])
        self._suppress_trace = False

        self._populate_item_lists(restore_want=mem["want"], restore_have=mem["have"])
        self._update_selection_summary()

    def show_custom_pfp(self) -> None:
        self._save_current_slot_memory()
        self.current_mode = "custom_pfp"
        self._highlight_nav("custom:pfp")
        self._hide_all_views()
        self.btn_export.pack_forget()
        self.pfp_frame.pack(side="top", fill="both", expand=True)
        self._update_selection_summary()

    def show_custom_banner(self) -> None:
        self._save_current_slot_memory()
        self.current_mode = "custom_banner"
        self._highlight_nav("custom:banner")
        self._hide_all_views()
        self.btn_export.pack_forget()
        self.banner_frame.pack(side="top", fill="both", expand=True)
        self._update_selection_summary()

    # ── Item Filtering & Selection ───────────────────────────────────────────
    def _is_pkg_swapped(self, pkg_name: str) -> bool:
        return (Path(self.folder) / f"{pkg_name}.bak").exists()

    def _is_entry_swapped(self, entry: dict) -> bool:
        pkg_name = entry["item"].asset_package
        if not self._is_pkg_swapped(pkg_name):
            return False
        meta = self.app_state.get("active_swaps", {}).get(pkg_name.lower())
        if meta and meta.get("target_asset_path"):
            return meta["target_asset_path"].lower() == entry["item"].asset_path.lower()
        return True

    def _display_for_have(self, entry: dict) -> str:
        if self._is_entry_swapped(entry):
            return f"{entry['display']}  •"
        return entry["display"]

    def _populate_item_lists(self, restore_want=None, restore_have=None) -> None:
        items = self.categorized_items.get(self.current_slot, [])
        qw = self.want_search_var.get().strip().lower()
        qh = self.have_search_var.get().strip().lower()

        if self.current_slot == "Decal" and self.decal_filter != "All":
            df = self.decal_filter
            if df == "Universal":
                want_pool = [e for e in items if e.get("decal_car") == "Universal"]
                have_pool = items
            else:
                # Car filter: compatible donors = same car + Universal; equipped targets = same car
                want_pool = [e for e in items if e.get("decal_car") in (df, "Universal")]
                have_pool = [e for e in items if e.get("decal_car") == df]
        else:
            want_pool = items
            have_pool = items

        self.want_visible = [e for e in want_pool if qw in e["display"].lower()]
        self.have_visible = [e for e in have_pool if qh in e["display"].lower()]

        self.want_lb.delete(0, "end")
        for idx, e in enumerate(self.want_visible):
            self.want_lb.insert("end", e["display"])
            if restore_want and e["display"] == restore_want:
                self.want_lb.selection_set(idx)

        self.have_lb.delete(0, "end")
        for idx, e in enumerate(self.have_visible):
            disp = self._display_for_have(e)
            self.have_lb.insert("end", disp)
            if restore_have and (disp == restore_have or e["display"] == restore_have.rstrip(" •")):
                self.have_lb.selection_set(idx)

    def _on_item_filter(self) -> None:
        if self._suppress_trace or self.current_mode != "items":
            return
        cur_w = self.want_visible[self.want_lb.curselection()[0]]["display"] if self.want_lb.curselection() else None
        cur_h = self.have_visible[self.have_lb.curselection()[0]]["display"] if self.have_lb.curselection() else None
        self._populate_item_lists(restore_want=cur_w, restore_have=cur_h)
        self._update_selection_summary()

    def _update_selection_summary(self) -> None:
        if self.current_mode == "items":
            sw, sh = self.want_lb.curselection(), self.have_lb.curselection()
            if sw and sh:
                donor_entry = self.want_visible[sw[0]]
                donor = donor_entry["product_name"]
                target_entry = self.have_visible[sh[0]]
                target = target_entry["product_name"]
                suffix = "  (replaces active swap via backup)" if self._is_pkg_swapped(target_entry["item"].asset_package) else ""
                mesh_note = ""
                if self.current_slot == "Decal":
                    dc = donor_entry.get("decal_car")
                    tc = target_entry.get("decal_car")
                    if dc and dc != "Universal" and tc and dc != tc:
                        mesh_note = f"  ·  note: {dc} UV mesh decal"
                self.set_status(f"{donor}  →  {target}{suffix}{mesh_note}", subtle=False)
            elif sw:
                self.set_status(f"{self.want_visible[sw[0]]['product_name']}  →  select equipped item", subtle=True)
            else:
                self.set_status("Select items to swap", subtle=True)
        elif self.current_mode == "custom_pfp":
            pkg = self.pfp_target_var.get()
            if pkg and self.cropped_pfp_path:
                suffix = "  (replaces active swap via backup)" if self._is_pkg_swapped(pkg) else ""
                self.set_status(f"Custom image  →  {self.pfp_name_lbl.cget('text')}{suffix}", subtle=False)
            else:
                self.set_status("Select an avatar border and choose an image", subtle=True)
        elif self.current_mode == "custom_banner":
            pkg = self.banner_target_var.get()
            if pkg and self.cropped_banner_path:
                suffix = "  (replaces active swap via backup)" if self._is_pkg_swapped(pkg) else ""
                self.set_status(f"Custom banner  →  {self.banner_name_lbl.cget('text')}{suffix}", subtle=False)
            else:
                self.set_status("Select a banner and choose an image", subtle=True)

    # ── Primary Actions ──────────────────────────────────────────────────────
    def _on_primary_apply(self) -> None:
        if self.current_mode == "items":
            self.do_swap(in_game=True)
        elif self.current_mode == "custom_pfp":
            self._apply_pfp()
        elif self.current_mode == "custom_banner":
            self._apply_banner()

    def do_swap(self, in_game: bool = True) -> None:
        sw, sh = self.want_lb.curselection(), self.have_lb.curselection()
        if not sw or not sh:
            self.set_status("Select both a replacement item and an equipped item", color=TEXT_DANGER)
            return

        donor_item = self.want_visible[sw[0]]["item"]
        target_item = self.have_visible[sh[0]]["item"]

        if in_game:
            out_dir = Path(self.folder)
            btn = self.btn_apply
            orig_text = "Apply"
        else:
            selected_dir = filedialog.askdirectory(title="Select export destination")
            if not selected_dir:
                return
            out_dir = Path(selected_dir)
            btn = self.btn_export
            orig_text = "Export"

        btn.configure(text="Working...", state="disabled")
        self.set_status(f"Applying {donor_item.product}  →  {target_item.product}...", subtle=False)
        threading.Thread(
            target=self._run_item_swap,
            args=(donor_item, target_item, out_dir, in_game, btn, orig_text),
            daemon=True,
        ).start()

    def _run_item_swap(self, donor_item, target_item, out_dir: Path, in_game: bool, btn, orig_text: str) -> None:
        try:
            rl_asset_swapper._keys_map = self.keys_map
            opts = SwapOptions(
                items_path=Path(get_resource_path("items.json")),
                keys_path=self.keys_path,
                donor_dir=Path(self.folder),
                output_dir=out_dir,
                key_source_dir=Path(self.folder),
                include_thumbnails=False,
                preserve_header_offsets=True,
                overwrite=True,
                logger=None,
            )
            rl_asset_swapper.swap_asset(rl_upk_editor, target_item, donor_item, opts)
            if in_game:
                self.record_swap(
                    target_item.asset_package,
                    donor_item.product,
                    target_item.product,
                    mode="IN-GAME",
                    target_asset_path=target_item.asset_path,
                )
                self.clear_rl_cache()

            def on_done():
                btn.configure(text="Applied" if in_game else "Exported", state="normal")
                self._on_item_filter()
                msg = (
                    f"Applied  {donor_item.product}  →  {target_item.product}"
                    if in_game else f"Exported {target_item.asset_package}"
                )
                self.set_status(msg, color=TEXT_SUCCESS)
                self.after(1600, lambda: btn.configure(text=orig_text))

            self.after(0, on_done)
        except Exception as e:
            def on_err(err=str(e)):
                btn.configure(text=orig_text, state="normal")
                self.set_status(f"Swap failed: {err}", color=TEXT_DANGER)
                messagebox.showerror("Swap Error", err)
            self.after(0, on_err)

    # ── Custom Profile Picture & Banner ──────────────────────────────────────
    def filter_pfps(self, *_args) -> None:
        q = self.pfp_search_var.get().strip().lower()
        self.pfp_listbox.delete(0, tk.END)
        self.pfp_visible = []
        for p in self.all_pfps:
            name = p.get("Product", "Unknown")
            pkg = p.get("AssetPackage", "")
            if q in name.lower() or q in pkg.lower():
                mark = "  •" if self._is_pkg_swapped(pkg) else ""
                self.pfp_listbox.insert(tk.END, f"{name}{mark}")
                self.pfp_visible.append((name, pkg))

    def on_pfp_select(self, _event=None) -> None:
        sel = self.pfp_listbox.curselection()
        if not sel:
            return
        name, pkg = self.pfp_visible[sel[0]]
        self.pfp_target_var.set(pkg)
        self.pfp_name_lbl.configure(text=name, text_color=TEXT_PRIMARY)
        self._update_selection_summary()

    def _choose_pfp_image(self) -> None:
        p = filedialog.askopenfilename(filetypes=[("Images", "*.png;*.jpg;*.jpeg;*.webp;*.bmp")])
        if not p:
            return
        def on_cropped(cropped_path):
            self.cropped_pfp_path = cropped_path
            prev = Image.open(cropped_path).resize((120, 120), Image.LANCZOS)
            self.pfp_preview_img = ImageTk.PhotoImage(prev)
            self.lbl_pfp_preview.configure(image=self.pfp_preview_img, text="")
            self._update_selection_summary()
        CropWindow(self, p, on_cropped, ratio_w=1, ratio_h=1)

    def _apply_pfp(self) -> None:
        if not self.cropped_pfp_path:
            self.set_status("Choose an image first", color=TEXT_DANGER)
            return
        target_pkg = self.pfp_target_var.get()
        if not target_pkg:
            self.set_status("Select an avatar border on the left first", color=TEXT_DANGER)
            return

        self.btn_apply.configure(text="Working...", state="disabled")
        border_name = self.pfp_name_lbl.cget("text")

        def worker():
            try:
                rl_asset_swapper._keys_map = self.keys_map
                opts = SwapOptions(
                    items_path=Path(get_resource_path("items.json")),
                    keys_path=self.keys_path,
                    donor_dir=Path(self.folder),
                    output_dir=Path(self.folder),
                    key_source_dir=Path(self.folder),
                    include_thumbnails=False,
                    preserve_header_offsets=True,
                    overwrite=True,
                )
                rl_asset_swapper.swap_pfp_from_png(rl_upk_editor, Path(self.cropped_pfp_path), target_pkg, opts)
                self.record_swap(target_pkg, "Custom PFP", border_name, mode="IN-GAME")
                self.clear_rl_cache()

                def done():
                    self.btn_apply.configure(text="Applied", state="normal")
                    self.filter_pfps()
                    self.set_status(f"Applied custom profile picture to {border_name}", color=TEXT_SUCCESS)
                    self.after(1600, lambda: self.btn_apply.configure(text="Apply"))
                self.after(0, done)
            except Exception as e:
                def err(msg=str(e)):
                    self.btn_apply.configure(text="Apply", state="normal")
                    self.set_status(f"Failed: {msg}", color=TEXT_DANGER)
                    messagebox.showerror("Profile Picture Error", msg)
                self.after(0, err)

        threading.Thread(target=worker, daemon=True).start()

    def filter_banners(self, *_args) -> None:
        q = self.banner_search_var.get().strip().lower()
        self.banner_listbox.delete(0, tk.END)
        self.banner_visible = []
        for b in self.all_banners:
            name = b.get("Product", "Unknown")
            pkg = b.get("AssetPackage", "")
            if q in name.lower() or q in pkg.lower():
                mark = "  •" if self._is_pkg_swapped(pkg) else ""
                self.banner_listbox.insert(tk.END, f"{name}{mark}")
                self.banner_visible.append((name, pkg))

    def on_banner_select(self, _event=None) -> None:
        sel = self.banner_listbox.curselection()
        if not sel:
            return
        name, pkg = self.banner_visible[sel[0]]
        self.banner_target_var.set(pkg)
        self.banner_name_lbl.configure(text=name, text_color=TEXT_PRIMARY)
        self._update_selection_summary()

    def _choose_banner_image(self) -> None:
        p = filedialog.askopenfilename(filetypes=[("Images", "*.png;*.jpg;*.jpeg;*.webp;*.bmp")])
        if not p:
            return
        def on_cropped(cropped_path):
            self.cropped_banner_path = cropped_path
            prev = Image.open(cropped_path).resize((240, 56), Image.LANCZOS)
            self.banner_preview_img = ImageTk.PhotoImage(prev)
            self.lbl_banner_preview.configure(image=self.banner_preview_img, text="")
            self._update_selection_summary()
        CropWindow(self, p, on_cropped, ratio_w=14, ratio_h=3)

    def _apply_banner(self) -> None:
        if not self.cropped_banner_path:
            self.set_status("Choose an image first", color=TEXT_DANGER)
            return
        target_pkg = self.banner_target_var.get()
        banner_name = self.banner_name_lbl.cget("text")
        self.btn_apply.configure(text="Working...", state="disabled")

        def worker():
            try:
                rl_asset_swapper._keys_map = self.keys_map
                opts = SwapOptions(
                    items_path=Path(get_resource_path("items.json")),
                    keys_path=self.keys_path,
                    donor_dir=Path(self.folder),
                    output_dir=Path(self.folder),
                    key_source_dir=Path(self.folder),
                    include_thumbnails=False,
                    preserve_header_offsets=True,
                    overwrite=True,
                )
                rl_asset_swapper.swap_banner_from_png(rl_upk_editor, Path(self.cropped_banner_path), target_pkg, opts)
                self.record_swap(target_pkg, "Custom Banner", banner_name, mode="IN-GAME")
                self.clear_rl_cache()

                def done():
                    self.btn_apply.configure(text="Applied", state="normal")
                    self.filter_banners()
                    self.set_status(f"Applied custom banner to {banner_name}", color=TEXT_SUCCESS)
                    self.after(1600, lambda: self.btn_apply.configure(text="Apply"))
                self.after(0, done)
            except Exception as e:
                def err(msg=str(e)):
                    self.btn_apply.configure(text="Apply", state="normal")
                    self.set_status(f"Failed: {msg}", color=TEXT_DANGER)
                    messagebox.showerror("Banner Error", msg)
                self.after(0, err)

        threading.Thread(target=worker, daemon=True).start()

    def _bulk_pfp(self) -> None:
        folder = filedialog.askdirectory(title="Select Folder of Profile Picture Images")
        if not folder:
            return
        images = self._collect_images(Path(folder))
        if not images:
            messagebox.showwarning("No Images", "No supported image files found in that folder.")
            return
        borders = [
            b.name for b in Path(self.folder).glob("AvatarBorder_*_SF.upk")
            if b.name.lower() != "avatarborder_default_sf.upk"
        ]
        self._run_bulk_image_job("Randomizing Avatar Borders", borders, images, mode="pfp")

    def _bulk_banner(self) -> None:
        folder = filedialog.askdirectory(title="Select Folder of Banner Images")
        if not folder:
            return
        images = self._collect_images(Path(folder))
        if not images:
            messagebox.showwarning("No Images", "No supported image files found in that folder.")
            return
        banners = [b.name for b in Path(self.folder).glob("*Banner*_SF.upk")]
        self._run_bulk_image_job("Randomizing Player Banners", banners, images, mode="banner")

    @staticmethod
    def _collect_images(folder_path: Path) -> list:
        found = []
        for ext in (".png", ".jpg", ".jpeg", ".bmp", ".webp"):
            found.extend(folder_path.glob(f"*{ext}"))
            found.extend(folder_path.glob(f"*{ext.upper()}"))
        return list({p.resolve(): p for p in found}.values())

    def _run_bulk_image_job(self, title: str, packages: list, images: list, mode: str) -> None:
        if not packages:
            messagebox.showwarning("Not Found", "No matching packages found in CookedPCConsole.")
            return

        prog = ctk.CTkToplevel(self)
        prog.title(title)
        prog.geometry("380x140")
        prog.configure(fg_color=OLED_BLACK)
        prog.grab_set()
        prog.after(50, lambda: apply_oled_titlebar(prog))

        lbl = ctk.CTkLabel(prog, text="Preparing...", text_color=TEXT_SECONDARY, font=ctk.CTkFont(family=FONT_FAMILY, size=12))
        lbl.pack(pady=(28, 12))
        pb = ctk.CTkProgressBar(prog, width=300, height=6, fg_color=OLED_SELECTED, progress_color=CTA_BG)
        pb.pack()
        pb.set(0)

        def worker():
            rl_asset_swapper._keys_map = self.keys_map
            opts = SwapOptions(
                items_path=Path(get_resource_path("items.json")),
                keys_path=self.keys_path,
                donor_dir=Path(self.folder),
                output_dir=Path(self.folder),
                key_source_dir=Path(self.folder),
                include_thumbnails=False,
                preserve_header_offsets=True,
                overwrite=True,
            )
            total = len(packages)
            success = 0
            tmp = Path(tempfile.gettempdir()) / f"rlswap_bulk_{os.urandom(4).hex()}.png"

            for idx, pkg in enumerate(packages):
                self.after(0, lambda i=idx, p=pkg: (lbl.configure(text=f"{i + 1} / {total}  ·  {p}"), pb.set((i + 1) / total)))
                try:
                    img = Image.open(str(random.choice(images))).convert("RGBA")
                    w, h = img.size
                    if mode == "pfp":
                        m = min(w, h)
                        img = img.crop(((w - m) // 2, (h - m) // 2, (w - m) // 2 + m, (h - m) // 2 + m))
                        img.save(tmp)
                        rl_asset_swapper.swap_pfp_from_png(rl_upk_editor, tmp, pkg, opts)
                        self.record_swap(pkg, "Bulk PFP", pkg, mode="IN-GAME")
                    else:
                        target_ratio = 14 / 3
                        ir = w / h
                        if ir > target_ratio:
                            nw = int(h * target_ratio)
                            img = img.crop(((w - nw) // 2, 0, (w - nw) // 2 + nw, h))
                        elif ir < target_ratio:
                            nh = int(w / target_ratio)
                            img = img.crop((0, (h - nh) // 2, w, (h - nh) // 2 + nh))
                        img.save(tmp)
                        rl_asset_swapper.swap_banner_from_png(rl_upk_editor, tmp, pkg, opts)
                        self.record_swap(pkg, "Bulk Banner", pkg, mode="IN-GAME")
                    success += 1
                except Exception:
                    pass

            if tmp.exists():
                try:
                    tmp.unlink()
                except Exception:
                    pass

            self.clear_rl_cache()

            def finish():
                prog.destroy()
                if mode == "pfp":
                    self.filter_pfps()
                else:
                    self.filter_banners()
                self.set_status(f"Randomized {success} of {total} packages", color=TEXT_SUCCESS)

            self.after(0, finish)

        threading.Thread(target=worker, daemon=True).start()

    # ── Folder, Cache & Reset Backups ────────────────────────────────────────
    def clear_rl_cache(self) -> bool:
        cache_paths = [
            os.path.join(os.path.expanduser("~"), "Documents", "My Games", "Rocket League", "TAGame", "Cache"),
            os.path.join(os.environ.get("USERPROFILE", ""), "Documents", "My Games", "Rocket League", "TAGame", "Cache"),
        ]
        cleared = False
        for cache_dir in cache_paths:
            if os.path.isdir(cache_dir):
                try:
                    for f in os.listdir(cache_dir):
                        fp = os.path.join(cache_dir, f)
                        if os.path.isfile(fp):
                            os.remove(fp)
                        elif os.path.isdir(fp):
                            shutil.rmtree(fp, ignore_errors=True)
                    cleared = True
                except Exception:
                    pass
        return cleared

    def set_in_dir(self) -> None:
        folder = filedialog.askdirectory(initialdir=self.folder, title="Select CookedPCConsole Directory")
        if folder:
            self.folder = folder
            self._save_state()
            undone = self.sync_game_update_silently()
            self._load_categorized_items()
            self._load_custom_catalogs()
            if self.current_mode == "items":
                self._on_item_filter()
            self.filter_pfps()
            self.filter_banners()
            if undone:
                self.set_status("Folder updated — game patch detected & swaps reset", subtle=False)
            else:
                self.set_status(f"Game folder set to {self.folder}", subtle=False)

    def restore_backups(self) -> None:
        folder_path = Path(self.folder)
        bak_files = sorted(folder_path.glob("*.bak"), key=lambda p: p.name.lower()) if folder_path.exists() else []
        if not bak_files:
            self.set_status("No active swaps to reset", subtle=True)
            return

        win = ctk.CTkToplevel(self)
        win.title("Reset Swaps")
        win.geometry("460x520")
        win.configure(fg_color=OLED_BLACK)
        win.grab_set()
        win.after(50, lambda: apply_oled_titlebar(win))

        ctk.CTkLabel(
            win, text="Active Swaps", anchor="w", text_color=TEXT_PRIMARY,
            font=ctk.CTkFont(family=FONT_FAMILY, size=15, weight="bold")
        ).pack(fill="x", padx=24, pady=(22, 4))

        ctk.CTkLabel(
            win, text="Select modified packages to restore to original game files.",
            anchor="w", text_color=TEXT_MUTED, font=ctk.CTkFont(family=FONT_FAMILY, size=12)
        ).pack(fill="x", padx=24, pady=(0, 12))

        scroll = ctk.CTkScrollableFrame(
            win, fg_color=OLED_ELEVATED, corner_radius=12,
            border_width=1, border_color=OLED_BORDER,
            scrollbar_button_color=OLED_BORDER, scrollbar_button_hover_color=OLED_BORDER_FOCUS
        )
        scroll.pack(fill="both", expand=True, padx=24, pady=4)

        active_map = self.app_state.get("active_swaps", {})
        check_vars = []
        for bak in bak_files:
            pkg_name = bak.name[:-4]
            meta = active_map.get(pkg_name.lower())
            label = f"{meta['target']}  ←  {meta['donor']}" if meta else pkg_name
            var = tk.BooleanVar(value=True)
            chk = ctk.CTkCheckBox(
                scroll, text=label, variable=var,
                text_color=TEXT_PRIMARY, fg_color=CTA_BG, hover_color=CTA_HOVER, checkmark_color=CTA_TEXT,
                border_color=OLED_BORDER_FOCUS, checkbox_width=18, checkbox_height=18, corner_radius=5,
                font=ctk.CTkFont(family=FONT_FAMILY, size=12)
            )
            chk.pack(anchor="w", padx=10, pady=6)
            check_vars.append((bak, var))

        def do_restore(targets):
            if not targets:
                return
            restored = 0
            for bak in targets:
                upk_path = bak.with_suffix("")
                try:
                    if upk_path.exists():
                        upk_path.unlink()
                    shutil.copy2(bak, upk_path)
                    bak.unlink()
                    self.app_state.get("active_swaps", {}).pop(upk_path.name.lower(), None)
                    restored += 1
                except Exception:
                    pass

            temp_dir = Path(self.app_dir) / "AssetSwapper_Decrypted"
            if temp_dir.exists():
                shutil.rmtree(temp_dir, ignore_errors=True)

            self._save_state()
            self._rewrite_swaps_log()
            self.clear_rl_cache()
            if self.current_mode == "items":
                self._on_item_filter()
            self.filter_pfps()
            self.filter_banners()
            self.set_status(f"Restored {restored} package(s) to original", color=TEXT_SUCCESS)
            win.destroy()

        btn_row = ctk.CTkFrame(win, fg_color="transparent")
        btn_row.pack(fill="x", padx=24, pady=20)

        ctk.CTkButton(
            btn_row, text="Cancel", width=90, height=34, corner_radius=8,
            fg_color=OLED_ELEVATED, hover_color=OLED_SELECTED, text_color=TEXT_SECONDARY,
            border_width=1, border_color=OLED_BORDER,
            font=ctk.CTkFont(family=FONT_FAMILY, size=12),
            command=win.destroy
        ).pack(side="left")

        ctk.CTkButton(
            btn_row, text="Reset All", width=96, height=34, corner_radius=8,
            fg_color=OLED_SELECTED, hover_color=OLED_BORDER_FOCUS, text_color=TEXT_DANGER,
            font=ctk.CTkFont(family=FONT_FAMILY, size=12, weight="bold"),
            command=lambda: do_restore([b for b, _ in check_vars])
        ).pack(side="right")

        ctk.CTkButton(
            btn_row, text="Restore Selected", width=130, height=34, corner_radius=8,
            fg_color=CTA_BG, hover_color=CTA_HOVER, text_color=CTA_TEXT,
            font=ctk.CTkFont(family=FONT_FAMILY, size=12, weight="bold"),
            command=lambda: do_restore([b for b, v in check_vars if v.get()])
        ).pack(side="right", padx=(0, 8))


if __name__ == "__main__":
    app = RLSwapperApp()
    app.mainloop()
