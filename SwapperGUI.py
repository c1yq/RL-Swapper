import customtkinter as ctk
import tkinter as tk
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

class RLSwapperApp(ctk.CTk):
    def __init__(self):
        super().__init__()
        self.title("RL Swapper")
        self.geometry("1100x750")
        self.configure(fg_color=DARK_BG)
        
        if getattr(sys, 'frozen', False):
            self.app_dir = os.path.dirname(sys.executable)
        else:
            self.app_dir = os.path.dirname(os.path.abspath(__file__))
            
        self.swaps_log = os.path.join(self.app_dir, "swaps.txt")
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
        
        self.in_dir_btn = ctk.CTkButton(self.header_frame, text="?? Set RL Folder", fg_color=PANEL_BG, text_color="#E0E0E0", hover_color="#333333", border_width=1, border_color="#555", command=self.set_in_dir)
        self.in_dir_btn.pack(side="left", padx=(0, 10))
        
        self.out_dir_btn = ctk.CTkButton(self.header_frame, text="?? Set Output Dir", fg_color=PANEL_BG, text_color="#E0E0E0", hover_color="#333333", border_width=1, border_color="#555", command=self.set_output_dir)
        self.out_dir_btn.pack(side="left")
        
        self.title_label = ctk.CTkLabel(self.header_frame, text="RL SWAPPER", text_color="#E0E0E0", font=ctk.CTkFont(family="Segoe UI", size=28, weight="bold"))
        self.title_label.pack(side="left", expand=True)
        
        self.logs_btn = ctk.CTkButton(self.header_frame, text="Logs", fg_color=PANEL_BG, text_color="#E0E0E0", hover_color="#333333", border_width=1, border_color="#555", command=self.show_logs)
        self.logs_btn.pack(side="right")
        
        popular_order = ["Body", "Decal", "Wheels", "Rocket Boost", "Goal Explosion", "Trail", "Antenna", "Topper"]
        sorted_slots = [s for s in popular_order if s in self.categorized_items] + [s for s in self.categorized_items if s not in popular_order]
        
        self.tabview = ctk.CTkTabview(self, fg_color=PANEL_BG, segmented_button_fg_color=DARK_BG, segmented_button_selected_color=ACCENT, segmented_button_selected_hover_color="#2563EB")
        self.tabview.pack(fill="both", expand=True, padx=20, pady=0)
        
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
        
        self.btn_swap = ctk.CTkButton(self.bot_frame, text="SWAP IN-GAME", text_color="#FFF", fg_color=ACCENT, hover_color="#2563EB", font=ctk.CTkFont(size=18, weight="bold"), height=50, command=lambda: self.do_swap(in_game=True))
        self.btn_swap.pack(side="left", fill="x", expand=True, padx=(0, 10))
        
        self.btn_generate = ctk.CTkButton(self.bot_frame, text="GENERATE ONLY", text_color="#FFF", fg_color="#10B981", hover_color="#059669", font=ctk.CTkFont(size=18, weight="bold"), height=50, command=lambda: self.do_swap(in_game=False))
        self.btn_generate.pack(side="right", fill="x", expand=True, padx=(10, 0))

    def set_in_dir(self):
        folder = filedialog.askdirectory(initialdir=self.folder, title="Select CookedPCConsole Directory")
        if folder:
            self.folder = folder
            messagebox.showinfo("RL Folder", f"Rocket League CookedPCConsole folder set to:\n{self.folder}")
            
    def set_output_dir(self):
        folder = filedialog.askdirectory(initialdir=self.output_dir, title="Select Output Directory")
        if folder:
            self.output_dir = folder
            messagebox.showinfo("Output Directory", f"Output directory set to:\n{self.output_dir}")

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
        
        def clear_logs_and_undo():
            if messagebox.askyesno("Undo All Changes", "This will restore all backed-up .upk files in CookedPCConsole and clear the log file. Proceed?"):
                count = self.undo_all_changes()
                textbox.configure(state="normal")
                textbox.delete("1.0", "end")
                textbox.insert("1.0", f"Restored {count} files. Logs cleared.")
                textbox.configure(state="disabled")
                with open(self.swaps_log, "w", encoding="utf-8") as f:
                    f.write("# RL Swaps Log\n\n")
                messagebox.showinfo("Success", f"Undid all changes. Restored {count} files.")
        
        btn_undo = ctk.CTkButton(log_win, text="Undo All Changes (Restore In-Game Backups)", fg_color="#EF4444", hover_color="#DC2626", command=clear_logs_and_undo)
        btn_undo.pack(fill="x", padx=10, pady=10)
        
    def undo_all_changes(self):
        restored_count = 0
        try:
            for root, dirs, files in os.walk(self.folder):
                for f in files:
                    if f.endswith(".bak"):
                        upk_name = f[:-4]
                        try:
                            shutil.copy2(os.path.join(root, f), os.path.join(root, upk_name))
                            os.remove(os.path.join(root, f))
                            restored_count += 1
                        except Exception: pass
        except Exception: pass
        return restored_count
        
    def append_to_log(self, target_item, donor_item, mode="SWAPPED"):
        upk_name = f"{target_item.asset_package}"
        with open(self.swaps_log, "a", encoding="utf-8") as f:
            f.write(f"[{mode}] {donor_item.product} -> {target_item.product} [{upk_name}]\n")

    def do_swap(self, in_game=True):
        current_tab_name = self.tabview.get()
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
            btn_text = "SWAP IN-GAME"
        else:
            out_dir = Path(self.output_dir)
            btn = self.btn_generate
            btn_text = "GENERATE ONLY"
            
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

if __name__ == "__main__":
    app = RLSwapperApp()
    app.mainloop()
