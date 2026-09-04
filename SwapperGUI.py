import tkinter as tk
from tkinter import filedialog, messagebox
import customtkinter as ctk

from tkinter import messagebox, filedialog
import json
import sys
import os
import shutil
import re
from pathlib import Path
import threading

import rl_asset_swapper
from rl_asset_swapper import SwapOptions, load_items
import rl_upk_editor

ctk.set_appearance_mode("Dark")
ctk.set_default_color_theme("blue")

DARK_BG = "#1A1A1A"
PANEL_BG = "#222222"
ACCENT = "#3B82F6"

HITBOX_MAP = {
    'Octane': ['Octane', 'Octane ZSR', 'Fennec', 'Takumi', 'Takumi RX-T', 'Twinzer', 'Bone Shaker', 'Marauder', 'Scarab', 'Zippy', 'Armadillo', 'Grog', 'Triton', 'Proteus', 'Vulcan', 'Fast 4WD', 'Mudcat', 'Mudcat GXT', 'Harbinger', 'Harbinger GXT', 'Jackal', 'Dingo', 'Outlaw', 'Outlaw GXT', 'Nomad', 'Nomad GXT', 'Aston Martin Valhalla', 'BMW M240i', 'Bugatti Centodieci', 'Ford Bronco Raptor', 'Ford F-150 RLE', 'Ford Mustang Mach-E RLE', 'Honda Civic Type R', 'Honda Civic Type R-LE', 'Jurassic Jeep Wrangler', 'Maestro', 'Nissan Silvia', 'Nissan Silvia RLE', 'Primo', 'Redline', 'Sweet Tooth', 'Volkswagen Golf GTI', 'Volkswagen Golf GTI RLE', 'Admiral', 'Mako'],
    'Dominus': ['Dominus', 'Dominus GT', 'Ice Charger', 'Aftershock', 'Masamune', 'Ripper', 'DeLorean Time Machine', 'Batmobile (1989)', 'Fast & Furious Dodge Charger', 'Fast & Furious Nissan Skyline', 'Ecto-1', 'K.I.T.T.', 'McLaren 570S', 'Gazella GT', 'MR11', 'Nemesis', 'Diestro', 'Ronin', 'Ronin GXT', 'Peregrine TT', 'Tyranno', 'Tyranno GXT', 'Mamba', 'Nissan Z Performance', '007''s Aston Martin DBS', 'Aston Martin DB5', 'Audi RS 3', 'BMW M4 CSL', 'Bugatti Bolide', 'Chikara', 'Chikara GXT', 'Emperor', 'Emperor II', 'Ferrari 296 GTB', 'Ford Mustang Shelby GT500', 'Formula 1', 'Guardian', 'Guardian GXT', 'Lamborghini Countach LPI 800-4', 'Lamborghini Huracan STO', 'McLaren 765LT', 'Porsche 911 Turbo', 'Porsche 911 Turbo RLE', 'Samus'' Gunship', 'Maserati Grecale Trofeo', 'Nissan Fairlady Z', 'Fairlady'],
    'Breakout': ['Breakout', 'Breakout Type-S', 'Animus GP', 'Cyclone', 'Samurai', 'Komodo', 'Nexus', 'Nexus SC'],
    'Plank': ['Batmobile (2016)', 'Mantis', 'Paladin', 'Centio', 'Centio V17', 'Artemis', 'Artemis GXT', 'Sentinel'],
    'Hybrid': ['Endo', 'Venom', 'X-Devil', 'X-Devil Mk2', 'Jager 619', 'Jager 619 RS', 'Nimbus', 'Tygris', 'Insidio', 'R3MX', 'R3MX GXT', 'Esper', 'Silvia'],
    'Merc': ['Merc', 'Battle Bus', 'Ford Bronco', 'Nomad']
}

def get_hitbox(name):
    for k, v in HITBOX_MAP.items():
        for car in v:
            if car.lower() == name.lower() or car.lower() in name.lower():
                return k
    return "Unknown"

def get_resource_path(relative_path):
    try:
        base_path = sys._MEIPASS
    except Exception:
        base_path = os.path.abspath(".")
    return os.path.join(base_path, relative_path)

def can_fit(visual_dict, sacrifice_dict):
    v_parts = visual_dict["parts"]
    s_parts = sacrifice_dict["parts"]
    
    is_body = (visual_dict["item"].slot == "Body" and sacrifice_dict["item"].slot == "Body")
    if is_body:
        if visual_dict.get("hitbox") != sacrifice_dict.get("hitbox"):
            return False
            
    # STRIKEST RESTRICTIONS FOR ALL ITEMS (INCLUDING BODIES)
    if len(sacrifice_dict["stem"]) > len(visual_dict["stem"]): return False
    if len(s_parts) == len(v_parts):
        for s, v in zip(s_parts, v_parts):
            if len(s) > len(v): return False
    else:
        if s_parts and v_parts:
            if len(s_parts[0]) > len(v_parts[0]): return False
            if len(s_parts[-1]) > len(v_parts[-1]): return False
        for s, v in zip(s_parts, v_parts):
            if len(s) > len(v): return False
            
    return True


class CropWindow(ctk.CTkToplevel):
    def __init__(self, parent, image_path, callback, ratio_w=1, ratio_h=1):
        super().__init__(parent)
        self.title("Crop Image")
        self.geometry("800x650")
        self.callback = callback
        
        from PIL import Image, ImageTk
        self.original_img = Image.open(image_path).convert("RGBA")
        
        max_size = 550
        w, h = self.original_img.size
        ratio = min(max_size / w, max_size / h)
        self.disp_w = int(w * ratio)
        self.disp_h = int(h * ratio)
        self.ratio = ratio
        
        self.disp_img = self.original_img.resize((self.disp_w, self.disp_h), Image.LANCZOS)
        self.tk_img = ImageTk.PhotoImage(self.disp_img)
        
        lbl = ctk.CTkLabel(self, text="Drag the box to move, drag the BOTTOM-RIGHT corner to resize:", font=ctk.CTkFont(size=16))
        lbl.pack(pady=(10, 5))
        
        import tkinter as tk
        self.canvas = tk.Canvas(self, width=self.disp_w, height=self.disp_h, bg="gray20", highlightthickness=0)
        self.canvas.pack(pady=10)
        self.canvas.create_image(0, 0, anchor="nw", image=self.tk_img)
        
        self.crop_ratio_w = ratio_w
        self.crop_ratio_h = ratio_h
        
        max_crop_w = self.disp_w
        max_crop_h = int(max_crop_w * (self.crop_ratio_h / self.crop_ratio_w))
        if max_crop_h > self.disp_h:
            max_crop_h = self.disp_h
            max_crop_w = int(max_crop_h * (self.crop_ratio_w / self.crop_ratio_h))
            
        self.crop_w = max_crop_w // 1.5
        self.crop_h = max_crop_h // 1.5
        
        self.crop_x = (self.disp_w - self.crop_w) // 2
        self.crop_y = (self.disp_h - self.crop_h) // 2
        
        self.rect = self.canvas.create_rectangle(
            self.crop_x, self.crop_y, self.crop_x + self.crop_w, self.crop_y + self.crop_h,
            outline="#10B981", width=3, dash=(4, 4)
        )
        
        # Resize handle
        self.handle = self.canvas.create_rectangle(
            self.crop_x + self.crop_w - 10, self.crop_y + self.crop_h - 10,
            self.crop_x + self.crop_w + 10, self.crop_y + self.crop_h + 10,
            fill="#10B981", outline="#FFF"
        )
        
        self.canvas.bind("<ButtonPress-1>", self.on_press)
        self.canvas.bind("<B1-Motion>", self.on_drag)
        self.canvas.bind("<Motion>", self.on_hover)
        
        btn_frame = ctk.CTkFrame(self, fg_color="transparent")
        btn_frame.pack(pady=10)
        ctk.CTkButton(btn_frame, text="Crop & Use", fg_color="#10B981", hover_color="#059669", font=ctk.CTkFont(size=14, weight="bold"), command=self.crop_and_close).pack(side="left", padx=10)
        ctk.CTkButton(btn_frame, text="Cancel", fg_color="#EF4444", hover_color="#B91C1C", font=ctk.CTkFont(size=14, weight="bold"), command=self.destroy).pack(side="left", padx=10)
        
        self.start_x = 0
        self.start_y = 0
        self.mode = "move"
        self.grab_set()
        self.focus_set()
        
    def on_hover(self, event):
        hx1, hy1, hx2, hy2 = self.canvas.coords(self.handle)
        if hx1 <= event.x <= hx2 and hy1 <= event.y <= hy2:
            self.canvas.config(cursor="sizing")
        else:
            self.canvas.config(cursor="arrow")

    def on_press(self, event):
        self.start_x = event.x
        self.start_y = event.y
        hx1, hy1, hx2, hy2 = self.canvas.coords(self.handle)
        if hx1 <= event.x <= hx2 and hy1 <= event.y <= hy2:
            self.mode = "resize"
        else:
            self.mode = "move"

    def on_drag(self, event):
        dx = event.x - self.start_x
        dy = event.y - self.start_y
        
        if self.mode == "move":
            new_x = self.crop_x + dx
            new_y = self.crop_y + dy
            if new_x < 0: new_x = 0
            if new_y < 0: new_y = 0
            if new_x + self.crop_w > self.disp_w: new_x = self.disp_w - self.crop_w
            if new_y + self.crop_h > self.disp_h: new_y = self.disp_h - self.crop_h
            self.crop_x = new_x
            self.crop_y = new_y
        elif self.mode == "resize":
            # Force aspect ratio on dx/dy
            # dy = dx * (ratio_h / ratio_w)
            new_w = self.crop_w + dx
            if new_w < 50: new_w = 50
            new_h = int(new_w * (self.crop_ratio_h / self.crop_ratio_w))
            
            if self.crop_x + new_w > self.disp_w:
                new_w = self.disp_w - self.crop_x
                new_h = int(new_w * (self.crop_ratio_h / self.crop_ratio_w))
                
            if self.crop_y + new_h > self.disp_h:
                new_h = self.disp_h - self.crop_y
                new_w = int(new_h * (self.crop_ratio_w / self.crop_ratio_h))
                
            self.crop_w = new_w
            self.crop_h = new_h
            
        self.start_x = event.x
        self.start_y = event.y
        
        self.canvas.coords(self.rect, self.crop_x, self.crop_y, self.crop_x + self.crop_w, self.crop_y + self.crop_h)
        self.canvas.coords(self.handle, self.crop_x + self.crop_w - 10, self.crop_y + self.crop_h - 10, self.crop_x + self.crop_w + 10, self.crop_y + self.crop_h + 10)
        
    def crop_and_close(self):
        orig_x = int(self.crop_x / self.ratio)
        orig_y = int(self.crop_y / self.ratio)
        orig_w = int(self.crop_w / self.ratio)
        orig_h = int(self.crop_h / self.ratio)
        
        cropped = self.original_img.crop((orig_x, orig_y, orig_x + orig_w, orig_y + orig_h))
        import os
        import tempfile
        out_path = os.path.join(tempfile.gettempdir(), f"cropped_{os.urandom(4).hex()}.png")
        cropped.save(out_path)
        
        self.callback(out_path)
        self.destroy()

class RLSwapperApp(ctk.CTk):
    @property
    def swaps_log(self):
        if os.path.exists(self.folder):
            return os.path.join(self.folder, 'swaps_log.txt')
        return os.path.join(self.appdata_dir, 'swaps_log.txt')

    def __init__(self):
        super().__init__()
        self.title("RL Swapper")
        self.geometry("1100x750")
        self.configure(fg_color=DARK_BG)
        
        if getattr(sys, 'frozen', False):
            self.app_dir = os.path.dirname(sys.executable)
        else:
            self.app_dir = os.path.dirname(os.path.abspath(__file__))
            
        self.appdata_dir = os.path.join(os.environ.get('LOCALAPPDATA', self.app_dir), 'RLSwapper')
        os.makedirs(self.appdata_dir, exist_ok=True)
        # swaps_log is now a property below
        self.output_dir = self.app_dir  
        
        self.folder = r'C:\Program Files\Epic Games\rocketleague\TAGame\CookedPCConsole'
        if not os.path.exists(self.folder):
            self.folder = r'C:\Program Files (x86)\Steam\steamapps\common\rocketleague\TAGame\CookedPCConsole'
        
        try:
            self.items = load_items(Path(get_resource_path('items.json')))
            self.keys_path = Path(get_resource_path('keys.txt'))
        except Exception as e:
            messagebox.showerror("Error", f"Could not load items DB: {e}")
            self.destroy()
            return
            
        self.keys_map = {}
        try:
            map_path = Path(get_resource_path('keys_map.json'))
            if map_path.exists():
                with open(map_path, 'r', encoding='utf-8') as f:
                    self.keys_map = json.load(f)
        except:
            pass

        if not os.path.exists(self.swaps_log):
            with open(self.swaps_log, "w", encoding="utf-8") as f:
                f.write("# RL Swaps Log\n\n")

        self.categorized_items = {}
        ignored_slots = {'Currency', 'Crate', 'Blueprint', 'Player Title', 'Player Anthem', 'Esports Team'}
        seen_keys = {}
        
        for itm in self.items:
            if not itm.slot or itm.slot in ignored_slots: continue
            
            stem = itm.asset_package.lower().replace(".upk", "")
            if stem not in self.keys_map and stem.replace("_sf", "") not in self.keys_map:
                continue
                
            prod_name = str(itm.product)
            if "Team " in prod_name or "Esports" in prod_name or re.search(r'\b20\d\d\b', prod_name):
                continue
            
            if itm.slot not in self.categorized_items: 
                self.categorized_items[itm.slot] = []
                seen_keys[itm.slot] = set()
                
            dedup_key = (prod_name, itm.slot)
            if dedup_key in seen_keys[itm.slot]:
                continue
            seen_keys[itm.slot].add(dedup_key)
            
            parts = [p for p in str(itm.asset_path).split(".") if p]
            stem_bare = itm.asset_package.lower().replace("_sf.upk", "").replace(".upk", "")
            
            hitbox = None
            display_name = prod_name
            if itm.slot == "Body":
                hitbox = get_hitbox(prod_name)
                display_name = f"{prod_name} [{hitbox}]"
            
            self.categorized_items[itm.slot].append({
                "display": display_name,
                "product_name": prod_name,
                "item": itm,
                "parts": parts,
                "stem": stem_bare,
                "hitbox": hitbox
            })
            
        for slot in self.categorized_items:
            self.categorized_items[slot] = sorted(self.categorized_items[slot], key=lambda x: x["display"])

        self.header_frame = ctk.CTkFrame(self, fg_color="transparent")
        self.header_frame.pack(fill="x", padx=20, pady=20)
        
        self.in_dir_btn = ctk.CTkButton(self.header_frame, text="📁 Set RL Folder", fg_color=PANEL_BG, text_color="#E0E0E0", hover_color="#333333", border_width=1, border_color="#555", command=self.set_in_dir)
        self.in_dir_btn.pack(side="left", padx=(0, 10))

        self.restore_btn = ctk.CTkButton(self.header_frame, text="🔄 Restore All Backups", fg_color="#C0392B", hover_color="#922B21", text_color="#FFF", font=ctk.CTkFont(size=13, weight="bold"), command=self.restore_backups)
        self.restore_btn.pack(side="left", padx=(0, 10))
        
        self.title_label = ctk.CTkLabel(self.header_frame, text="RL SWAPPER", text_color="#E0E0E0", font=ctk.CTkFont(family="Segoe UI", size=28, weight="bold"))
        self.title_label.pack(side="left", expand=True)
        
        self.logs_btn = ctk.CTkButton(self.header_frame, text="Logs", fg_color=PANEL_BG, text_color="#E0E0E0", hover_color="#333333", border_width=1, border_color="#555", command=self.show_logs)
        self.logs_btn.pack(side="right")
        
        popular_order = ["Body", "Decal", "Wheels", "Rocket Boost", "Goal Explosion", "Trail", "Antenna", "Topper"]
        sorted_slots = [s for s in popular_order if s in self.categorized_items] + [s for s in self.categorized_items if s not in popular_order]
        
        self.main_tabview = ctk.CTkTabview(self, fg_color=PANEL_BG, segmented_button_fg_color=DARK_BG, segmented_button_selected_color=ACCENT, segmented_button_selected_hover_color="#2563EB")
        self.main_tabview.pack(fill="both", expand=True, padx=20, pady=0)
        self.tab_items = self.main_tabview.add("Items")
        self.tab_custom = self.main_tabview.add("Custom Items")

        self.tabview = ctk.CTkTabview(self.tab_items, fg_color=PANEL_BG, segmented_button_fg_color=DARK_BG, segmented_button_selected_color=ACCENT, segmented_button_selected_hover_color="#2563EB")
        self.tabview.pack(fill="both", expand=True, padx=10, pady=0)
        
        self.tab_data = {}
        
        for slot in sorted_slots:
            tab = self.tabview.add(slot)
            
            left_frame = ctk.CTkFrame(tab, fg_color=DARK_BG)
            left_frame.pack(side="left", fill="both", expand=True, padx=10, pady=10)
            ctk.CTkLabel(left_frame, text="1. Target (What you want)", text_color="#E0E0E0", font=ctk.CTkFont(size=14, weight="bold")).pack(anchor="w", padx=10, pady=(10,0))
            
            search_var = tk.StringVar()
            search_entry = ctk.CTkEntry(left_frame, textvariable=search_var, placeholder_text="Search...", border_width=0, fg_color=PANEL_BG, text_color="#FFF")
            search_entry.pack(fill="x", padx=10, pady=10)
            
            lb_frame_t = ctk.CTkFrame(left_frame, fg_color="transparent")
            lb_frame_t.pack(fill="both", expand=True, padx=10, pady=(0,10))
            
            target_lb = tk.Listbox(lb_frame_t, bg=PANEL_BG, fg="#FFFFFF", font=("Segoe UI", 12), selectbackground=ACCENT, selectforeground="#FFFFFF", exportselection=False, highlightthickness=0, borderwidth=0)
            target_lb.pack(side="left", fill="both", expand=True)
            t_scroll = ctk.CTkScrollbar(lb_frame_t, command=target_lb.yview, button_color="#555", button_hover_color="#777")
            t_scroll.pack(side="right", fill="y")
            target_lb.config(yscrollcommand=t_scroll.set)
            
            right_frame = ctk.CTkFrame(tab, fg_color=DARK_BG)
            right_frame.pack(side="right", fill="both", expand=True, padx=10, pady=10)
            ctk.CTkLabel(right_frame, text="2. Sacrifice (What you have equipped)", text_color="#E0E0E0", font=ctk.CTkFont(size=14, weight="bold")).pack(anchor="w", padx=10, pady=(10,0))
            
            sac_search_var = tk.StringVar()
            sac_search_entry = ctk.CTkEntry(right_frame, textvariable=sac_search_var, placeholder_text="Search...", border_width=0, fg_color=PANEL_BG, text_color="#FFF")
            sac_search_entry.pack(fill="x", padx=10, pady=10)
            
            lb_frame_s = ctk.CTkFrame(right_frame, fg_color="transparent")
            lb_frame_s.pack(fill="both", expand=True, padx=10, pady=(0,10))
            
            sac_lb = tk.Listbox(lb_frame_s, bg=PANEL_BG, fg="#FFFFFF", font=("Segoe UI", 12), selectbackground=ACCENT, selectforeground="#FFFFFF", exportselection=False, highlightthickness=0, borderwidth=0)
            sac_lb.pack(side="left", fill="both", expand=True)
            s_scroll = ctk.CTkScrollbar(lb_frame_s, command=sac_lb.yview, button_color="#555", button_hover_color="#777")
            s_scroll.pack(side="right", fill="y")
            sac_lb.config(yscrollcommand=s_scroll.set)
            
            tab_info = {
                "items": self.categorized_items[slot],
                "target_lb": target_lb,
                "sac_lb": sac_lb,
                "search_var": search_var,
                "sac_search_var": sac_search_var,
                "current_target_dict": None
            }
            self.tab_data[slot] = tab_info
            
            for item_dict in tab_info["items"]:
                target_lb.insert("end", item_dict["display"])
                sac_lb.insert("end", item_dict["display"])
                
            def filter_target(var, index, mode, t_info=tab_info):
                query = t_info["search_var"].get().lower()
                t_info["target_lb"].delete(0, "end")
                for item_dict in t_info["items"]:
                    if query in item_dict["display"].lower(): 
                        t_info["target_lb"].insert("end", item_dict["display"])
                        
            def filter_sac(var, index, mode, t_info=tab_info):
                query = t_info["sac_search_var"].get().lower()
                t_info["sac_lb"].delete(0, "end")
                tdict = t_info["current_target_dict"]
                for item_dict in t_info["items"]:
                    if tdict is None or can_fit(tdict, item_dict):
                        if query in item_dict["display"].lower():
                            t_info["sac_lb"].insert("end", item_dict["display"])
                        
            def on_target_select(evt, t_info=tab_info):
                sel = t_info["target_lb"].curselection()
                if not sel: return
                t_disp = t_info["target_lb"].get(sel[0])
                target_item_dict = next(i for i in t_info["items"] if i["display"] == t_disp)
                t_info["current_target_dict"] = target_item_dict
                filter_sac(None, None, None, t_info)
                
            target_lb.bind("<<ListboxSelect>>", on_target_select)
                        
            search_var.trace_add("write", filter_target)
            sac_search_var.trace_add("write", filter_sac)

        self.bot_frame = ctk.CTkFrame(self, fg_color="transparent")
        self.bot_frame.pack(fill="x", padx=20, pady=20)
        
        self.btn_swap = ctk.CTkButton(self.bot_frame, text="REPLACE IN-GAME", text_color="#FFF", fg_color=ACCENT, hover_color="#2563EB", font=ctk.CTkFont(size=18, weight="bold"), height=50, command=lambda: self.do_swap(in_game=True))
        self.btn_swap.pack(side="left", fill="x", expand=True, padx=(0, 10))
        self.btn_generate = ctk.CTkButton(self.bot_frame, text="GENERATE FILE ONLY", text_color="#FFF", fg_color="#10B981", hover_color="#059669", font=ctk.CTkFont(size=18, weight="bold"), height=50, command=lambda: self.do_swap(in_game=False))
        self.btn_generate.pack(side="right", fill="x", expand=True, padx=(10, 0))
        self.build_custom_items_tab()
    def clear_rl_cache(self):
        """Clear the Rocket League texture/shader cache so the game re-reads our modified files."""
        import os, shutil
        cache_paths = [
            os.path.join(os.path.expanduser("~"), "Documents", "My Games", "Rocket League", "TAGame", "Cache"),
            os.path.join(os.environ.get("USERPROFILE",""), "Documents", "My Games", "Rocket League", "TAGame", "Cache"),
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

    def restore_backups(self):
        folder_path = Path(self.folder)
        bak_files = list(folder_path.glob("*.bak"))
        if not bak_files:
            messagebox.showinfo("Restore Backups", "No .bak backup files found.\nNothing to restore.")
            return
        restored = 0
        failed = 0
        for bak in bak_files:
            upk_path = bak.with_suffix("")
            try:
                shutil.copy2(bak, upk_path)
                bak.unlink()
                restored += 1
            except Exception as e:
                failed += 1
        # Also clean up AssetSwapper_Decrypted temp folder
        if getattr(sys, 'frozen', False):
            script_d = Path(sys.executable).parent
        else:
            script_d = Path(__file__).parent
        temp_dir = script_d / "AssetSwapper_Decrypted"
        if temp_dir.exists():
            shutil.rmtree(temp_dir, ignore_errors=True)
        cache_cleared = self.clear_rl_cache()
        msg = f"✅ Restored {restored} file(s) from backup."
        if failed:
            msg += f"\n⚠️ {failed} file(s) failed to restore."
        msg += "\n\nYour game files are back to normal!"
        if cache_cleared:
            msg += "\n✅ Game cache cleared automatically."
        messagebox.showinfo("Restore Complete", msg)

    def set_in_dir(self):
        folder = filedialog.askdirectory(initialdir=self.folder, title="Select CookedPCConsole Directory")
        if folder:
            self.folder = folder
            messagebox.showinfo("RL Folder", f"Rocket League CookedPCConsole folder set to:\n{self.folder}")
            
    def show_logs(self):
        log_win = ctk.CTkToplevel(self)
        log_win.title("Swaps Log")
        log_win.geometry("600x400")
        log_win.configure(fg_color=DARK_BG)
        
        textbox = ctk.CTkTextbox(log_win, fg_color=PANEL_BG, text_color="#FFF", font=("Consolas", 12))
        textbox.pack(fill="both", expand=True, padx=10, pady=10)
        
        if os.path.exists(self.swaps_log):
            with open(self.swaps_log, "r", encoding="utf-8") as f:
                textbox.insert("1.0", f.read())
        else:
            textbox.insert("1.0", "No logs found.")
            
        textbox.configure(state="disabled")
        
        def purge_backups():
            if messagebox.askyesno("Clear Swap History", "This will permanently delete all .bak backup files and clear your swap log so you can start completely fresh.\n\nNote: This will NOT restore swapped items. You must Verify Game Files in Steam/Epic to revert items to normal.\n\nProceed?"):
                count = 0
                try:
                    for root, dirs, files in os.walk(self.folder):
                        for f in files:
                            if f.endswith(".bak"):
                                os.remove(os.path.join(root, f))
                                count += 1
                except Exception: pass
                
                textbox.configure(state="normal")
                textbox.delete("1.0", "end")
                textbox.insert("1.0", f"Purged {count} obsolete backups. Logs cleared.")
                textbox.configure(state="disabled")
                with open(self.swaps_log, "w", encoding="utf-8") as f:
                    f.write("# RL Swaps Log\n\n")
                messagebox.showinfo("Success", f"Cleared swap history and deleted {count} backups.")
        
        btn_purge = ctk.CTkButton(log_win, text="Clear Swap History & Delete Backups", fg_color="#F59E0B", hover_color="#D97706", command=purge_backups)
        btn_purge.pack(fill="x", padx=10, pady=(0, 10))

    def append_to_log(self, target_item, donor_item, mode="SWAPPED"):
        upk_name = f"{target_item.asset_package}"
        with open(self.swaps_log, "a", encoding="utf-8") as f:
            f.write(f"[{mode}] {donor_item.product} -> {target_item.product} [{upk_name}]\n")

    def do_swap(self, in_game=True):
        current_tab_name = self.tabview.get()
        if current_tab_name not in self.tab_data:
            return
        t_info = self.tab_data[current_tab_name]
        sel_t, sel_s = t_info["target_lb"].curselection(), t_info["sac_lb"].curselection()
        if not sel_t or not sel_s:
            messagebox.showwarning("Warning", "Please select BOTH a Target and a Sacrifice item!")
            return
            
        donor_item = next(i["item"] for i in t_info["items"] if i["display"] == t_info["target_lb"].get(sel_t[0]))
        target_item = next(i["item"] for i in t_info["items"] if i["display"] == t_info["sac_lb"].get(sel_s[0]))
        
        if in_game:
            out_dir = Path(self.folder)
            btn = self.btn_swap
            btn_text = "REPLACE IN-GAME"
        else:
            selected_dir = filedialog.askdirectory(title="Select where to save the generated file")
            if not selected_dir: return
            out_dir = Path(selected_dir)
            btn = self.btn_generate
            btn_text = "GENERATE FILE ONLY"
            
        btn.configure(text="PROCESSING...", fg_color="#D84315", text_color="#FFF", state="disabled")
        threading.Thread(target=self.run_engine, args=(donor_item, target_item, out_dir, in_game, btn, btn_text), daemon=True).start()
        
    def run_engine(self, donor_item, target_item, out_dir, in_game, btn, btn_text):
        try:
            map_path = Path(get_resource_path('keys_map.json'))
            if map_path.exists():
                with open(map_path, 'r', encoding='utf-8') as f:
                    rl_asset_swapper._keys_map = json.load(f)
            else:
                rl_asset_swapper._keys_map = {}
                
            opts = SwapOptions(
                items_path=Path(get_resource_path('items.json')),
                keys_path=self.keys_path,
                donor_dir=Path(self.folder),
                output_dir=out_dir,
                key_source_dir=Path(self.folder),
                include_thumbnails=False,
                preserve_header_offsets=True,
                overwrite=True,
                logger=None
            )
            rl_asset_swapper.swap_asset(rl_upk_editor, target_item, donor_item, opts)
            mode = "IN-GAME" if in_game else f"GENERATED"
            self.append_to_log(target_item, donor_item, mode=mode)
            
            self.after(0, lambda: btn.configure(text="SUCCESS!", fg_color="#10B981", state="normal"))
            if in_game:
                self.after(2000, lambda: btn.configure(text=btn_text, fg_color=ACCENT))
            else:
                self.after(2000, lambda: btn.configure(text=btn_text, fg_color="#10B981"))
            self.after(0, lambda: messagebox.showinfo("Success", f"Operation complete: {donor_item.product} -> {target_item.product}."))
        except Exception as e:
            self.after(0, lambda: btn.configure(text="FAILED", fg_color="#EF4444", state="normal"))
            if in_game:
                self.after(2000, lambda: btn.configure(text=btn_text, fg_color=ACCENT))
            else:
                self.after(2000, lambda: btn.configure(text=btn_text, fg_color="#10B981"))
            self.after(0, lambda: messagebox.showerror("Error", f"Failed: {str(e)}"))


    def build_custom_items_tab(self):
        self.custom_tabview = ctk.CTkTabview(self.tab_custom, fg_color=PANEL_BG)
        self.custom_tabview.pack(fill="both", expand=True, padx=10, pady=0)
        
        # Profile Picture Subtab
        pfp_sub = self.custom_tabview.add("Profile Picture")
        pfp_paned = ctk.CTkFrame(pfp_sub, fg_color="transparent")
        pfp_paned.pack(fill="both", expand=True, padx=10, pady=10)
        
        pfp_left = ctk.CTkFrame(pfp_paned, fg_color=DARK_BG)
        pfp_left.pack(side="left", fill="both", expand=True, padx=(0, 10))
        
        pfp_right = ctk.CTkFrame(pfp_paned, fg_color=DARK_BG, width=320)
        pfp_right.pack(side="right", fill="y")
        pfp_right.pack_propagate(False)
        
        search_frame_p = ctk.CTkFrame(pfp_left, fg_color="transparent")
        search_frame_p.pack(fill="x", padx=10, pady=(10, 5))
        
        self.pfp_search_var = ctk.StringVar()
        self.pfp_search_var.trace_add("write", self.filter_pfps)
        search_entry_p = ctk.CTkEntry(search_frame_p, textvariable=self.pfp_search_var, placeholder_text="Search avatar borders...", width=300)
        search_entry_p.pack(side="left", padx=(0, 10))
        
        from tkinter import Listbox
        self.pfp_listbox = Listbox(pfp_left, bg="#2A2A2A", fg="#FFF", font=("Consolas", 11), selectbackground="#3B82F6", highlightthickness=0, bd=0)
        self.pfp_listbox.pack(fill="both", expand=True, padx=10, pady=(0, 10))
        self.pfp_listbox.bind('<<ListboxSelect>>', self.on_pfp_select)
        
        ctk.CTkLabel(pfp_right, text="Custom Profile Picture", text_color=ACCENT, font=ctk.CTkFont(size=20, weight="bold")).pack(pady=(20, 10))
        
        self.pfp_target_var = ctk.StringVar(value="")
        self.pfp_name_lbl = ctk.CTkLabel(pfp_right, text="Selected: None", font=ctk.CTkFont(size=14, weight="bold"), text_color="#10B981")
        self.pfp_name_lbl.pack(pady=(0, 10))
        
        import tkinter as tk
        self.lbl_pfp_preview = tk.Label(pfp_right, bg="#2D2D2D")
        self.lbl_pfp_preview.pack(pady=10)
        
        self.pfp_path_var = ctk.StringVar()
        
        def select_pfp():
            p = filedialog.askopenfilename(filetypes=[("Image Files", "*.png;*.jpg;*.jpeg;*.webp;*.bmp")])
            if p:
                def set_cropped(cropped_path):
                    self.pfp_path_var.set(cropped_path)
                    from PIL import Image, ImageTk
                    prev_img = Image.open(cropped_path).resize((120, 120), Image.LANCZOS)
                    self.pfp_preview_img = ImageTk.PhotoImage(prev_img)
                    self.lbl_pfp_preview.configure(image=self.pfp_preview_img)
                    self.lbl_pfp_preview.image = self.pfp_preview_img
                CropWindow(self, p, set_cropped, ratio_w=1, ratio_h=1)
                
        def apply_pfp():
            png_path = self.pfp_path_var.get()
            if not png_path:
                messagebox.showwarning("Warning", "Select an image first!")
                return
            if not self.pfp_target_var.get():
                messagebox.showwarning("Warning", "Select a target Avatar Border first!")
                return
            btn = self.btn_apply_pfp
            btn.configure(text="PROCESSING...", fg_color="#D84315", state="disabled")
            def run_pfp():
                try:
                    import rl_asset_swapper, rl_upk_editor
                    from rl_asset_swapper import SwapOptions
                    from pathlib import Path
                    opts = SwapOptions(
                        items_path=Path(get_resource_path('items.json')),
                        keys_path=self.keys_path,
                        donor_dir=Path(self.folder),
                        output_dir=Path(self.folder),
                        key_source_dir=Path(self.folder),
                        include_thumbnails=False,
                        preserve_header_offsets=True,
                        overwrite=True,
                        logger=None
                    )
                    target_pkg = self.pfp_target_var.get()
                    rl_asset_swapper.swap_pfp_from_png(rl_upk_editor, Path(png_path), target_pkg, opts)
                    with open(self.swaps_log, "a", encoding="utf-8") as f:
                        f.write(f"[SWAPPED] Custom PFP -> {target_pkg}\n")
                    self.clear_rl_cache()
                    self.after(0, lambda: messagebox.showinfo("Success", f"Successfully applied Custom PFP to {target_pkg}!\n\n✅ Game cache cleared — start Rocket League now!"))
                except Exception as e:
                    self.after(0, lambda e=e: messagebox.showerror("Error", f"Failed: {e}"))
                finally:
                    self.after(0, lambda: btn.configure(text="APPLY PROFILE PICTURE", fg_color=ACCENT, state="normal"))
            import threading
            threading.Thread(target=run_pfp, daemon=True).start()
            
        ctk.CTkButton(pfp_right, text="Select Image", text_color="#FFF", fg_color="#10B981", hover_color="#059669", font=ctk.CTkFont(size=16, weight="bold"), height=40, command=select_pfp).pack(pady=10)
        self.btn_apply_pfp = ctk.CTkButton(pfp_right, text="APPLY PROFILE PICTURE", text_color="#FFF", fg_color=ACCENT, hover_color="#2563EB", font=ctk.CTkFont(size=18, weight="bold"), height=50, command=apply_pfp)
        self.btn_apply_pfp.pack(pady=(20, 10))

        # Custom Banner Subtab
        banner_sub = self.custom_tabview.add("Custom Banner")
        banner_paned = ctk.CTkFrame(banner_sub, fg_color="transparent")
        banner_paned.pack(fill="both", expand=True, padx=10, pady=10)
        
        banner_left = ctk.CTkFrame(banner_paned, fg_color=DARK_BG)
        banner_left.pack(side="left", fill="both", expand=True, padx=(0, 10))
        
        banner_right = ctk.CTkFrame(banner_paned, fg_color=DARK_BG, width=320)
        banner_right.pack(side="right", fill="y")
        banner_right.pack_propagate(False)
        
        search_frame_b = ctk.CTkFrame(banner_left, fg_color="transparent")
        search_frame_b.pack(fill="x", padx=10, pady=(10, 5))
        
        self.banner_search_var = ctk.StringVar()
        self.banner_search_var.trace_add("write", self.filter_banners)
        search_entry_b = ctk.CTkEntry(search_frame_b, textvariable=self.banner_search_var, placeholder_text="Search banners by name...", width=300)
        search_entry_b.pack(side="left", padx=(0, 10))
        
        from tkinter import Listbox
        self.banner_listbox = Listbox(banner_left, bg="#2A2A2A", fg="#FFF", font=("Consolas", 11), selectbackground="#3B82F6", highlightthickness=0, bd=0)
        self.banner_listbox.pack(fill="both", expand=True, padx=10, pady=(0, 10))
        self.banner_listbox.bind('<<ListboxSelect>>', self.on_banner_select)
        
        ctk.CTkLabel(banner_right, text="Custom Player Banner", text_color=ACCENT, font=ctk.CTkFont(size=20, weight="bold")).pack(pady=(20, 10))
        
        self.banner_target_var = ctk.StringVar(value="PlayerBanner_Default_SF.upk")
        self.banner_name_lbl = ctk.CTkLabel(banner_right, text="Selected: Default Banner", font=ctk.CTkFont(size=14, weight="bold"), text_color="#10B981")
        self.banner_name_lbl.pack(pady=(0, 10))
        
        self.lbl_banner_preview = tk.Label(banner_right, bg="#2D2D2D")
        self.lbl_banner_preview.pack(pady=10)
        
        def select_banner():
            p = filedialog.askopenfilename(filetypes=[("Image Files", "*.png;*.jpg;*.jpeg;*.webp;*.bmp")])
            if p:
                def set_cropped(cropped_path):
                    self.cropped_banner_path = cropped_path
                    from PIL import Image, ImageTk
                    prev_img = Image.open(cropped_path).resize((256, 64), Image.LANCZOS)
                    self.banner_preview_img = ImageTk.PhotoImage(prev_img)
                    self.lbl_banner_preview.configure(image=self.banner_preview_img)
                    self.lbl_banner_preview.image = self.banner_preview_img
                CropWindow(self, p, set_cropped, ratio_w=14, ratio_h=3)
        
        btn_banner = ctk.CTkButton(banner_right, text="STEP 1: Select & Crop Image", font=("Arial", 14, "bold"), fg_color="#3B82F6", hover_color="#2563EB", command=select_banner)
        btn_banner.pack(pady=(0, 20))
        
        self.btn_apply_banner = ctk.CTkButton(banner_right, text="STEP 2: INJECT", font=ctk.CTkFont(size=18, weight="bold"), height=50, fg_color="#10B981", hover_color="#059669", state="disabled", command=self.apply_banner)
        self.btn_apply_banner.pack(pady=20)
        
        self.cropped_banner_path = None
        
        import json
        try:
            with open(get_resource_path('items.json'), 'r', encoding='utf-8') as f:
                data = json.load(f)['Items']
            self.all_banners = [i for i in data if i.get('Slot') == 'Player Banner' and i.get('AssetPackage')]
            self.all_banners.sort(key=lambda x: x.get('Product', ''))
            self.all_pfps = [i for i in data if i.get('Slot') == 'Avatar Border' and i.get('AssetPackage')]
            self.all_pfps.sort(key=lambda x: x.get('Product', ''))
        except:
            self.all_banners = []
            self.all_pfps = []
            
        self.filter_banners()
        self.filter_pfps()

    def filter_pfps(self, *args):
        import tkinter as tk
        search = self.pfp_search_var.get().lower()
        self.pfp_listbox.delete(0, tk.END)
        self.pfp_map = {}
        for p in self.all_pfps:
            name = p.get('Product', 'Unknown')
            pkg = p.get('AssetPackage', '')
            if search in name.lower() or search in pkg.lower():
                display = f"{name} ({pkg})"
                self.pfp_listbox.insert(tk.END, display)
                self.pfp_map[display] = (name, pkg)

    def on_pfp_select(self, event):
        sel = self.pfp_listbox.curselection()
        if not sel: return
        display = self.pfp_listbox.get(sel[0])
        name, pkg = self.pfp_map.get(display, ("", ""))
        self.pfp_target_var.set(pkg)
        self.pfp_name_lbl.configure(text=f"Selected: {name}")

    def filter_banners(self, *args):
        import tkinter as tk
        search = self.banner_search_var.get().lower()
        self.banner_listbox.delete(0, tk.END)
        self.banner_map = {}
        for b in self.all_banners:
            name = b.get('Product', 'Unknown')
            pkg = b.get('AssetPackage', '')
            if search in name.lower() or search in pkg.lower():
                display = f"{name} ({pkg})"
                self.banner_listbox.insert(tk.END, display)
                self.banner_map[display] = (name, pkg)

    def on_banner_select(self, event):
        sel = self.banner_listbox.curselection()
        if not sel: return
        display = self.banner_listbox.get(sel[0])
        name, pkg = self.banner_map.get(display, ("", ""))
        self.banner_target_var.set(pkg)
        self.banner_name_lbl.configure(text=f"Selected:\n{name}")
        self.btn_apply_banner.configure(state="normal")

    def apply_banner(self):
        if not getattr(self, 'cropped_banner_path', None):
            messagebox.showwarning("Warning", "Please Select and Crop an Image in Step 1 first!")
            return
        target_pkg = self.banner_target_var.get()
        btn = self.btn_apply_banner
        btn.configure(text="INJECTING...", fg_color="#D84315", state="disabled")
        
        def run_ban():
            try:
                import rl_asset_swapper, rl_upk_editor
                from rl_asset_swapper import SwapOptions
                from pathlib import Path
                opts = SwapOptions(
                    items_path=Path(get_resource_path('items.json')),
                    keys_path=self.keys_path,
                    donor_dir=Path(self.folder),
                    output_dir=Path(self.folder),
                    key_source_dir=Path(self.folder),
                    include_thumbnails=False,
                    preserve_header_offsets=True,
                    overwrite=True,
                    logger=None
                )
                rl_asset_swapper.swap_banner_from_png(rl_upk_editor, Path(self.cropped_banner_path), target_pkg, opts)
                with open(self.swaps_log, "a", encoding="utf-8") as f:
                    f.write(f"[SWAPPED] Custom Banner -> {target_pkg}\n")
                self.after(0, lambda: messagebox.showinfo("Success", f"Successfully applied Custom Banner to {target_pkg}!"))
            except Exception as e:
                self.after(0, lambda e=e: messagebox.showerror("Banner Error", f"Failed to inject banner:\n{str(e)}"))
            finally:
                self.after(0, lambda: btn.configure(text="STEP 2: INJECT", fg_color="#10B981", state="normal"))
                
        import threading
        threading.Thread(target=run_ban, daemon=True).start()

if __name__ == "__main__":
    app = RLSwapperApp()
    app.mainloop()
