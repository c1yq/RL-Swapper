#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui::{
    self, Align, Color32, ColorImage, FontId, Layout, RichText, Rounding, Sense, Stroke,
    TextureHandle, TextureOptions, Vec2,
};
use image::{imageops::FilterType, RgbaImage};
use rl_swapper::custom_studio::{
    compose_pfp_with_border_preview, inject_alphaconsole_ball, inject_alphaconsole_decal_at,
    inject_custom_banner, inject_custom_pfp, list_avatar_border_packages_with_catalog,
    list_player_banner_packages_with_catalog, parse_alphaconsole_ball_source,
    parse_alphaconsole_decal_source, AlphaConsoleBallPack, AlphaConsoleDecalPack,
};
use rl_swapper::icon_system::{
    composite_icon_with_paint, extract_ball_texture_preview, IconCacheManager,
};
use rl_swapper::map_editor::{
    extract_map_preview_image, reset_all_standard_maps, swap_standard_map, StandardMapSpec,
    STANDARD_SOCCAR_MAPS,
};
use rl_swapper::templates::{
    bake_name_onto_banner, generate_ball_template, generate_banner_template,
    generate_decal_template_for_car, render_2d_ball_orb_preview,
    render_2d_banner_scoreboard_preview, render_2d_car_decal_with_mask_preview,
};
use rl_swapper::upk_engine::{
    bak_path_for, body_hitbox_for_item, clear_rl_texture_cache, detect_car_uv_label,
    find_catalog_item_by_id, get_appdata_dir, inspect_cooked_health, is_avatar_border_modified,
    is_rocket_league_running, load_catalog_items, official_paints, repair_cooked_health,
    restore_single_package, swap_item_packages, CarHitbox, CatalogItem, CookedHealthReport,
    PaintColorSpec,
};
use rl_swapper::workshop::{
    delete_library_map, download_online_workshop_map_with_progress, fetch_all_online_workshop_maps,
    fetch_or_cache_thumb, get_workshop_library_dir, import_workshop_file_or_zip,
    install_workshop_textures_folder, list_library_maps, load_workshop_map_into_underpass,
    restore_workshop_underpass, scan_upk_external_dependencies, OnlineWorkshopItem,
    WorkshopMapEntry, WORKSHOP_SAFE_SLOT,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

// ── Adaptive System Light / OLED Dark Palette (Crisp Architectural Minimal) ──
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
enum ThemeMode {
    #[default]
    Auto,
    Dark,
    Light,
}

#[derive(Clone, Copy)]
struct AppPalette {
    canvas: Color32,
    sidebar: Color32,
    surface: Color32,
    elevated: Color32,
    inset: Color32,
    list_bg: Color32,
    well_bg: Color32,
    hover: Color32,
    hover_stroke: Color32,
    selected: Color32,
    selected_stroke: Color32,
    selected_text: Color32,
    sidebar_sel_bg: Color32,
    sidebar_sel_stroke: Color32,
    chip_sel_bg: Color32,
    chip_sel_fg: Color32,
    chip_sel_stroke: Color32,
    btn_secondary_stroke: Color32,
    divider: Color32,
    input_stroke: Color32,
    switch_off_stroke: Color32,
    switch_off_hover: Color32,
    thumb_bg: Color32,
    text_primary: Color32,
    text_secondary: Color32,
    text_muted: Color32,
    accent_blue: Color32,
    accent_green: Color32,
    accent_amber: Color32,
    accent_red: Color32,
}

impl AppPalette {
    const fn dark() -> Self {
        Self {
            canvas: Color32::BLACK,
            sidebar: Color32::from_rgb(5, 5, 7),
            surface: Color32::from_rgb(9, 9, 12),
            elevated: Color32::from_rgb(13, 13, 17),
            inset: Color32::from_rgb(16, 16, 20),
            list_bg: Color32::from_rgb(6, 6, 8),
            well_bg: Color32::from_rgb(2, 2, 4),
            hover: Color32::from_rgb(22, 23, 29),
            hover_stroke: Color32::from_rgb(42, 44, 54),
            selected: Color32::from_rgb(12, 24, 42),
            selected_stroke: Color32::from_rgb(38, 88, 160),
            selected_text: Color32::WHITE,
            sidebar_sel_bg: Color32::from_rgb(12, 24, 42),
            sidebar_sel_stroke: Color32::from_rgb(30, 66, 118),
            chip_sel_bg: Color32::from_rgb(16, 36, 68),
            chip_sel_fg: Color32::from_rgb(142, 202, 255),
            chip_sel_stroke: Color32::from_rgb(42, 124, 232),
            btn_secondary_stroke: Color32::from_rgb(32, 33, 42),
            divider: Color32::from_rgb(24, 25, 32),
            input_stroke: Color32::from_rgb(38, 40, 52),
            switch_off_stroke: Color32::from_rgb(54, 56, 68),
            switch_off_hover: Color32::from_rgb(76, 80, 96),
            thumb_bg: Color32::from_rgb(18, 18, 25),
            text_primary: Color32::from_rgb(242, 242, 246),
            text_secondary: Color32::from_rgb(160, 164, 176),
            text_muted: Color32::from_rgb(102, 106, 118),
            accent_blue: Color32::from_rgb(24, 128, 245),
            accent_green: Color32::from_rgb(46, 204, 88),
            accent_amber: Color32::from_rgb(248, 154, 14),
            accent_red: Color32::from_rgb(250, 68, 58),
        }
    }

    const fn light() -> Self {
        Self {
            canvas: Color32::from_rgb(243, 244, 247),
            sidebar: Color32::from_rgb(235, 237, 242),
            surface: Color32::WHITE,
            elevated: Color32::from_rgb(248, 249, 252),
            inset: Color32::from_rgb(239, 241, 246),
            list_bg: Color32::WHITE,
            well_bg: Color32::from_rgb(230, 233, 240),
            hover: Color32::from_rgb(227, 231, 239),
            hover_stroke: Color32::from_rgb(190, 196, 208),
            selected: Color32::from_rgb(224, 236, 255),
            selected_stroke: Color32::from_rgb(32, 112, 224),
            selected_text: Color32::from_rgb(14, 48, 104),
            sidebar_sel_bg: Color32::from_rgb(222, 235, 254),
            sidebar_sel_stroke: Color32::from_rgb(148, 186, 242),
            chip_sel_bg: Color32::from_rgb(222, 236, 255),
            chip_sel_fg: Color32::from_rgb(12, 74, 170),
            chip_sel_stroke: Color32::from_rgb(36, 118, 230),
            btn_secondary_stroke: Color32::from_rgb(204, 209, 220),
            divider: Color32::from_rgb(214, 219, 228),
            input_stroke: Color32::from_rgb(192, 198, 210),
            switch_off_stroke: Color32::from_rgb(174, 180, 194),
            switch_off_hover: Color32::from_rgb(138, 144, 160),
            thumb_bg: Color32::from_rgb(236, 239, 245),
            text_primary: Color32::from_rgb(22, 24, 30),
            text_secondary: Color32::from_rgb(80, 86, 100),
            text_muted: Color32::from_rgb(126, 132, 146),
            accent_blue: Color32::from_rgb(14, 102, 220),
            accent_green: Color32::from_rgb(22, 146, 62),
            accent_amber: Color32::from_rgb(194, 106, 0),
            accent_red: Color32::from_rgb(212, 42, 36),
        }
    }

    const fn from_dark(is_dark: bool) -> Self {
        if is_dark { Self::dark() } else { Self::light() }
    }

    fn from_ui(ui: &egui::Ui) -> Self {
        Self::from_dark(ui.style().visuals.dark_mode)
    }
}

#[derive(Debug, Clone)]
struct WorkshopDownloadProgress {
    pub name: String,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub phase: String,
}

pub const ORDERED_SLOTS: &[&str] = &[
    "Body",
    "Decal",
    "Wheels",
    "Rocket Boost",
    "Goal Explosion",
    "Trail",
    "Topper",
    "Antenna",
    "Paint Finish",
    "Engine Audio",
    "Player Banner",
    "Avatar Border",
];

#[derive(Debug, Clone, PartialEq, Eq)]
enum ActiveTab {
    CosmeticSlot(String),
    CustomDecal,
    CustomPfp,
    CustomBanner,
    CustomBall,
    MapSwapper,
    WorkshopMaps,
    Presets,
    ActiveHealth,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ActiveSwapRecord {
    pub target_pkg: String,
    pub donor_desc: String,
    pub target_desc: String,
    pub kind: String,
    #[serde(default)]
    pub slot: String,
    #[serde(default)]
    pub donor_pkg: String,
    #[serde(default)]
    pub donor_asset_path: String,
    #[serde(default)]
    pub target_asset_path: String,
    #[serde(default)]
    pub paint_hex: Option<String>,
    #[serde(default)]
    pub paint_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct AppPersistentState {
    pub cooked_dir: String,
    pub rl_exe_mtime: u64,
    pub rl_exe_size: u64,
    pub active_swaps: HashMap<String, ActiveSwapRecord>,
    pub swap_history: Vec<String>,
    #[serde(default)]
    pub last_stadium_setup: Option<SavedStadiumSpec>,
    #[serde(default)]
    pub theme_mode: ThemeMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SavedStadiumSpec {
    #[serde(default)]
    pub donor_upk: Option<String>,
    pub target_upk: String,
    pub override_all_57: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LoadoutPreset {
    pub name: String,
    pub created_at: String,
    pub item_swaps: Vec<ActiveSwapRecord>,
    pub stadium: Option<SavedStadiumSpec>,
}

enum WorkerEvent {
    IconReady {
        cache_key: String,
        rgba: RgbaImage,
    },
    CategorySyncDone {
        slot: String,
        local: usize,
        web: usize,
    },
    WorkshopCatalogDone(Result<Vec<OnlineWorkshopItem>, String>),
    WorkshopThumbReady {
        url: String,
        rgba: RgbaImage,
    },
    WorkshopDownloadProgress(WorkshopDownloadProgress),
    WorkshopDownloadDone(Result<WorkshopMapEntry, String>),
    OperationDone {
        success: bool,
        message: String,
        swapped_pkgs: Vec<ActiveSwapRecord>,
        cleared_pkgs: Vec<String>,
    },
}

fn build_theme_style(is_dark: bool) -> egui::Style {
    let pal = if is_dark { AppPalette::dark() } else { AppPalette::light() };
    let mut style = egui::Style::default();
    style.wrap_mode = Some(egui::TextWrapMode::Truncate);

    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(11.0, 5.0);
    style.spacing.window_margin = egui::Margin::same(14.0);
    // Dedicated crisp scrollbar gutter that never hides behind content or obscures badges
    style.spacing.scroll = egui::style::ScrollStyle {
        floating: false,
        bar_width: 8.0,
        handle_min_length: 32.0,
        bar_inner_margin: 3.0,
        bar_outer_margin: 2.0,
        floating_width: 8.0,
        floating_allocated_width: 8.0,
        foreground_color: true,
        dormant_background_opacity: 0.28,
        active_background_opacity: 0.45,
        interact_background_opacity: 0.62,
        dormant_handle_opacity: 0.65,
        active_handle_opacity: 0.90,
        interact_handle_opacity: 1.0,
    };

    let mut v = if is_dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.dark_mode = is_dark;
    v.window_fill = pal.surface;
    v.panel_fill = pal.canvas;
    v.faint_bg_color = pal.sidebar;
    v.extreme_bg_color = pal.inset;
    v.window_rounding = Rounding::same(6.0);
    v.window_stroke = Stroke::new(1.0_f32, pal.divider);
    v.menu_rounding = Rounding::same(5.0);

    let handle_col = if is_dark {
        Color32::from_rgb(156, 162, 178)
    } else {
        Color32::from_rgb(96, 102, 118)
    };
    let hover_handle_col = if is_dark {
        Color32::from_rgb(220, 224, 235)
    } else {
        Color32::from_rgb(42, 46, 58)
    };

    v.widgets.noninteractive.bg_fill = pal.surface;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, pal.divider);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, pal.text_secondary);
    v.widgets.noninteractive.rounding = Rounding::same(4.0);

    v.widgets.inactive.bg_fill = pal.inset;
    v.widgets.inactive.weak_bg_fill = pal.inset;
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, handle_col);
    v.widgets.inactive.rounding = Rounding::same(4.0);

    v.widgets.hovered.bg_fill = pal.hover;
    v.widgets.hovered.weak_bg_fill = pal.hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, pal.hover_stroke);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, hover_handle_col);
    v.widgets.hovered.rounding = Rounding::same(4.0);

    v.widgets.active.bg_fill = pal.selected;
    v.widgets.active.weak_bg_fill = pal.selected;
    v.widgets.active.bg_stroke = Stroke::new(1.0_f32, pal.accent_blue);
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, hover_handle_col);
    v.widgets.active.rounding = Rounding::same(4.0);

    v.selection.bg_fill = pal.accent_blue;
    v.selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);

    style.visuals = v;
    style
}

fn configure_app_themes(ctx: &egui::Context, mode: ThemeMode) {
    ctx.set_style_of(egui::Theme::Dark, build_theme_style(true));
    ctx.set_style_of(egui::Theme::Light, build_theme_style(false));
    apply_theme_preference(ctx, mode);
}

fn apply_theme_preference(ctx: &egui::Context, mode: ThemeMode) {
    if let Ok(env_t) = std::env::var("RL_SWAPPER_THEME") {
        if env_t.eq_ignore_ascii_case("light") {
            ctx.set_theme(egui::ThemePreference::Light);
            return;
        } else if env_t.eq_ignore_ascii_case("dark") {
            ctx.set_theme(egui::ThemePreference::Dark);
            return;
        }
    }
    let pref = match mode {
        ThemeMode::Auto => egui::ThemePreference::System,
        ThemeMode::Dark => egui::ThemePreference::Dark,
        ThemeMode::Light => egui::ThemePreference::Light,
    };
    ctx.set_theme(pref);
}

fn rgba_to_egui_image(img: &RgbaImage) -> ColorImage {
    ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw())
}

fn make_rounded_icon_rgba(bytes: &[u8], size: u32) -> Option<RgbaImage> {
    let mut out = image::load_from_memory(bytes)
        .ok()?
        .resize_exact(size, size, FilterType::Lanczos3)
        .to_rgba8();
    let r = (size as f32) * 0.16;
    let max_c = (size as f32) - 1.0;
    for y in 0..size {
        for x in 0..size {
            let fx = x as f32;
            let fy = y as f32;
            let dx = if fx < r { r - fx } else if fx > max_c - r { fx - (max_c - r) } else { 0.0 };
            let dy = if fy < r { r - fy } else if fy > max_c - r { fy - (max_c - r) } else { 0.0 };
            if (dx * dx + dy * dy).sqrt() > r {
                out.get_pixel_mut(x, y)[3] = 0;
            }
        }
    }
    Some(out)
}

fn app_card(pal: &AppPalette) -> egui::Frame {
    egui::Frame::none()
        .fill(pal.surface)
        .stroke(Stroke::new(1.0_f32, pal.divider))
        .rounding(Rounding::same(6.0))
        .inner_margin(egui::Margin::same(14.0))
}

fn app_card_full(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    let pal = AppPalette::from_ui(ui);
    app_card(&pal).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        add_contents(ui);
    });
}

fn app_list_card(pal: &AppPalette) -> egui::Frame {
    egui::Frame::none()
        .fill(pal.list_bg)
        .stroke(Stroke::new(1.0_f32, pal.divider))
        .rounding(Rounding::same(6.0))
        .inner_margin(egui::Margin::same(6.0))
}

fn app_well_frame(pal: &AppPalette) -> egui::Frame {
    egui::Frame::none()
        .fill(pal.well_bg)
        .stroke(Stroke::new(1.0_f32, pal.divider))
        .rounding(Rounding::same(4.0))
        .inner_margin(egui::Margin::same(8.0))
}

fn paint_status_dot(ui: &mut egui::Ui, color: Color32, radius: f32) {
    let dim = radius * 2.0 + 4.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(dim), Sense::hover());
    if ui.is_rect_visible(rect) {
        let c = rect.center();
        let glow = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 45);
        ui.painter().circle_filled(c, radius + 2.2, glow);
        ui.painter().circle_filled(c, radius, color);
    }
}

fn paint_arrow_badge(ui: &mut egui::Ui, diameter: f32) {
    let pal = AppPalette::from_ui(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(diameter), Sense::hover());
    if ui.is_rect_visible(rect) {
        let c = rect.center();
        let r = diameter * 0.5;
        ui.painter().rect_filled(rect, Rounding::same(5.0), pal.elevated);
        ui.painter().rect_stroke(rect, Rounding::same(5.0), Stroke::new(1.0_f32, pal.divider));
        let st = Stroke::new(1.6_f32, pal.text_secondary);
        let half = r * 0.40;
        ui.painter().line_segment([egui::pos2(c.x - half, c.y), egui::pos2(c.x + half, c.y)], st);
        ui.painter().line_segment([egui::pos2(c.x + half * 0.25, c.y - half * 0.65), egui::pos2(c.x + half, c.y)], st);
        ui.painter().line_segment([egui::pos2(c.x + half * 0.25, c.y + half * 0.65), egui::pos2(c.x + half, c.y)], st);
    }
}

fn app_switch(ui: &mut egui::Ui, checked: &mut bool, label: &str) -> bool {
    let pal = AppPalette::from_ui(ui);
    let mut toggled = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 7.0;
        let track_size = Vec2::new(32.0, 18.0);
        let (rect, resp) = ui.allocate_exact_size(track_size, Sense::click());
        if resp.clicked() {
            *checked = !*checked;
            toggled = true;
        }
        if ui.is_rect_visible(rect) {
            let fill = if *checked { pal.accent_blue } else { pal.inset };
            let stroke = if *checked {
                Stroke::new(1.0_f32, pal.accent_blue)
            } else if resp.hovered() {
                Stroke::new(1.0_f32, pal.switch_off_hover)
            } else {
                Stroke::new(1.0_f32, pal.switch_off_stroke)
            };
            ui.painter().rect_filled(rect, Rounding::same(4.5), fill);
            ui.painter().rect_stroke(rect, Rounding::same(4.5), stroke);
            let thumb_sz = Vec2::splat(12.0);
            let tx = if *checked {
                rect.right() - thumb_sz.x - 3.0
            } else {
                rect.left() + 3.0
            };
            let thumb_rect = egui::Rect::from_min_size(egui::pos2(tx, rect.center().y - thumb_sz.y * 0.5), thumb_sz);
            ui.painter().rect_filled(
                thumb_rect,
                Rounding::same(3.0),
                if *checked { Color32::WHITE } else { pal.text_secondary },
            );
        }
        let lbl_col = if *checked { pal.text_primary } else { pal.text_secondary };
        if ui.add(egui::Label::new(RichText::new(label).font(FontId::proportional(12.0)).color(lbl_col)).sense(Sense::click())).clicked() {
            *checked = !*checked;
            toggled = true;
        }
    });
    toggled
}

fn paint_swatch_pill(ui: &mut egui::Ui, selected: bool, name: &str, rgb: [u8; 3]) -> bool {
    let pal = AppPalette::from_ui(ui);
    let swatch = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
    let fill = if selected { pal.selected } else { pal.inset };
    let border = if selected {
        Stroke::new(1.2_f32, pal.accent_blue)
    } else {
        Stroke::new(1.0_f32, pal.btn_secondary_stroke)
    };
    let text_w = name.len() as f32 * 6.5;
    let total_w = (26.0 + text_w).clamp(52.0, 140.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(total_w, 24.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let bg = if selected { fill } else if resp.hovered() { pal.hover } else { fill };
        ui.painter().rect_filled(rect, Rounding::same(4.0), bg);
        ui.painter().rect_stroke(rect, Rounding::same(4.0), border);
        let swatch_rect = egui::Rect::from_center_size(egui::pos2(rect.left() + 11.5, rect.center().y), Vec2::splat(9.5));
        ui.painter().rect_filled(swatch_rect, Rounding::same(2.0), swatch);
        ui.painter().rect_stroke(swatch_rect, Rounding::same(2.0), Stroke::new(0.8_f32, pal.divider));
        ui.painter().text(
            egui::pos2(rect.left() + 21.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            name,
            FontId::proportional(11.5),
            if selected { pal.selected_text } else { pal.text_primary },
        );
    }
    resp.clicked()
}

fn primary_action_button<'a>(text: &str, min_w: f32) -> egui::Button<'a> {
    egui::Button::new(RichText::new(text).font(FontId::proportional(13.0)).color(Color32::WHITE).strong())
        .fill(Color32::from_rgb(14, 108, 224))
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(38, 134, 248)))
        .rounding(Rounding::same(5.0))
        .min_size(Vec2::new(min_w, 33.0))
}

fn secondary_action_button<'a>(pal: &AppPalette, text: &str, color: Color32) -> egui::Button<'a> {
    egui::Button::new(RichText::new(text).font(FontId::proportional(12.0)).color(color))
        .fill(pal.inset)
        .stroke(Stroke::new(1.0_f32, pal.btn_secondary_stroke))
        .rounding(Rounding::same(4.0))
        .min_size(Vec2::new(0.0, 28.0))
}

fn choice_chip(ui: &mut egui::Ui, selected: bool, label: &str) -> bool {
    let pal = AppPalette::from_ui(ui);
    let fill = if selected { pal.chip_sel_bg } else { pal.inset };
    let fg = if selected { pal.chip_sel_fg } else { pal.text_secondary };
    let stroke = if selected {
        Stroke::new(1.0_f32, pal.chip_sel_stroke)
    } else {
        Stroke::new(1.0_f32, pal.btn_secondary_stroke)
    };
    let btn = egui::Button::new(RichText::new(label).font(FontId::proportional(12.0)).color(fg))
        .fill(fill)
        .stroke(stroke)
        .rounding(Rounding::same(4.0))
        .min_size(Vec2::new(0.0, 26.0));
    ui.add(btn).clicked()
}

fn sidebar_nav_row(
    ui: &mut egui::Ui,
    selected: bool,
    row_h: f32,
    label: &str,
    badge: Option<(&str, Color32)>,
) -> bool {
    let pal = AppPalette::from_ui(ui);
    let avail_w = ui.available_width().max(80.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(avail_w, row_h), Sense::click());
    if ui.is_rect_visible(rect) {
        let bg = if selected {
            pal.sidebar_sel_bg
        } else if resp.hovered() {
            pal.hover
        } else {
            Color32::TRANSPARENT
        };
        if bg != Color32::TRANSPARENT {
            ui.painter().rect_filled(rect, Rounding::same(4.0), bg);
            if selected {
                ui.painter().rect_stroke(rect, Rounding::same(4.0), Stroke::new(1.0_f32, pal.sidebar_sel_stroke));
            }
        }
        if selected {
            let bar = egui::Rect::from_min_size(
                egui::pos2(rect.left() + 2.0, rect.top() + 4.5),
                Vec2::new(3.0, (row_h - 9.0).max(8.0)),
            );
            ui.painter().rect_filled(bar, Rounding::same(1.0), pal.accent_blue);
        }
        let inner = rect.shrink2(Vec2::new(10.0, 2.0));
        let mut child_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(inner)
                .layout(Layout::left_to_right(Align::Center)),
        );
        child_ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
        child_ui.add(
            egui::Label::new(
                RichText::new(label)
                    .font(FontId::proportional(12.5))
                    .color(if selected { pal.selected_text } else { pal.text_primary }),
            )
            .truncate(),
        );
        if let Some((b_txt, b_col)) = badge {
            child_ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                rui.label(RichText::new(b_txt).font(FontId::proportional(10.5)).color(b_col));
            });
        }
    }
    resp.clicked()
}

fn clipped_row(
    ui: &mut egui::Ui,
    selected: bool,
    row_h: f32,
    icon: Option<egui::TextureId>,
    icon_sz: f32,
    title: &str,
    badge: Option<(&str, Color32)>,
) -> bool {
    let pal = AppPalette::from_ui(ui);
    let avail_w = ui.available_width().max(80.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(avail_w, row_h), Sense::click());
    if ui.is_rect_visible(rect) {
        let bg = if selected {
            pal.selected
        } else if resp.hovered() {
            pal.hover
        } else {
            Color32::TRANSPARENT
        };
        if bg != Color32::TRANSPARENT {
            ui.painter().rect_filled(rect, Rounding::same(4.0), bg);
        }
        if selected {
            ui.painter().rect_stroke(rect, Rounding::same(4.0), Stroke::new(1.0_f32, pal.selected_stroke));
        }

        let inner = rect.shrink2(Vec2::new(8.0, 3.0));
        let mut child_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(inner)
                .layout(Layout::left_to_right(Align::Center)),
        );
        child_ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);

        if icon_sz > 0.0 {
            let (ir, _) = child_ui.allocate_exact_size(Vec2::splat(icon_sz), Sense::hover());
            if let Some(tid) = icon {
                child_ui.painter().rect_filled(ir, Rounding::same(3.5), pal.thumb_bg);
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                child_ui.painter().image(tid, ir.shrink(1.5), uv, Color32::WHITE);
            } else {
                child_ui.painter().rect_filled(ir, Rounding::same(3.5), pal.inset);
                child_ui.painter().rect_stroke(ir, Rounding::same(3.5), Stroke::new(1.0_f32, pal.divider));
            }
            child_ui.add_space(6.0);
        }

        let badge_reserve = badge.as_ref().map(|(b, _)| b.len() as f32 * 6.8 + 14.0).unwrap_or(0.0);
        let text_w = (child_ui.available_width() - badge_reserve).max(40.0);
        child_ui.allocate_ui_with_layout(
            Vec2::new(text_w, inner.height()),
            Layout::left_to_right(Align::Center),
            |tui| {
                tui.add(
                    egui::Label::new(
                        RichText::new(title)
                            .font(FontId::proportional(12.6))
                            .color(if selected { pal.selected_text } else { pal.text_primary }),
                    )
                    .truncate(),
                );
            },
        );

        if let Some((b_txt, b_col)) = badge {
            child_ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                rui.label(RichText::new(b_txt).font(FontId::proportional(10.5)).color(b_col).strong());
            });
        }
    }
    if resp.hovered() && title.len() > 28 {
        resp.clone().on_hover_text(title);
    }
    resp.clicked()
}

fn styled_input_box(ui: &mut egui::Ui, buf: &mut String, hint: &str, show_search_icon: bool) -> bool {
    let pal = AppPalette::from_ui(ui);
    let mut changed = false;
    let avail = ui.available_width().max(120.0);
    egui::Frame::none()
        .fill(pal.inset)
        .stroke(Stroke::new(1.0_f32, pal.input_stroke))
        .rounding(Rounding::same(5.0))
        .inner_margin(egui::Margin::symmetric(10.0, 5.0))
        .show(ui, |ui| {
            ui.set_width((avail - 20.0).max(80.0));
            ui.horizontal(|ui| {
                if show_search_icon {
                    let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                    if ui.is_rect_visible(icon_rect) {
                        let c = icon_rect.center() - Vec2::splat(1.2);
                        let st = Stroke::new(1.35_f32, pal.text_muted);
                        ui.painter().circle_stroke(c, 4.2, st);
                        ui.painter().line_segment([c + Vec2::splat(3.0), c + Vec2::splat(6.2)], st);
                    }
                    ui.add_space(2.0);
                }
                let text_w = (ui.available_width() - if buf.is_empty() { 0.0 } else { 22.0 }).max(40.0);
                let te = egui::TextEdit::singleline(buf)
                    .hint_text(hint)
                    .frame(false)
                    .desired_width(text_w);
                if ui.add(te).changed() {
                    changed = true;
                }
                if !buf.is_empty() && ui.small_button("×").clicked() {
                    buf.clear();
                    changed = true;
                }
            });
        });
    changed
}

fn search_box(ui: &mut egui::Ui, buf: &mut String, hint: &str) -> bool {
    styled_input_box(ui, buf, hint, true)
}

fn detect_default_cooked_dir() -> String {
    let cands = [
        r"C:\Program Files\Epic Games\rocketleague\TAGame\CookedPCConsole",
        r"C:\Program Files (x86)\Steam\steamapps\common\rocketleague\TAGame\CookedPCConsole",
        r"D:\Epic Games\rocketleague\TAGame\CookedPCConsole",
        r"D:\SteamLibrary\steamapps\common\rocketleague\TAGame\CookedPCConsole",
    ];
    for c in cands {
        if Path::new(c).is_dir() {
            return c.to_string();
        }
    }
    cands[0].to_string()
}

fn presets_file_path() -> PathBuf {
    get_appdata_dir().join("presets_v4.json")
}

struct RLSwapperGui {
    cooked_dir: String,
    state: AppPersistentState,
    active_tab: ActiveTab,

    // Full Item Catalog
    all_catalog: Vec<CatalogItem>,
    items_by_slot: HashMap<String, Vec<CatalogItem>>,
    avatar_borders: Vec<(String, String)>,
    player_banners: Vec<(String, String)>,
    paints: Vec<PaintColorSpec>,
    selected_paint_idx: Option<usize>,
    custom_hex_color: [u8; 3],
    use_custom_hex: bool,

    // Cosmetic Slot Swapper State + Filtered Caches
    want_search: String,
    have_search: String,
    selected_want_idx: usize,
    selected_have_idx: usize,
    body_hitbox_filter: Option<CarHitbox>,
    cache_slot_key: (String, Option<CarHitbox>, String),
    cached_want_idxs: Vec<usize>,
    cache_have_key: (String, Option<CarHitbox>, String),
    cached_have_idxs: Vec<usize>,

    // Async Worker & Icons
    icon_mgr: Arc<IconCacheManager>,
    tx: Sender<WorkerEvent>,
    rx: Receiver<WorkerEvent>,
    gpu_icons: HashMap<String, TextureHandle>,
    pending_icons: HashSet<String>,
    is_busy: bool,
    status_text: String,
    status_is_error: bool,

    // Custom Decal Studio (AlphaConsole .ZIP/.JSON + 1:1 UV)
    decal_car_label: &'static str,
    decal_filter_car_only: bool,
    decal_target_idx: usize,
    decal_target_search: String,
    cache_decal_key: Option<(String, &'static str, bool)>,
    cached_decal_idxs: Vec<usize>,
    ac_decal_packs: Vec<AlphaConsoleDecalPack>,
    ac_decal_idx: usize,
    decal_custom_img: Option<RgbaImage>,
    decal_mask_img: Option<RgbaImage>,
    decal_custom_path: String,
    decal_primary_rgb: [u8; 3],
    decal_accent_rgb: [u8; 3],
    decal_preview_tex: Option<TextureHandle>,
    decal_preview_dirty: bool,

    // Custom PFP Studio
    pfp_border_idx: usize,
    pfp_search: String,
    cache_pfp_q: Option<String>,
    cached_pfp_idxs: Vec<usize>,
    pfp_custom_img: Option<RgbaImage>,
    pfp_composited_img: Option<RgbaImage>,
    pfp_custom_path: String,
    pfp_preview_tex: Option<TextureHandle>,
    pfp_preview_dirty: bool,

    // Custom Banner Studio
    banner_target_idx: usize,
    banner_search: String,
    cache_banner_q: Option<String>,
    cached_banner_idxs: Vec<usize>,
    banner_custom_img: Option<RgbaImage>,
    banner_custom_path: String,
    banner_player_name: String,
    banner_bake_name: bool,
    banner_preview_tex: Option<TextureHandle>,
    banner_preview_dirty: bool,

    // Custom Match Ball Studio (AlphaConsole Diffuse + Normal, NO Boost Pads)
    ac_ball_packs: Vec<AlphaConsoleBallPack>,
    ac_ball_idx: usize,
    ball_custom_img: Option<RgbaImage>,
    ball_normal_img: Option<RgbaImage>,
    ball_mask_img: Option<RgbaImage>,
    ball_custom_path: String,
    ball_orb_tex: Option<TextureHandle>,
    ball_flat_tex: Option<TextureHandle>,
    ball_norm_tex: Option<TextureHandle>,
    ball_orb_dirty: bool,

    // Pure Map Swapper (96 Arenas, Normal Swapping Only)
    map_donor_idx: usize,
    map_target_idx: usize,
    map_donor_search: String,
    map_target_search: String,
    map_filter_cat: usize,
    map_override_all_default: bool,
    cache_map_donor_key: Option<(String, usize)>,
    cached_map_donor_idxs: Vec<usize>,
    cache_map_target_key: Option<(String, usize)>,
    cached_map_target_idxs: Vec<usize>,
    map_donor_photo: (usize, Option<TextureHandle>),
    map_target_photo: (usize, Option<TextureHandle>),

    // Workshop Maps
    workshop_library: Vec<WorkshopMapEntry>,
    workshop_selected_idx: usize,
    workshop_missing_deps: Vec<String>,
    workshop_online_items: Vec<OnlineWorkshopItem>,
    workshop_online_loading: bool,
    workshop_online_search: String,
    workshop_direct_only: bool,
    cache_ws_key: (String, bool, usize),
    cached_ws_idxs: Vec<usize>,
    workshop_online_thumbs: HashMap<String, TextureHandle>,
    workshop_online_pending_thumbs: HashSet<String>,
    workshop_dl_progress: Option<WorkshopDownloadProgress>,

    // Presets & Health
    presets: Vec<LoadoutPreset>,
    new_preset_name: String,
    health_report: Option<CookedHealthReport>,
    show_reset_modal: bool,
    rl_running: bool,
    last_proc_check: Instant,
    #[allow(dead_code)]
    snap_state: Option<(PathBuf, String, usize, u8)>,
}

impl RLSwapperGui {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let state_path = get_appdata_dir().join("swapper_state_v4.json");
        let mut state: AppPersistentState = fs::read_to_string(&state_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        configure_app_themes(&cc.egui_ctx, state.theme_mode);

        if state.cooked_dir.is_empty() || !Path::new(&state.cooked_dir).is_dir() {
            state.cooked_dir = detect_default_cooked_dir();
        }
        let cooked_path = PathBuf::from(&state.cooked_dir);

        let all_catalog = load_catalog_items(Some(&cooked_path));
        let mut items_by_slot: HashMap<String, Vec<CatalogItem>> = HashMap::new();
        for it in &all_catalog {
            if !it.slot.is_empty() {
                items_by_slot.entry(it.slot.clone()).or_default().push(it.clone());
            }
        }
        for list in items_by_slot.values_mut() {
            list.sort_by(|a, b| a.product.to_lowercase().cmp(&b.product.to_lowercase()));
        }

        let avatar_borders = list_avatar_border_packages_with_catalog(&cooked_path, &all_catalog);
        let player_banners = list_player_banner_packages_with_catalog(&cooked_path, &all_catalog);

        let icon_mgr = Arc::new(IconCacheManager::new());
        icon_mgr.index_cooked_dir(&cooked_path);
        let (tx, rx) = mpsc::channel();

        let default_decal = generate_decal_template_for_car("Octane", Some(&cooked_path));
        let default_banner = generate_banner_template();
        let live_ball_tex = extract_ball_texture_preview(&cooked_path)
            .unwrap_or_else(|| generate_ball_template(None));

        let presets: Vec<LoadoutPreset> = fs::read_to_string(presets_file_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let workshop_library = list_library_maps();
        let health_report = Some(inspect_cooked_health(&cooked_path));

        let mut app = Self {
            cooked_dir: state.cooked_dir.clone(),
            state,
            active_tab: ActiveTab::CosmeticSlot("Body".into()),
            all_catalog,
            items_by_slot,
            avatar_borders,
            player_banners,
            paints: official_paints(),
            selected_paint_idx: None,
            custom_hex_color: [0, 229, 255],
            use_custom_hex: false,
            want_search: String::new(),
            have_search: String::new(),
            selected_want_idx: 0,
            selected_have_idx: 0,
            body_hitbox_filter: None,
            cache_slot_key: (String::new(), None, "\x01".into()),
            cached_want_idxs: Vec::new(),
            cache_have_key: (String::new(), None, "\x01".into()),
            cached_have_idxs: Vec::new(),
            icon_mgr,
            tx,
            rx,
            gpu_icons: HashMap::new(),
            pending_icons: HashSet::new(),
            is_busy: false,
            status_text: "Ready".into(),
            status_is_error: false,
            decal_car_label: "Octane",
            decal_filter_car_only: true,
            decal_target_idx: 0,
            decal_target_search: String::new(),
            cache_decal_key: None,
            cached_decal_idxs: Vec::new(),
            ac_decal_packs: Vec::new(),
            ac_decal_idx: 0,
            decal_custom_img: Some(default_decal),
            decal_mask_img: None,
            decal_custom_path: "Octane UV".into(),
            decal_primary_rgb: [30, 64, 175],
            decal_accent_rgb: [56, 189, 248],
            decal_preview_tex: None,
            decal_preview_dirty: true,
            pfp_border_idx: 0,
            pfp_search: String::new(),
            cache_pfp_q: None,
            cached_pfp_idxs: Vec::new(),
            pfp_custom_img: None,
            pfp_composited_img: None,
            pfp_custom_path: String::new(),
            pfp_preview_tex: None,
            pfp_preview_dirty: true,
            banner_target_idx: 0,
            banner_search: String::new(),
            cache_banner_q: None,
            cached_banner_idxs: Vec::new(),
            banner_custom_img: Some(default_banner),
            banner_custom_path: "Banner Template".into(),
            banner_player_name: String::new(),
            banner_bake_name: false,
            banner_preview_tex: None,
            banner_preview_dirty: true,
            ac_ball_packs: Vec::new(),
            ac_ball_idx: 0,
            ball_custom_img: Some(live_ball_tex),
            ball_normal_img: None,
            ball_mask_img: None,
            ball_custom_path: "Vanilla Match Ball".into(),
            ball_orb_tex: None,
            ball_flat_tex: None,
            ball_norm_tex: None,
            ball_orb_dirty: true,
            map_donor_idx: 0,
            map_target_idx: 1,
            map_donor_search: String::new(),
            map_target_search: String::new(),
            map_filter_cat: 0,
            map_override_all_default: false,
            cache_map_donor_key: None,
            cached_map_donor_idxs: Vec::new(),
            cache_map_target_key: None,
            cached_map_target_idxs: Vec::new(),
            map_donor_photo: (usize::MAX, None),
            map_target_photo: (usize::MAX, None),
            workshop_library,
            workshop_selected_idx: 0,
            workshop_missing_deps: Vec::new(),
            workshop_online_items: Vec::new(),
            workshop_online_loading: false,
            workshop_online_search: String::new(),
            workshop_direct_only: false,
            cache_ws_key: (String::new(), false, usize::MAX),
            cached_ws_idxs: Vec::new(),
            workshop_online_thumbs: HashMap::new(),
            workshop_online_pending_thumbs: HashSet::<String>::new(),
            workshop_dl_progress: None,
            presets,
            new_preset_name: "Clean Preset".into(),
            health_report,
            show_reset_modal: false,
            rl_running: is_rocket_league_running(),
            last_proc_check: Instant::now(),
            snap_state: std::env::var_os("RL_SWAPPER_SCREENSHOT_DIR").map(|d| {
                let pfx = std::env::var("RL_SWAPPER_SCREENSHOT_PREFIX").unwrap_or_else(|_| "ui".into());
                (PathBuf::from(d), pfx, 0usize, 0u8)
            }),
        };
        app.sync_rl_exe_patch_guard();
        app
    }

    fn save_state(&self) {
        let state_path = get_appdata_dir().join("swapper_state_v4.json");
        if let Ok(json) = serde_json::to_string_pretty(&self.state) {
            let _ = fs::write(state_path, json);
        }
    }

    fn save_presets(&self) {
        if let Ok(json) = serde_json::to_string_pretty(&self.presets) {
            let _ = fs::write(presets_file_path(), json);
        }
    }

    fn refresh_health_report(&mut self) {
        self.health_report = Some(inspect_cooked_health(Path::new(&self.cooked_dir)));
    }

    fn invalidate_caches(&mut self) {
        self.cache_slot_key.2.push('!');
        self.cache_have_key.2.push('!');
        self.cache_decal_key = None;
        self.cache_pfp_q = None;
        self.cache_banner_q = None;
        self.cache_ws_key.2 = usize::MAX;
    }

    fn sync_rl_exe_patch_guard(&mut self) {
        let cooked = Path::new(&self.cooked_dir);
        let Some(rl_exe) = cooked
            .parent()
            .and_then(|p| p.parent())
            .map(|r| r.join("Binaries").join("Win64").join("RocketLeague.exe"))
        else {
            return;
        };
        let Ok(meta) = rl_exe.metadata() else { return };
        let cur_size = meta.len();
        let cur_mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        if self.state.rl_exe_size != 0
            && (self.state.rl_exe_size != cur_size || self.state.rl_exe_mtime != cur_mtime)
        {
            for rec in self.state.active_swaps.values() {
                restore_single_package(&cooked.join(&rec.target_pkg));
            }
            self.state.active_swaps.clear();
            self.state.swap_history.clear();
            self.status_text = "Game updated — swaps cleared safely".into();
        }
        self.state.rl_exe_size = cur_size;
        self.state.rl_exe_mtime = cur_mtime;
        self.save_state();
    }

    fn is_pkg_swapped(&self, pkg_name: &str) -> bool {
        let low = pkg_name.to_lowercase();
        if self.state.active_swaps.contains_key(&low) {
            return true;
        }
        let p = Path::new(&self.cooked_dir).join(pkg_name);
        if bak_path_for(&p).exists() {
            return true;
        }
        if low.starts_with("avatarborder_") && is_avatar_border_modified(&p) {
            return true;
        }
        false
    }

    fn active_paint_spec(&self) -> Option<PaintColorSpec> {
        if self.use_custom_hex {
            let hex = format!(
                "#{:02X}{:02X}{:02X}",
                self.custom_hex_color[0], self.custom_hex_color[1], self.custom_hex_color[2]
            );
            Some(PaintColorSpec::from_custom_hex(&hex))
        } else {
            self.selected_paint_idx.and_then(|i| self.paints.get(i).cloned())
        }
    }

    fn request_item_icon(&mut self, item: &CatalogItem, paint: Option<&PaintColorSpec>, sz: u32) -> String {
        let base_key = format!("{}_{}", IconCacheManager::cache_key(item), sz);
        let full_key = match paint {
            Some(p) => format!("{}_{}", base_key, p.hex_srgb),
            None => base_key,
        };
        if self.gpu_icons.contains_key(&full_key) || self.pending_icons.contains(&full_key) {
            return full_key;
        }
        self.pending_icons.insert(full_key.clone());
        let mgr = Arc::clone(&self.icon_mgr);
        let cooked = PathBuf::from(&self.cooked_dir);
        let item_cl = item.clone();
        let paint_cl = paint.cloned();
        let tx = self.tx.clone();
        let fk = full_key.clone();

        thread::spawn(move || {
            let (base_im, _) = mgr.get_icon_sync(&cooked, &item_cl, sz >= 48);
            let comp = composite_icon_with_paint(&base_im, paint_cl.as_ref(), sz);
            let _ = tx.send(WorkerEvent::IconReady {
                cache_key: fk,
                rgba: comp,
            });
        });
        full_key
    }

    fn record_swap(&mut self, rec: ActiveSwapRecord) {
        let k = rec.target_pkg.to_lowercase();
        self.state.active_swaps.insert(k.clone(), rec);
        self.state.swap_history.retain(|x| x != &k);
        self.state.swap_history.push(k);
        self.save_state();
    }

    fn undo_last_swap(&mut self) {
        while let Some(last_key) = self.state.swap_history.pop() {
            if let Some(rec) = self.state.active_swaps.remove(&last_key) {
                restore_single_package(&Path::new(&self.cooked_dir).join(&rec.target_pkg));
                clear_rl_texture_cache();
                self.save_state();
                self.refresh_health_report();
                self.status_is_error = false;
                self.status_text = format!("Restored {}", rec.target_desc);
                return;
            }
        }
        self.status_text = "Nothing to undo".into();
    }

    fn reset_all_swaps(&mut self) {
        let cdir = PathBuf::from(&self.cooked_dir);
        for rec in self.state.active_swaps.values() {
            restore_single_package(&cdir.join(&rec.target_pkg));
        }
        if let Some(ref rep) = self.health_report {
            for pkg in &rep.bak_filenames {
                restore_single_package(&cdir.join(pkg));
            }
        }
        clear_rl_texture_cache();
        self.state.active_swaps.clear();
        self.state.swap_history.clear();
        self.save_state();
        self.refresh_health_report();
        self.status_is_error = false;
        self.status_text = "Restored all files to vanilla".into();
    }

    fn ensure_decal_target_cache(&mut self) {
        let q_low = self.decal_target_search.to_lowercase();
        let key = (q_low.clone(), self.decal_car_label, self.decal_filter_car_only);
        if self.cache_decal_key.as_ref() == Some(&key) {
            return;
        }
        let mut matches: Vec<(u8, usize)> = self
            .items_by_slot
            .get("Decal")
            .map(|list| {
                list.iter()
                    .enumerate()
                    .filter(|(_, d)| {
                        let car_ok = !self.decal_filter_car_only
                            || detect_car_uv_label(&format!("{} {}", d.product, d.asset_package))
                                == Some(self.decal_car_label);
                        let q_ok = q_low.is_empty()
                            || d.product.to_lowercase().contains(&q_low)
                            || d.asset_package.to_lowercase().contains(&q_low);
                        car_ok && q_ok
                    })
                    .map(|(i, d)| {
                        let pkg_low = d.asset_package.to_lowercase();
                        let prod_low = d.product.to_lowercase();
                        // Prioritize starter owned decals (Stripes first) at top when browsing a car
                        let tier = if pkg_low.ends_with("_stripes_sf.upk") {
                            0u8
                        } else if prod_low.ends_with(": stripes")
                            || prod_low.ends_with(": flames")
                            || prod_low.ends_with(": lightning")
                            || prod_low.ends_with(": stars")
                            || prod_low.ends_with(": wings")
                            || prod_low.ends_with(": tech")
                            || prod_low.ends_with(": skulls")
                        {
                            1u8
                        } else if pkg_low.starts_with("skin_") {
                            2u8
                        } else {
                            3u8
                        };
                        (tier, i)
                    })
                    .collect()
            })
            .unwrap_or_default();
        matches.sort_by_key(|&(tier, idx)| (tier, idx));
        self.cached_decal_idxs = matches.into_iter().map(|(_, i)| i).collect();
        self.cache_decal_key = Some(key);
    }

    fn apply_selected_ac_decal_pack(&mut self) {
        if let Some(pack) = self.ac_decal_packs.get(self.ac_decal_idx).cloned() {
            // For legacy Diffuse+Skin packs without 2_Diffuse_Skin_Mask (e.g. Violent Octane), show the
            // full body diffuse in the studio preview rather than the raw black-background Skin overlay.
            self.decal_custom_img = if pack.mask_img.is_none() && pack.diffuse_base_img.is_some() {
                pack.diffuse_base_img.or(pack.skin_img)
            } else {
                pack.skin_img.or(pack.diffuse_base_img)
            };
            self.decal_mask_img = pack.mask_img;
            self.decal_custom_path = format!("{} ({})", pack.name, pack.source_label);

            if let Some(bid) = pack.body_id {
                if let Some(body_item) = find_catalog_item_by_id(&self.all_catalog, bid) {
                    if let Some(lbl) = detect_car_uv_label(&format!("{} {}", body_item.product, body_item.asset_package)) {
                        self.decal_car_label = lbl;
                    }
                }
            }
            if let Some(sid) = pack.skin_id {
                if let Some(skin_item) = find_catalog_item_by_id(&self.all_catalog, sid) {
                    if let Some(lbl) = detect_car_uv_label(&format!("{} {}", skin_item.product, skin_item.asset_package)) {
                        self.decal_car_label = lbl;
                    }
                    self.decal_target_search.clear();
                    self.cache_decal_key = None;
                    self.ensure_decal_target_cache();
                    let decal_list = self.items_by_slot.get("Decal");
                    if let Some(pos) = self.cached_decal_idxs.iter().position(|&raw_i| {
                        decal_list.and_then(|l| l.get(raw_i)).map_or(false, |d| {
                            d.id == sid || d.asset_package.eq_ignore_ascii_case(&skin_item.asset_package)
                        })
                    }) {
                        self.decal_target_idx = pos;
                    } else {
                        self.decal_target_idx = 0;
                    }
                }
            } else {
                self.cache_decal_key = None;
                self.ensure_decal_target_cache();
                self.decal_target_idx = 0;
            }
            self.decal_preview_dirty = true;
            self.status_is_error = false;
            self.status_text = format!("Loaded decal '{}'", pack.name);
        }
    }

    fn apply_selected_ac_ball_pack(&mut self) {
        if let Some(pack) = self.ac_ball_packs.get(self.ac_ball_idx).cloned() {
            self.ball_custom_img = Some(pack.diffuse_img);
            self.ball_normal_img = pack.normal_img;
            self.ball_mask_img = pack.mask_img;
            self.ball_custom_path = format!("{} ({})", pack.name, pack.source_label);
            self.ball_orb_dirty = true;
            self.status_is_error = false;
            self.status_text = format!("Loaded ball '{}'", pack.name);
        }
    }

    fn import_decal_file(&mut self, p: &Path) {
        match parse_alphaconsole_decal_source(p) {
            Ok(packs) => {
                self.ac_decal_packs = packs;
                self.ac_decal_idx = 0;
                self.apply_selected_ac_decal_pack();
            }
            Err(e) => {
                self.status_is_error = true;
                self.status_text = e;
            }
        }
    }

    fn import_ball_file(&mut self, p: &Path) {
        match parse_alphaconsole_ball_source(p) {
            Ok(packs) => {
                self.ac_ball_packs = packs;
                self.ac_ball_idx = 0;
                self.apply_selected_ac_ball_pack();
            }
            Err(e) => {
                self.status_is_error = true;
                self.status_text = e;
            }
        }
    }
}

impl eframe::App for RLSwapperGui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        apply_theme_preference(ctx, self.state.theme_mode);
        let pal = AppPalette::from_dark(ctx.theme() == egui::Theme::Dark);

        // Drain worker events
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                WorkerEvent::IconReady { cache_key, rgba } => {
                    self.pending_icons.remove(&cache_key);
                    let tex = ctx.load_texture(&cache_key, rgba_to_egui_image(&rgba), TextureOptions::LINEAR);
                    self.gpu_icons.insert(cache_key, tex);
                }
                WorkerEvent::CategorySyncDone { slot, local, web } => {
                    self.is_busy = false;
                    self.status_is_error = false;
                    self.gpu_icons.clear();
                    self.pending_icons.clear();
                    self.status_text = format!("Synced {}: {} local · {} online", slot, local, web);
                }
                WorkerEvent::WorkshopCatalogDone(res) => {
                    self.workshop_online_loading = false;
                    match res {
                        Ok(items) => {
                            let cnt = items.len();
                            self.workshop_online_items = items;
                            self.cache_ws_key.2 = usize::MAX;
                            self.status_is_error = false;
                            self.status_text = format!("{} online maps ready", cnt);
                        }
                        Err(e) => {
                            self.status_is_error = true;
                            self.status_text = e;
                        }
                    }
                }
                WorkerEvent::WorkshopThumbReady { url, rgba } => {
                    self.workshop_online_pending_thumbs.remove(&url);
                    let tex = ctx.load_texture(&format!("ws_{}", url), rgba_to_egui_image(&rgba), TextureOptions::LINEAR);
                    self.workshop_online_thumbs.insert(url, tex);
                }
                WorkerEvent::WorkshopDownloadProgress(prog) => {
                    self.workshop_dl_progress = Some(prog);
                }
                WorkerEvent::WorkshopDownloadDone(res) => {
                    self.is_busy = false;
                    self.workshop_dl_progress = None;
                    match res {
                        Ok(entry) => {
                            self.workshop_library = list_library_maps();
                            if let Some(pos) = self.workshop_library.iter().position(|e| e.id == entry.id) {
                                self.workshop_selected_idx = pos;
                            }
                            self.status_is_error = false;
                            self.status_text = format!("Downloaded '{}'", entry.name);
                        }
                        Err(e) => {
                            self.status_is_error = true;
                            self.status_text = e;
                        }
                    }
                }
                WorkerEvent::OperationDone { success, message, swapped_pkgs, cleared_pkgs } => {
                    self.is_busy = false;
                    self.status_is_error = !success;
                    self.status_text = message;
                    for pkg in cleared_pkgs {
                        self.state.active_swaps.remove(&pkg.to_lowercase());
                    }
                    for rec in swapped_pkgs {
                        self.record_swap(rec);
                    }
                    self.save_state();
                    self.refresh_health_report();
                }
            }
        }

        if self.last_proc_check.elapsed() > Duration::from_secs(5) {
            self.rl_running = is_rocket_league_running();
            self.last_proc_check = Instant::now();
        }

        // Bottom minimal status bar (strictly truncated so long paths/errors never expand window)
        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(30.0)
            .frame(egui::Frame::none().fill(pal.sidebar).stroke(Stroke::new(1.0_f32, pal.divider)).inner_margin(egui::Margin::symmetric(16.0, 5.0)))
            .show(ctx, |ui| {
                ui.set_max_width(ui.available_width());
                ui.horizontal(|ui| {
                    let dot_col = if self.is_busy {
                        pal.accent_blue
                    } else if self.status_is_error {
                        pal.accent_red
                    } else {
                        pal.accent_green
                    };
                    paint_status_dot(ui, dot_col, 3.6);
                    let col = if self.status_is_error { pal.accent_red } else { pal.text_secondary };
                    ui.add(egui::Label::new(RichText::new(&self.status_text).font(FontId::proportional(11.8)).color(col)).truncate());
                });
            });

        // Left fixed sidebar with pinned bottom action dock (all tabs fit cleanly without scroll-off)
        egui::SidePanel::left("nav_sidebar")
            .exact_width(216.0)
            .resizable(false)
            .frame(egui::Frame::none().fill(pal.sidebar).stroke(Stroke::new(1.0_f32, pal.divider)).inner_margin(egui::Margin::symmetric(12.0, 12.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("RL Swapper").font(FontId::proportional(17.0)).color(pal.text_primary).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let (lbl, col) = if self.rl_running { ("Live", pal.accent_amber) } else { ("Idle", pal.accent_green) };
                        ui.label(RichText::new(lbl).font(FontId::proportional(11.0)).color(col).strong());
                        paint_status_dot(ui, col, 3.3);
                    });
                });
                ui.add_space(8.0);

                let swap_cnt = self
                    .health_report
                    .as_ref()
                    .map(|r| r.total_bak_files.max(self.state.active_swaps.len()))
                    .unwrap_or(self.state.active_swaps.len());

                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    ui.set_width(192.0);
                    if ui
                        .add(secondary_action_button(&pal, "Game Folder", pal.text_secondary).min_size(Vec2::new(192.0, 26.0)))
                        .clicked()
                    {
                        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                            self.cooked_dir = folder.to_string_lossy().to_string();
                            self.state.cooked_dir = self.cooked_dir.clone();
                            self.icon_mgr.index_cooked_dir(&folder);
                            self.avatar_borders = list_avatar_border_packages_with_catalog(&folder, &self.all_catalog);
                            self.player_banners = list_player_banner_packages_with_catalog(&folder, &self.all_catalog);
                            self.invalidate_caches();
                            self.refresh_health_report();
                            self.save_state();
                        }
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                !self.state.active_swaps.is_empty() && !self.is_busy,
                                secondary_action_button(&pal, "Undo", pal.text_primary).min_size(Vec2::new(92.0, 26.0)),
                            )
                            .clicked()
                        {
                            self.undo_last_swap();
                        }
                        if ui
                            .add(
                                secondary_action_button(
                                    &pal,
                                    &format!("Reset ({})", swap_cnt),
                                    if swap_cnt > 0 { pal.accent_red } else { pal.text_muted },
                                )
                                .min_size(Vec2::new(92.0, 26.0)),
                            )
                            .clicked()
                        {
                            self.show_reset_modal = true;
                        }
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for (tm, lbl) in [(ThemeMode::Auto, "Auto"), (ThemeMode::Dark, "Dark"), (ThemeMode::Light, "Light")] {
                            let is_sel = self.state.theme_mode == tm;
                            let fill = if is_sel { pal.chip_sel_bg } else { pal.inset };
                            let fg = if is_sel { pal.chip_sel_fg } else { pal.text_secondary };
                            let st = if is_sel { pal.chip_sel_stroke } else { pal.btn_secondary_stroke };
                            let b = egui::Button::new(RichText::new(lbl).font(FontId::proportional(11.0)).color(fg))
                                .fill(fill)
                                .stroke(Stroke::new(1.0_f32, st))
                                .rounding(Rounding::same(4.0))
                                .min_size(Vec2::new(61.0, 23.0));
                            if ui.add(b).clicked() {
                                self.state.theme_mode = tm;
                                apply_theme_preference(ctx, tm);
                                self.save_state();
                            }
                        }
                    });
                    ui.add_space(5.0);
                    ui.separator();

                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("nav_scroll")
                            .auto_shrink([false, false])
                            .max_height(ui.available_height().max(120.0))
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing.y = 1.5;
                                ui.label(RichText::new("ITEMS").font(FontId::proportional(9.5)).color(pal.text_muted).strong());
                                ui.add_space(2.0);

                                for &slot in ORDERED_SLOTS {
                                    let is_sel = self.active_tab == ActiveTab::CosmeticSlot(slot.to_string());
                                    let cnt = self.items_by_slot.get(slot).map(|v| v.len().to_string()).unwrap_or_default();
                                    if sidebar_nav_row(ui, is_sel, 22.0, slot, Some((&cnt, pal.text_muted))) {
                                        self.active_tab = ActiveTab::CosmeticSlot(slot.to_string());
                                        self.selected_want_idx = 0;
                                        self.selected_have_idx = 0;
                                    }
                                }

                                ui.add_space(7.0);
                                ui.label(RichText::new("STUDIO").font(FontId::proportional(9.5)).color(pal.text_muted).strong());
                                ui.add_space(2.0);

                                for (tab_val, label) in [
                                    (ActiveTab::CustomDecal, "Decals"),
                                    (ActiveTab::CustomPfp, "Avatar PFP"),
                                    (ActiveTab::CustomBanner, "Banner"),
                                    (ActiveTab::CustomBall, "Match Ball"),
                                    (ActiveTab::MapSwapper, "Maps"),
                                    (ActiveTab::WorkshopMaps, "Workshop"),
                                ] {
                                    let is_sel = self.active_tab == tab_val;
                                    if sidebar_nav_row(ui, is_sel, 23.0, label, None) {
                                        self.active_tab = tab_val;
                                    }
                                }

                                ui.add_space(7.0);
                                ui.label(RichText::new("SYSTEM").font(FontId::proportional(9.5)).color(pal.text_muted).strong());
                                ui.add_space(2.0);

                                let p_cnt = self.presets.len().to_string();
                                let s_cnt = swap_cnt.to_string();

                                if sidebar_nav_row(ui, self.active_tab == ActiveTab::Presets, 22.5, "Presets", Some((&p_cnt, pal.text_muted))) {
                                    self.active_tab = ActiveTab::Presets;
                                }
                                let h_col = if swap_cnt > 0 { pal.accent_green } else { pal.text_muted };
                                if sidebar_nav_row(ui, self.active_tab == ActiveTab::ActiveHealth, 22.5, "Health", Some((&s_cnt, h_col))) {
                                    self.refresh_health_report();
                                    self.active_tab = ActiveTab::ActiveHealth;
                                }
                            });
                    });
                });
            });

        // Main Adaptive Canvas
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(pal.canvas).inner_margin(egui::Margin::symmetric(20.0, 16.0)))
            .show(ctx, |ui| {
                ui.set_max_width(ui.available_width());
                match self.active_tab.clone() {
                    ActiveTab::CosmeticSlot(slot) => self.render_cosmetic_slot_view(ui, &slot),
                    ActiveTab::CustomDecal => self.render_custom_decal_studio(ctx, ui),
                    ActiveTab::CustomPfp => self.render_custom_pfp_studio(ctx, ui),
                    ActiveTab::CustomBanner => self.render_custom_banner_studio(ctx, ui),
                    ActiveTab::CustomBall => self.render_custom_ball_studio(ctx, ui),
                    ActiveTab::MapSwapper => self.render_map_swapper(ctx, ui),
                    ActiveTab::WorkshopMaps => self.render_workshop_maps_studio(ctx, ui),
                    ActiveTab::Presets => self.render_presets_tab(ui),
                    ActiveTab::ActiveHealth => self.render_active_health_tab(ui),
                }
            });

        if self.show_reset_modal {
            self.render_reset_modal(ctx);
        }

        self.handle_env_screenshot(ctx);
    }
}

impl RLSwapperGui {
    fn handle_env_screenshot(&mut self, ctx: &egui::Context) {
        let Some((dir, pfx, idx, phase)) = self.snap_state.clone() else {
            return;
        };
        let shots: [(&str, ActiveTab); 7] = [
            ("body", ActiveTab::CosmeticSlot("Body".into())),
            ("decal", ActiveTab::CustomDecal),
            ("pfp", ActiveTab::CustomPfp),
            ("banner", ActiveTab::CustomBanner),
            ("ball", ActiveTab::CustomBall),
            ("maps", ActiveTab::MapSwapper),
            ("workshop", ActiveTab::WorkshopMaps),
        ];
        let mut saved_now = false;
        ctx.input(|i| {
            for ev in &i.events {
                if let egui::Event::Screenshot { image, .. } = ev {
                    let _ = fs::create_dir_all(&dir);
                    if let Some((name, _)) = shots.get(idx) {
                        let [w, h] = image.size;
                        if let Some(rgba) = RgbaImage::from_raw(w as u32, h as u32, image.as_raw().to_vec()) {
                            let _ = rgba.save(dir.join(format!("{}_{}.png", pfx, name)));
                        }
                    }
                    saved_now = true;
                }
            }
        });
        if let Some(ref mut st) = self.snap_state {
            if saved_now {
                if st.2 + 1 >= shots.len() {
                    self.snap_state = None;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                } else {
                    st.2 += 1;
                    st.3 = 0;
                    self.active_tab = shots[st.2].1.clone();
                }
            } else if phase < 6 {
                st.3 = phase + 1;
                self.active_tab = shots[idx].1.clone();
            } else if phase == 6 {
                st.3 = 7;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
            }
            ctx.request_repaint();
        }
    }
    // ── 1. Cosmetic Item Swapper ─────────────────────────────────────────────
    fn render_cosmetic_slot_view(&mut self, ui: &mut egui::Ui, slot: &str) {
        let pal = AppPalette::from_ui(ui);
        let is_body_slot = slot == "Body";
        let hb_filter = if is_body_slot { self.body_hitbox_filter } else { None };

        let want_key = (slot.to_string(), hb_filter, self.want_search.to_lowercase());
        if self.cache_slot_key != want_key {
            let q = &want_key.2;
            self.cached_want_idxs = self
                .items_by_slot
                .get(slot)
                .map(|list| {
                    list.iter()
                        .enumerate()
                        .filter(|(_, it)| {
                            (q.is_empty()
                                || it.product.to_lowercase().contains(q)
                                || it.asset_package.to_lowercase().contains(q))
                                && hb_filter.map_or(true, |hb| body_hitbox_for_item(&it.product, &it.asset_package) == hb)
                        })
                        .map(|(i, _)| i)
                        .collect()
                })
                .unwrap_or_default();
            self.cache_slot_key = want_key;
        }

        let have_key = (slot.to_string(), hb_filter, self.have_search.to_lowercase());
        if self.cache_have_key != have_key {
            let q = &have_key.2;
            self.cached_have_idxs = self
                .items_by_slot
                .get(slot)
                .map(|list| {
                    list.iter()
                        .enumerate()
                        .filter(|(_, it)| {
                            (q.is_empty()
                                || it.product.to_lowercase().contains(q)
                                || it.asset_package.to_lowercase().contains(q))
                                && hb_filter.map_or(true, |hb| body_hitbox_for_item(&it.product, &it.asset_package) == hb)
                        })
                        .map(|(i, _)| i)
                        .collect()
                })
                .unwrap_or_default();
            self.cache_have_key = have_key;
        }

        let want_len = self.cached_want_idxs.len();
        let have_len = self.cached_have_idxs.len();
        if self.selected_want_idx >= want_len {
            self.selected_want_idx = 0;
        }
        if self.selected_have_idx >= have_len {
            self.selected_have_idx = 0;
        }

        let want_item = self
            .cached_want_idxs
            .get(self.selected_want_idx)
            .and_then(|&i| self.items_by_slot.get(slot).and_then(|v| v.get(i)))
            .cloned();
        let have_item = self
            .cached_have_idxs
            .get(self.selected_have_idx)
            .and_then(|&i| self.items_by_slot.get(slot).and_then(|v| v.get(i)))
            .cloned();
        let paint_spec = self.active_paint_spec();

        ui.horizontal(|ui| {
            ui.label(RichText::new(slot).font(FontId::proportional(20.0)).color(pal.text_primary).strong());
            ui.add_space(6.0);
            if ui.add_enabled(!self.is_busy, secondary_action_button(&pal, "Sync Icons", pal.accent_blue)).clicked() {
                self.is_busy = true;
                self.status_text = format!("Syncing {} icons...", slot);
                let mgr = Arc::clone(&self.icon_mgr);
                let cdir = PathBuf::from(&self.cooked_dir);
                let slot_str = slot.to_string();
                let list_cl = self.items_by_slot.get(slot).cloned().unwrap_or_default();
                let tx = self.tx.clone();
                thread::spawn(move || {
                    let (l, w, _) = mgr.bulk_sync_category(&cdir, &list_cl, true);
                    let _ = tx.send(WorkerEvent::CategorySyncDone { slot: slot_str, local: l, web: w });
                });
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let mut launch_pair: Option<(CatalogItem, CatalogItem)> = None;
                let btn_lbl = if self.is_busy { "Working..." } else { "Swap Item" };
                if ui
                    .add_enabled(!self.is_busy && want_item.is_some() && have_item.is_some(), primary_action_button(btn_lbl, 118.0))
                    .clicked()
                {
                    if let (Some(w), Some(h)) = (want_item.clone(), have_item.clone()) {
                        launch_pair = Some((w, h));
                    }
                }
                if paint_spec.is_some() {
                    if let Some(target_only) = have_item.clone().or_else(|| want_item.clone()) {
                        if ui.add_enabled(!self.is_busy, secondary_action_button(&pal, "Paint Only", pal.accent_green)).clicked() {
                            launch_pair = Some((target_only.clone(), target_only));
                        }
                    }
                }
                if let Some((w_it, h_it)) = launch_pair {
                    self.is_busy = true;
                    let cdir = PathBuf::from(&self.cooked_dir);
                    let p_spec = self.active_paint_spec();
                    let tx = self.tx.clone();
                    let slot_owned = slot.to_string();
                    thread::spawn(move || match swap_item_packages(&cdir, &w_it, &h_it, p_spec.as_ref()) {
                        Ok(msg) => {
                            let rec = ActiveSwapRecord {
                                target_pkg: h_it.asset_package.clone(),
                                donor_desc: w_it.product.clone(),
                                target_desc: h_it.product.clone(),
                                kind: if p_spec.is_some() { "PAINT".into() } else { "ITEM".into() },
                                slot: slot_owned,
                                donor_pkg: w_it.asset_package.clone(),
                                donor_asset_path: w_it.asset_path.clone(),
                                target_asset_path: h_it.asset_path.clone(),
                                paint_hex: p_spec.as_ref().map(|p| p.hex_srgb.clone()),
                                paint_name: p_spec.as_ref().map(|p| p.name.clone()),
                            };
                            let _ = tx.send(WorkerEvent::OperationDone {
                                success: true,
                                message: msg,
                                swapped_pkgs: vec![rec],
                                cleared_pkgs: vec![],
                            });
                        }
                        Err(e) => {
                            let _ = tx.send(WorkerEvent::OperationDone {
                                success: false,
                                message: e,
                                swapped_pkgs: vec![],
                                cleared_pkgs: vec![],
                            });
                        }
                    });
                }
            });
        });

        if is_body_slot {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                if choice_chip(ui, self.body_hitbox_filter.is_none(), "All") {
                    self.body_hitbox_filter = None;
                }
                for hb in [
                    CarHitbox::Octane,
                    CarHitbox::Dominus,
                    CarHitbox::Hybrid,
                    CarHitbox::Breakout,
                    CarHitbox::Plank,
                    CarHitbox::Merc,
                ] {
                    if choice_chip(ui, self.body_hitbox_filter == Some(hb), hb.label()) {
                        self.body_hitbox_filter = Some(hb);
                    }
                }
            });
        }

        ui.add_space(5.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
            let def_sel = self.selected_paint_idx.is_none() && !self.use_custom_hex;
            if choice_chip(ui, def_sel, "Default") {
                self.selected_paint_idx = None;
                self.use_custom_hex = false;
            }
            for (idx, p) in self.paints.iter().enumerate() {
                let is_sel = !self.use_custom_hex && self.selected_paint_idx == Some(idx);
                if paint_swatch_pill(ui, is_sel, &p.name, p.rgb8()) {
                    self.selected_paint_idx = Some(idx);
                    self.use_custom_hex = false;
                }
            }
            if ui.color_edit_button_srgb(&mut self.custom_hex_color).changed() {
                self.use_custom_hex = true;
            }
            if self.use_custom_hex {
                let [r, g, b] = self.custom_hex_color;
                ui.label(
                    RichText::new(format!("#{:02X}{:02X}{:02X}", r, g, b))
                        .font(FontId::monospace(11.0))
                        .color(Color32::from_rgb(r, g, b)),
                );
            }
        });

        ui.add_space(8.0);
        if want_item.is_some() || have_item.is_some() {
            app_card_full(ui, |ui| {
                let avail_w = ui.available_width();
                ui.horizontal(|ui| {
                    let side_w = ((avail_w - 52.0) * 0.5).max(160.0);
                    ui.allocate_ui_with_layout(Vec2::new(side_w, 56.0), Layout::left_to_right(Align::Center), |lui| {
                        lui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                        if let Some(ref w) = want_item {
                            let wk = self.request_item_icon(w, paint_spec.as_ref(), 64);
                            let (ir, _) = lui.allocate_exact_size(Vec2::splat(52.0), Sense::hover());
                            lui.painter().rect_filled(ir, Rounding::same(4.0), pal.thumb_bg);
                            lui.painter().rect_stroke(ir, Rounding::same(4.0), Stroke::new(1.0_f32, pal.divider));
                            if let Some(tex) = self.gpu_icons.get(&wk) {
                                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                                lui.painter().image(tex.id(), ir.shrink(2.0), uv, Color32::WHITE);
                            }
                            lui.add_space(8.0);
                            lui.vertical(|vui| {
                                vui.label(RichText::new("DONOR").font(FontId::proportional(10.0)).color(pal.accent_blue).strong());
                                vui.add(egui::Label::new(RichText::new(&w.product).font(FontId::proportional(13.5)).color(pal.text_primary).strong()).truncate());
                                if let Some(ref p) = paint_spec {
                                    vui.add(egui::Label::new(RichText::new(&p.name).font(FontId::proportional(11.0)).color(pal.text_secondary)).truncate());
                                }
                            });
                        }
                    });
                    paint_arrow_badge(ui, 30.0);
                    ui.allocate_ui_with_layout(Vec2::new(side_w, 56.0), Layout::left_to_right(Align::Center), |rui| {
                        rui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                        if let Some(ref h) = have_item {
                            let hk = self.request_item_icon(h, None, 64);
                            let (ir, _) = rui.allocate_exact_size(Vec2::splat(52.0), Sense::hover());
                            rui.painter().rect_filled(ir, Rounding::same(4.0), pal.thumb_bg);
                            rui.painter().rect_stroke(ir, Rounding::same(4.0), Stroke::new(1.0_f32, pal.divider));
                            if let Some(tex) = self.gpu_icons.get(&hk) {
                                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                                rui.painter().image(tex.id(), ir.shrink(2.0), uv, Color32::WHITE);
                            }
                            rui.add_space(8.0);
                            rui.vertical(|vui| {
                                vui.label(RichText::new("TARGET").font(FontId::proportional(10.0)).color(pal.accent_green).strong());
                                vui.add(egui::Label::new(RichText::new(&h.product).font(FontId::proportional(13.5)).color(pal.text_primary).strong()).truncate());
                                if is_body_slot {
                                    if let Some(ref w) = want_item {
                                        let wh = body_hitbox_for_item(&w.product, &w.asset_package);
                                        let hh = body_hitbox_for_item(&h.product, &h.asset_package);
                                        let (ht, hc) = if wh == hh {
                                            (format!("{} match", hh.label()), pal.accent_green)
                                        } else {
                                            (format!("{} physics", hh.label()), pal.accent_amber)
                                        };
                                        vui.label(RichText::new(ht).font(FontId::proportional(11.0)).color(hc));
                                    }
                                }
                            });
                        }
                    });
                });
            });
        }

        ui.add_space(8.0);
        let want_idxs = self.cached_want_idxs.clone();
        let have_idxs = self.cached_have_idxs.clone();
        ui.columns(2, |cols| {
            self.render_item_picker_col(&mut cols[0], slot, true, is_body_slot, &want_idxs);
            self.render_item_picker_col(&mut cols[1], slot, false, is_body_slot, &have_idxs);
        });
    }

    fn render_item_picker_col(
        &mut self,
        col: &mut egui::Ui,
        slot: &str,
        is_donor: bool,
        is_body_slot: bool,
        indices: &[usize],
    ) {
        let pal = AppPalette::from_ui(col);
        if is_donor {
            search_box(col, &mut self.want_search, "Donor...");
        } else {
            search_box(col, &mut self.have_search, "Target...");
        }
        col.add_space(6.0);
        let avail_h = col.available_height().max(220.0);
        let list_opt = self.items_by_slot.get(slot).cloned();
        app_list_card(&pal).show(col, |ui| {
            egui::ScrollArea::vertical()
                .id_salt(if is_donor { "want_scroll" } else { "have_scroll" })
                .auto_shrink([false, false])
                .max_height((avail_h - 16.0).max(160.0))
                .show_rows(ui, 36.0, indices.len(), |ui, row_range| {
                    let Some(ref list) = list_opt else { return };
                    for row_i in row_range {
                        let it = &list[indices[row_i]];
                        let selected = if is_donor {
                            self.selected_want_idx == row_i
                        } else {
                            self.selected_have_idx == row_i
                        };
                        let swapped = !is_donor && self.is_pkg_swapped(&it.asset_package);
                        let key = self.request_item_icon(it, None, 40);
                        let tid = self.gpu_icons.get(&key).map(|t| t.id());
                        let hb_s = if is_body_slot {
                            Some((body_hitbox_for_item(&it.product, &it.asset_package).label(), pal.text_muted))
                        } else if swapped {
                            Some(("Swapped", pal.accent_green))
                        } else {
                            None
                        };
                        if clipped_row(ui, selected, 34.0, tid, 28.0, &it.product, hb_s) {
                            if is_donor {
                                self.selected_want_idx = row_i;
                            } else {
                                self.selected_have_idx = row_i;
                            }
                        }
                    }
                });
        });
    }

    // ── 2. Custom Decal Studio (AlphaConsole .ZIP/.JSON/.PNG + Mask Support) ─
    fn render_custom_decal_studio(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let pal = AppPalette::from_ui(ui);
        self.ensure_decal_target_cache();
        if self.decal_target_idx >= self.cached_decal_idxs.len() {
            self.decal_target_idx = 0;
        }
        let target_decal = self
            .cached_decal_idxs
            .get(self.decal_target_idx)
            .and_then(|&i| self.items_by_slot.get("Decal").and_then(|v| v.get(i)))
            .cloned();

        ui.horizontal(|ui| {
            ui.label(RichText::new("Decal Studio").font(FontId::proportional(20.0)).color(pal.text_primary).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let has_img = self.decal_custom_img.is_some() || self.decal_mask_img.is_some();
                if ui
                    .add_enabled(
                        !self.is_busy && target_decal.is_some() && has_img,
                        primary_action_button(if self.is_busy { "Applying..." } else { "Apply Decal" }, 122.0),
                    )
                    .clicked()
                {
                    if let Some(dec) = target_decal.clone() {
                        let active_pack = self.ac_decal_packs.get(self.ac_decal_idx).cloned().unwrap_or_else(|| {
                            AlphaConsoleDecalPack {
                                name: self.decal_custom_path.clone(),
                                body_id: None,
                                skin_id: None,
                                diffuse_base_img: None,
                                skin_img: self.decal_custom_img.clone(),
                                mask_img: self.decal_mask_img.clone(),
                                chassis_diffuse_img: None,
                                source_label: self.decal_custom_path.clone(),
                            }
                        });
                        let pref_donor = active_pack
                            .skin_id
                            .and_then(|sid| find_catalog_item_by_id(&self.all_catalog, sid))
                            .map(|it| it.asset_package);
                        self.is_busy = true;
                        let cdir = PathBuf::from(&self.cooked_dir);
                        let tx = self.tx.clone();
                        thread::spawn(move || {
                            match inject_alphaconsole_decal_at(
                                &cdir,
                                &dec.asset_package,
                                Some(&dec.asset_path),
                                &active_pack,
                                pref_donor.as_deref(),
                            ) {
                                Ok(msg) => {
                                    let rec = ActiveSwapRecord {
                                        target_pkg: dec.asset_package.clone(),
                                        donor_desc: active_pack.name,
                                        target_desc: dec.product.clone(),
                                        kind: "DECAL".into(),
                                        slot: "Decal".into(),
                                        donor_pkg: String::new(),
                                        donor_asset_path: String::new(),
                                        target_asset_path: dec.asset_path.clone(),
                                        paint_hex: None,
                                        paint_name: None,
                                    };
                                    let _ = tx.send(WorkerEvent::OperationDone {
                                        success: true,
                                        message: msg,
                                        swapped_pkgs: vec![rec],
                                        cleared_pkgs: vec![],
                                    });
                                }
                                Err(e) => {
                                    let _ = tx.send(WorkerEvent::OperationDone {
                                        success: false,
                                        message: e,
                                        swapped_pkgs: vec![],
                                        cleared_pkgs: vec![],
                                    });
                                }
                            }
                        });
                    }
                }
                if let Some(ref td) = target_decal {
                    if self.is_pkg_swapped(&td.asset_package)
                        && ui.add(secondary_action_button(&pal, "Reset", pal.accent_red)).clicked()
                    {
                        restore_single_package(&Path::new(&self.cooked_dir).join(&td.asset_package));
                        self.state.active_swaps.remove(&td.asset_package.to_lowercase());
                        self.save_state();
                        self.refresh_health_report();
                        self.status_text = format!("Restored {}", td.product);
                    }
                }
            });
        });

        ui.add_space(6.0);
        app_card_full(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.add(secondary_action_button(&pal, "Import .ZIP / .JSON / .PNG", pal.accent_blue)).clicked() {
                    if let Some(p) = rfd::FileDialog::new()
                        .add_filter("AlphaConsole Decal or Image", &["zip", "json", "png", "jpg", "jpeg", "webp"])
                        .pick_file()
                    {
                        self.import_decal_file(&p);
                    }
                }
                if ui.add(secondary_action_button(&pal, "Import Folder", pal.accent_blue)).clicked() {
                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        self.import_decal_file(&dir);
                    }
                }
                if ui.add(secondary_action_button(&pal, "Export UV", pal.text_secondary)).clicked() {
                    let tpl = generate_decal_template_for_car(self.decal_car_label, Some(Path::new(&self.cooked_dir)));
                    if let Some(sp) = rfd::FileDialog::new()
                        .set_file_name(&format!("{}_UV.png", self.decal_car_label))
                        .save_file()
                    {
                        let _ = tpl.save(&sp);
                        self.status_text = format!("Saved {} UV", self.decal_car_label);
                    }
                }
                if ui.add(secondary_action_button(&pal, "Reset UV Guide", pal.text_muted)).clicked() {
                    self.ac_decal_packs.clear();
                    self.decal_mask_img = None;
                    self.decal_custom_img = Some(generate_decal_template_for_car(self.decal_car_label, Some(Path::new(&self.cooked_dir))));
                    self.decal_custom_path = format!("{} UV", self.decal_car_label);
                    self.decal_preview_dirty = true;
                }
                if !self.ac_decal_packs.is_empty() {
                    ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                        let p = &self.ac_decal_packs[self.ac_decal_idx];
                        if p.mask_img.is_some() {
                            rui.label(RichText::new("Mask").font(FontId::proportional(11.0)).color(pal.accent_green).strong());
                        }
                        if p.skin_img.is_some() {
                            rui.label(RichText::new("Diffuse").font(FontId::proportional(11.0)).color(pal.accent_blue).strong());
                        }
                    });
                }
            });

            if let Some(pack) = self.ac_decal_packs.get(self.ac_decal_idx).cloned() {
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    if self.ac_decal_packs.len() > 1 {
                        let cur_nm = pack.name.clone();
                        let mut picked = None;
                        egui::ComboBox::from_id_salt("ac_dec_preset")
                            .selected_text(cur_nm)
                            .width(180.0)
                            .show_ui(ui, |ui| {
                                for (i, p) in self.ac_decal_packs.iter().enumerate() {
                                    if ui.selectable_label(self.ac_decal_idx == i, &p.name).clicked() {
                                        picked = Some(i);
                                    }
                                }
                            });
                        if let Some(i) = picked {
                            self.ac_decal_idx = i;
                            self.apply_selected_ac_decal_pack();
                        }
                    }
                    if let Some(bid) = pack.body_id {
                        let b_name = find_catalog_item_by_id(&self.all_catalog, bid)
                            .map(|b| b.product)
                            .unwrap_or_else(|| format!("Body #{}", bid));
                        ui.label(RichText::new(b_name).font(FontId::proportional(11.5)).color(pal.accent_green).strong());
                    }
                    if let Some(sid) = pack.skin_id {
                        let s_name = find_catalog_item_by_id(&self.all_catalog, sid)
                            .map(|s| s.product)
                            .unwrap_or_else(|| format!("Skin #{}", sid));
                        ui.label(RichText::new(format!("· {}", s_name)).font(FontId::proportional(11.5)).color(pal.text_secondary));
                    }
                });
            }
        });

        ui.add_space(8.0);
        let decal_list_opt = self.items_by_slot.get("Decal").cloned();
        let decal_idxs = self.cached_decal_idxs.clone();
        ui.columns(2, |cols| {
            let left_h = cols[0].available_height().max(260.0);
            app_card(&pal).show(&mut cols[0], |ui| {
                ui.set_min_size(Vec2::new(ui.available_width(), (left_h - 30.0).max(220.0)));
                ui.horizontal(|ui| {
                    for label in ["Octane", "Fennec", "Dominus"] {
                        if choice_chip(ui, self.decal_car_label == label, label) {
                            self.decal_car_label = label;
                            self.decal_target_idx = 0;
                            self.cache_decal_key = None;
                            if self.decal_custom_path.ends_with("UV") {
                                self.decal_custom_img = Some(generate_decal_template_for_car(label, Some(Path::new(&self.cooked_dir))));
                                self.decal_custom_path = format!("{} UV", label);
                            }
                            self.decal_preview_dirty = true;
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                        if rui.color_edit_button_srgb(&mut self.decal_accent_rgb).changed() {
                            self.decal_preview_dirty = true;
                        }
                        if rui.color_edit_button_srgb(&mut self.decal_primary_rgb).changed() {
                            self.decal_preview_dirty = true;
                        }
                        rui.label(RichText::new("Tint").font(FontId::proportional(11.0)).color(pal.text_muted));
                    });
                });
                ui.add_space(10.0);
                if self.decal_preview_dirty || self.decal_preview_tex.is_none() {
                    let rendered = render_2d_car_decal_with_mask_preview(
                        self.decal_car_label,
                        Some(Path::new(&self.cooked_dir)),
                        self.decal_custom_img.as_ref(),
                        self.decal_mask_img.as_ref(),
                        self.decal_primary_rgb,
                        self.decal_accent_rgb,
                        320,
                    );
                    self.decal_preview_tex = Some(ctx.load_texture("decal_2d_vp", rgba_to_egui_image(&rendered), TextureOptions::LINEAR));
                    self.decal_preview_dirty = false;
                }
                let prev_dim = (ui.available_width() - 32.0).min(ui.available_height() - 54.0).clamp(220.0, 360.0);
                let tgt_hint = target_decal
                    .as_ref()
                    .map(|d| format!("{} -> {}", self.decal_custom_path, d.product))
                    .unwrap_or_else(|| self.decal_custom_path.clone());
                ui.vertical_centered(|ui| {
                    app_well_frame(&pal).show(ui, |wui| {
                        if let Some(ref tex) = self.decal_preview_tex {
                            wui.image((tex.id(), Vec2::splat(prev_dim)));
                        }
                    });
                    ui.add_space(8.0);
                    ui.add(egui::Label::new(RichText::new(tgt_hint).font(FontId::proportional(11.5)).color(pal.text_secondary)).truncate());
                });
            });

            cols[1].horizontal(|ui| {
                let sw = (ui.available_width() - 90.0).max(120.0);
                ui.allocate_ui(Vec2::new(sw, 30.0), |sui| {
                    search_box(sui, &mut self.decal_target_search, "Target decal...");
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                    if choice_chip(rui, !self.decal_filter_car_only, "All") {
                        self.decal_filter_car_only = !self.decal_filter_car_only;
                        self.decal_target_idx = 0;
                        self.cache_decal_key = None;
                    }
                });
            });
            cols[1].add_space(6.0);
            let avail_h = cols[1].available_height().max(220.0);
            app_list_card(&pal).show(&mut cols[1], |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("decal_target_list")
                    .auto_shrink([false, false])
                    .max_height((avail_h - 16.0).max(160.0))
                    .show_rows(ui, 28.0, decal_idxs.len(), |ui, row_range| {
                        let Some(ref list) = decal_list_opt else { return };
                        for i in row_range {
                            let d = &list[decal_idxs[i]];
                            let badge = if self.is_pkg_swapped(&d.asset_package) {
                                Some(("Custom", pal.accent_green))
                            } else {
                                None
                            };
                            if clipped_row(ui, self.decal_target_idx == i, 26.0, None, 0.0, &d.product, badge) {
                                self.decal_target_idx = i;
                                if let Some(car) = detect_car_uv_label(&format!("{} {}", d.product, d.asset_package)) {
                                    if self.decal_car_label != car {
                                        self.decal_car_label = car;
                                        if self.decal_custom_path.ends_with("UV") {
                                            self.decal_custom_img = Some(generate_decal_template_for_car(car, Some(Path::new(&self.cooked_dir))));
                                            self.decal_custom_path = format!("{} UV", car);
                                        }
                                        self.decal_preview_dirty = true;
                                    }
                                }
                            }
                        }
                    });
            });
        });
    }

    // ── 3. Custom PFP Studio ─────────────────────────────────────────────────
    fn render_custom_pfp_studio(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let pal = AppPalette::from_ui(ui);
        let q_low = self.pfp_search.to_lowercase();
        if self.cache_pfp_q.as_deref() != Some(&q_low) {
            self.cached_pfp_idxs = self
                .avatar_borders
                .iter()
                .enumerate()
                .filter(|(_, (lbl, pkg))| {
                    q_low.is_empty() || lbl.to_lowercase().contains(&q_low) || pkg.to_lowercase().contains(&q_low)
                })
                .map(|(i, _)| i)
                .collect();
            self.cache_pfp_q = Some(q_low);
        }
        if self.pfp_border_idx >= self.cached_pfp_idxs.len() {
            self.pfp_border_idx = 0;
        }
        let selected_border = self
            .cached_pfp_idxs
            .get(self.pfp_border_idx)
            .and_then(|&i| self.avatar_borders.get(i))
            .cloned();

        ui.horizontal(|ui| {
            ui.label(RichText::new("Avatar PFP").font(FontId::proportional(20.0)).color(pal.text_primary).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(
                        !self.is_busy && selected_border.is_some() && self.pfp_custom_img.is_some(),
                        primary_action_button("Apply PFP", 118.0),
                    )
                    .clicked()
                {
                    if let (Some((lbl, pkg)), Some(img)) = (selected_border.clone(), self.pfp_custom_img.clone()) {
                        self.is_busy = true;
                        let cdir = PathBuf::from(&self.cooked_dir);
                        let tx = self.tx.clone();
                        thread::spawn(move || match inject_custom_pfp(&cdir, &pkg, &img) {
                            Ok(msg) => {
                                let rec = ActiveSwapRecord {
                                    target_pkg: pkg,
                                    donor_desc: "Custom PFP".into(),
                                    target_desc: lbl,
                                    kind: "PFP".into(),
                                    slot: "Avatar Border".into(),
                                    donor_pkg: String::new(),
                                    donor_asset_path: String::new(),
                                    target_asset_path: String::new(),
                                    paint_hex: None,
                                    paint_name: None,
                                };
                                let _ = tx.send(WorkerEvent::OperationDone {
                                    success: true,
                                    message: msg,
                                    swapped_pkgs: vec![rec],
                                    cleared_pkgs: vec![],
                                });
                            }
                            Err(e) => {
                                let _ = tx.send(WorkerEvent::OperationDone {
                                    success: false,
                                    message: e,
                                    swapped_pkgs: vec![],
                                    cleared_pkgs: vec![],
                                });
                            }
                        });
                    }
                }
                if let Some((ref lbl, ref pkg)) = selected_border {
                    if self.is_pkg_swapped(pkg) && ui.add(secondary_action_button(&pal, "Reset", pal.accent_red)).clicked() {
                        restore_single_package(&Path::new(&self.cooked_dir).join(pkg));
                        clear_rl_texture_cache();
                        self.state.active_swaps.remove(&pkg.to_lowercase());
                        self.save_state();
                        self.refresh_health_report();
                        self.pfp_preview_dirty = true;
                        self.status_text = format!("Reset {}", lbl);
                    }
                }
            });
        });

        ui.add_space(6.0);
        app_card_full(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.add(secondary_action_button(&pal, "Load Image", pal.accent_blue)).clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Image", &["png", "jpg", "jpeg", "webp"]).pick_file() {
                        if let Ok(im) = image::open(&p) {
                            self.pfp_custom_img = Some(im.to_rgba8());
                            self.pfp_custom_path = p.file_name().unwrap_or_default().to_string_lossy().to_string();
                            self.pfp_preview_dirty = true;
                            self.banner_preview_dirty = true;
                        }
                    }
                }
                if !self.pfp_custom_path.is_empty() {
                    ui.add(egui::Label::new(RichText::new(&self.pfp_custom_path).font(FontId::proportional(12.0)).color(pal.text_primary)).truncate());
                } else {
                    ui.label(RichText::new("PNG / JPG / WEBP").font(FontId::proportional(11.5)).color(pal.text_muted));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                    rui.label(RichText::new(format!("{} borders", self.avatar_borders.len())).font(FontId::proportional(11.5)).color(pal.text_muted));
                });
            });
        });

        if self.pfp_preview_dirty || self.pfp_preview_tex.is_none() {
            let composited = selected_border
                .as_ref()
                .and_then(|(_, pkg)| compose_pfp_with_border_preview(Path::new(&self.cooked_dir), pkg, self.pfp_custom_img.as_ref()))
                .or_else(|| self.pfp_custom_img.clone());
            if let Some(ref c_im) = composited {
                let scaled = image::DynamicImage::ImageRgba8(c_im.clone()).resize_exact(216, 216, FilterType::Lanczos3).to_rgba8();
                self.pfp_preview_tex = Some(ctx.load_texture("pfp_prev", rgba_to_egui_image(&scaled), TextureOptions::LINEAR));
            } else {
                self.pfp_preview_tex = None;
            }
            self.pfp_composited_img = composited;
            self.pfp_preview_dirty = false;
            self.banner_preview_dirty = true;
        }

        ui.add_space(8.0);
        let pfp_idxs = self.cached_pfp_idxs.clone();
        ui.columns(2, |cols| {
            let left_h = cols[0].available_height().max(260.0);
            app_card(&pal).show(&mut cols[0], |ui| {
                ui.set_min_size(Vec2::new(ui.available_width(), (left_h - 30.0).max(220.0)));
                ui.vertical_centered(|ui| {
                    ui.add_space(((left_h - 300.0) * 0.4).max(16.0));
                    app_well_frame(&pal).show(ui, |wui| {
                        if let Some(ref tex) = self.pfp_preview_tex {
                            wui.image((tex.id(), Vec2::splat(216.0)));
                        }
                    });
                    ui.add_space(12.0);
                    if let Some((ref lbl, _)) = selected_border {
                        ui.add(egui::Label::new(RichText::new(lbl).font(FontId::proportional(13.5)).color(pal.text_primary).strong()).truncate());
                        ui.label(RichText::new("Avatar Frame Cutout Preview").font(FontId::proportional(11.0)).color(pal.text_muted));
                    }
                });
            });

            search_box(&mut cols[1], &mut self.pfp_search, "Search border...");
            cols[1].add_space(6.0);
            let avail_h = cols[1].available_height().max(220.0);
            app_list_card(&pal).show(&mut cols[1], |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("pfp_border_list")
                    .auto_shrink([false, false])
                    .max_height((avail_h - 16.0).max(160.0))
                    .show_rows(ui, 28.0, pfp_idxs.len(), |ui, row_range| {
                        for i in row_range {
                            let (lbl, pkg) = &self.avatar_borders[pfp_idxs[i]];
                            let badge = if self.is_pkg_swapped(pkg) { Some(("Custom", pal.accent_green)) } else { None };
                            if clipped_row(ui, self.pfp_border_idx == i, 26.0, None, 0.0, lbl, badge) {
                                self.pfp_border_idx = i;
                                self.pfp_preview_dirty = true;
                            }
                        }
                    });
            });
        });
    }

    // ── 4. Custom Banner Studio ──────────────────────────────────────────────
    fn render_custom_banner_studio(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let pal = AppPalette::from_ui(ui);
        let q_low = self.banner_search.to_lowercase();
        if self.cache_banner_q.as_deref() != Some(&q_low) {
            self.cached_banner_idxs = self
                .player_banners
                .iter()
                .enumerate()
                .filter(|(_, (lbl, pkg))| {
                    q_low.is_empty() || lbl.to_lowercase().contains(&q_low) || pkg.to_lowercase().contains(&q_low)
                })
                .map(|(i, _)| i)
                .collect();
            self.cache_banner_q = Some(q_low);
        }
        if self.banner_target_idx >= self.cached_banner_idxs.len() {
            self.banner_target_idx = 0;
        }
        let selected_banner = self
            .cached_banner_idxs
            .get(self.banner_target_idx)
            .and_then(|&i| self.player_banners.get(i))
            .cloned();

        ui.horizontal(|ui| {
            ui.label(RichText::new("Banner Studio").font(FontId::proportional(20.0)).color(pal.text_primary).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(
                        !self.is_busy && selected_banner.is_some() && self.banner_custom_img.is_some(),
                        primary_action_button("Apply Banner", 126.0),
                    )
                    .clicked()
                {
                    if let (Some((lbl, pkg)), Some(img)) = (selected_banner.clone(), self.banner_custom_img.clone()) {
                        let final_img = if self.banner_bake_name {
                            bake_name_onto_banner(&img, &self.banner_player_name)
                        } else {
                            img
                        };
                        self.is_busy = true;
                        let cdir = PathBuf::from(&self.cooked_dir);
                        let tx = self.tx.clone();
                        thread::spawn(move || match inject_custom_banner(&cdir, &pkg, &final_img) {
                            Ok(msg) => {
                                let rec = ActiveSwapRecord {
                                    target_pkg: pkg,
                                    donor_desc: "Custom Banner".into(),
                                    target_desc: lbl,
                                    kind: "BANNER".into(),
                                    slot: "Player Banner".into(),
                                    donor_pkg: String::new(),
                                    donor_asset_path: String::new(),
                                    target_asset_path: String::new(),
                                    paint_hex: None,
                                    paint_name: None,
                                };
                                let _ = tx.send(WorkerEvent::OperationDone {
                                    success: true,
                                    message: msg,
                                    swapped_pkgs: vec![rec],
                                    cleared_pkgs: vec![],
                                });
                            }
                            Err(e) => {
                                let _ = tx.send(WorkerEvent::OperationDone {
                                    success: false,
                                    message: e,
                                    swapped_pkgs: vec![],
                                    cleared_pkgs: vec![],
                                });
                            }
                        });
                    }
                }
                if let Some((ref lbl, ref pkg)) = selected_banner {
                    if self.is_pkg_swapped(pkg) && ui.add(secondary_action_button(&pal, "Reset", pal.accent_red)).clicked() {
                        restore_single_package(&Path::new(&self.cooked_dir).join(pkg));
                        clear_rl_texture_cache();
                        self.state.active_swaps.remove(&pkg.to_lowercase());
                        self.save_state();
                        self.refresh_health_report();
                        self.status_text = format!("Reset {}", lbl);
                    }
                }
            });
        });

        ui.add_space(6.0);
        app_card_full(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.add(secondary_action_button(&pal, "Load Image", pal.accent_blue)).clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Image", &["png", "jpg", "jpeg", "webp"]).pick_file() {
                        if let Ok(im) = image::open(&p) {
                            self.banner_custom_img = Some(im.to_rgba8());
                            self.banner_custom_path = p.file_name().unwrap_or_default().to_string_lossy().to_string();
                            self.banner_preview_dirty = true;
                        }
                    }
                }
                if ui.add(secondary_action_button(&pal, "Export 14:3", pal.text_secondary)).clicked() {
                    let tpl = generate_banner_template();
                    if let Some(sp) = rfd::FileDialog::new().set_file_name("Banner_840x200.png").save_file() {
                        let _ = tpl.save(&sp);
                    }
                }
                if ui.add(secondary_action_button(&pal, "Template", pal.text_muted)).clicked() {
                    self.banner_custom_img = Some(generate_banner_template());
                    self.banner_custom_path = "Banner Template".into();
                    self.banner_preview_dirty = true;
                }
                ui.add_space(10.0);
                ui.allocate_ui(Vec2::new(165.0, 29.0), |nui| {
                    if styled_input_box(nui, &mut self.banner_player_name, "Custom player name...", false) {
                        self.banner_preview_dirty = true;
                    }
                });
                ui.add_space(6.0);
                app_switch(ui, &mut self.banner_bake_name, "Bake Name");
            });
        });

        ui.add_space(8.0);
        let banner_idxs = self.cached_banner_idxs.clone();
        ui.columns(2, |cols| {
            if self.banner_preview_dirty || self.banner_preview_tex.is_none() {
                let pfp_or_comp = self.pfp_composited_img.as_ref().or(self.pfp_custom_img.as_ref());
                let rendered = render_2d_banner_scoreboard_preview(
                    self.banner_custom_img.as_ref(),
                    pfp_or_comp,
                    &self.banner_player_name,
                    540,
                    168,
                );
                self.banner_preview_tex = Some(ctx.load_texture("banner_2d_vp", rgba_to_egui_image(&rendered), TextureOptions::LINEAR));
                self.banner_preview_dirty = false;
            }
            let left_h = cols[0].available_height().max(260.0);
            app_card(&pal).show(&mut cols[0], |ui| {
                ui.set_min_size(Vec2::new(ui.available_width(), (left_h - 30.0).max(220.0)));
                ui.vertical_centered(|ui| {
                    ui.add_space(((left_h - 240.0) * 0.42).max(20.0));
                    app_well_frame(&pal).show(ui, |wui| {
                        if let Some(ref tex) = self.banner_preview_tex {
                            let w = wui.available_width().min(460.0);
                            wui.image((tex.id(), Vec2::new(w, w * 168.0 / 540.0)));
                        }
                    });
                    ui.add_space(12.0);
                    ui.add(egui::Label::new(RichText::new(&self.banner_custom_path).font(FontId::proportional(13.0)).color(pal.text_primary).strong()).truncate());
                    ui.label(RichText::new("14:3 Scoreboard Aspect (840 x 200)").font(FontId::proportional(11.0)).color(pal.text_muted));
                });
            });

            search_box(&mut cols[1], &mut self.banner_search, "Search banner...");
            cols[1].add_space(6.0);
            let avail_h = cols[1].available_height().max(220.0);
            app_list_card(&pal).show(&mut cols[1], |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("banner_studio_list")
                    .auto_shrink([false, false])
                    .max_height((avail_h - 16.0).max(160.0))
                    .show_rows(ui, 28.0, banner_idxs.len(), |ui, row_range| {
                        for i in row_range {
                            let (lbl, pkg) = &self.player_banners[banner_idxs[i]];
                            let badge = if self.is_pkg_swapped(pkg) { Some(("Custom", pal.accent_green)) } else { None };
                            if clipped_row(ui, self.banner_target_idx == i, 26.0, None, 0.0, lbl, badge) {
                                self.banner_target_idx = i;
                            }
                        }
                    });
            });
        });
    }

    // ── 5. Custom Match Ball Studio (AlphaConsole .ZIP/.JSON/.PNG — No Boost Pads) ─
    fn render_custom_ball_studio(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let pal = AppPalette::from_ui(ui);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Match Ball").font(FontId::proportional(20.0)).color(pal.text_primary).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(
                        !self.is_busy && self.ball_custom_img.is_some(),
                        primary_action_button(if self.is_busy { "Applying..." } else { "Apply Ball" }, 122.0),
                    )
                    .clicked()
                {
                    let pack = self.ac_ball_packs.get(self.ac_ball_idx).cloned().unwrap_or_else(|| {
                        AlphaConsoleBallPack {
                            name: self.ball_custom_path.clone(),
                            group: String::new(),
                            diffuse_img: self.ball_custom_img.clone().unwrap(),
                            normal_img: self.ball_normal_img.clone(),
                            mask_img: self.ball_mask_img.clone(),
                            source_label: self.ball_custom_path.clone(),
                        }
                    });
                    self.is_busy = true;
                    let cdir = PathBuf::from(&self.cooked_dir);
                    let tx = self.tx.clone();
                    thread::spawn(move || match inject_alphaconsole_ball(&cdir, &pack) {
                        Ok(msg) => {
                            let rec = ActiveSwapRecord {
                                target_pkg: "GameInfo_Soccar_SF.upk".into(),
                                donor_desc: pack.name,
                                target_desc: "Match Ball".into(),
                                kind: "BALL".into(),
                                slot: String::new(),
                                donor_pkg: String::new(),
                                donor_asset_path: String::new(),
                                target_asset_path: String::new(),
                                paint_hex: None,
                                paint_name: None,
                            };
                            let _ = tx.send(WorkerEvent::OperationDone {
                                success: true,
                                message: msg,
                                swapped_pkgs: vec![rec],
                                cleared_pkgs: vec![],
                            });
                        }
                        Err(e) => {
                            let _ = tx.send(WorkerEvent::OperationDone {
                                success: false,
                                message: e,
                                swapped_pkgs: vec![],
                                cleared_pkgs: vec![],
                            });
                        }
                    });
                }
                if self.is_pkg_swapped("GameInfo_Soccar_SF.upk") && ui.add(secondary_action_button(&pal, "Reset", pal.accent_red)).clicked() {
                    restore_single_package(&Path::new(&self.cooked_dir).join("GameInfo_Soccar_SF.upk"));
                    clear_rl_texture_cache();
                    self.state.active_swaps.remove("gameinfo_soccar_sf.upk");
                    self.save_state();
                    self.refresh_health_report();
                    self.status_text = "Restored Match Ball".into();
                }
            });
        });

        ui.add_space(6.0);
        app_card_full(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.add(secondary_action_button(&pal, "Import .ZIP / .JSON / .PNG", pal.accent_blue)).clicked() {
                    if let Some(p) = rfd::FileDialog::new()
                        .add_filter("AlphaConsole Ball or Image", &["zip", "json", "png", "jpg", "jpeg", "webp"])
                        .pick_file()
                    {
                        self.import_ball_file(&p);
                    }
                }
                if ui.add(secondary_action_button(&pal, "Import Folder", pal.accent_blue)).clicked() {
                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        self.import_ball_file(&dir);
                    }
                }
                if ui.add(secondary_action_button(&pal, "Export UV", pal.text_secondary)).clicked() {
                    let tpl = generate_ball_template(Some(Path::new(&self.cooked_dir)));
                    if let Some(sp) = rfd::FileDialog::new().set_file_name("Ball_UV.png").save_file() {
                        let _ = tpl.save(&sp);
                    }
                }
                if ui.add(secondary_action_button(&pal, "Vanilla", pal.text_muted)).clicked() {
                    self.ac_ball_packs.clear();
                    self.ball_normal_img = None;
                    self.ball_mask_img = None;
                    self.ball_custom_img = Some(extract_ball_texture_preview(Path::new(&self.cooked_dir)).unwrap_or_else(|| generate_ball_template(None)));
                    self.ball_custom_path = "Vanilla Match Ball".into();
                    self.ball_orb_dirty = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                    if self.ball_mask_img.is_some() {
                        rui.label(RichText::new("Masks").font(FontId::proportional(11.0)).color(pal.accent_amber).strong());
                    }
                    if self.ball_normal_img.is_some() {
                        rui.label(RichText::new("Normal").font(FontId::proportional(11.0)).color(pal.accent_green).strong());
                    }
                    rui.label(RichText::new("Diffuse").font(FontId::proportional(11.0)).color(pal.accent_blue).strong());
                });
            });

            if self.ac_ball_packs.len() > 1 {
                ui.add_space(6.0);
                let cur_nm = self.ac_ball_packs[self.ac_ball_idx].name.clone();
                let mut picked = None;
                egui::ComboBox::from_id_salt("ac_ball_preset")
                    .selected_text(cur_nm)
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for (i, p) in self.ac_ball_packs.iter().enumerate() {
                            if ui.selectable_label(self.ac_ball_idx == i, &p.name).clicked() {
                                picked = Some(i);
                            }
                        }
                    });
                if let Some(i) = picked {
                    self.ac_ball_idx = i;
                    self.apply_selected_ac_ball_pack();
                }
            }
        });

        if self.ball_orb_dirty {
            if let Some(ref b_im) = self.ball_custom_img {
                let orb = render_2d_ball_orb_preview(b_im, 260, None);
                self.ball_orb_tex = Some(ctx.load_texture("ball_2d_orb", rgba_to_egui_image(&orb), TextureOptions::LINEAR));
                let flat = image::DynamicImage::ImageRgba8(b_im.clone()).resize_exact(148, 148, FilterType::Triangle).to_rgba8();
                self.ball_flat_tex = Some(ctx.load_texture("ball_flat", rgba_to_egui_image(&flat), TextureOptions::LINEAR));
            }
            self.ball_norm_tex = self.ball_normal_img.as_ref().map(|n_im| {
                let nf = image::DynamicImage::ImageRgba8(n_im.clone()).resize_exact(148, 148, FilterType::Triangle).to_rgba8();
                ctx.load_texture("ball_norm", rgba_to_egui_image(&nf), TextureOptions::LINEAR)
            });
            self.ball_orb_dirty = false;
        }

        ui.add_space(8.0);
        let stage_h = ui.available_height().max(280.0);
        app_card_full(ui, |ui| {
            ui.set_min_height((stage_h - 32.0).max(240.0));
            ui.columns(2, |cols| {
                cols[0].vertical_centered(|ui| {
                    ui.add_space(((stage_h - 360.0) * 0.35).max(16.0));
                    app_well_frame(&pal).show(ui, |wui| {
                        if let Some(ref tex) = self.ball_orb_tex {
                            wui.image((tex.id(), Vec2::splat(256.0)));
                        }
                    });
                    ui.add_space(12.0);
                    ui.add(egui::Label::new(RichText::new(&self.ball_custom_path).font(FontId::proportional(14.5)).color(pal.text_primary).strong()).truncate());
                    ui.label(RichText::new("GameInfo_Soccar_SF.upk · Sphere Preview").font(FontId::proportional(11.0)).color(pal.text_muted));
                });

                cols[1].vertical(|ui| {
                    ui.add_space(16.0);
                    ui.label(RichText::new("TEXTURE CHANNELS").font(FontId::proportional(10.0)).color(pal.text_muted).strong());
                    ui.add_space(10.0);
                    let col_w = ui.available_width().max(240.0);
                    let tile_w = ((col_w - 14.0) * 0.5).clamp(112.0, 186.0);
                    let img_dim = (tile_w - 22.0).clamp(88.0, 148.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        if let Some(ref ft) = self.ball_flat_tex {
                            ui.allocate_ui(Vec2::new(tile_w, img_dim + 46.0), |tui| {
                                app_well_frame(&pal).show(tui, |vui| {
                                    vui.set_min_width((tile_w - 22.0).max(80.0));
                                    vui.vertical_centered(|vui| {
                                        vui.image((ft.id(), Vec2::splat(img_dim)));
                                        vui.add_space(6.0);
                                        vui.label(RichText::new("Diffuse Map").font(FontId::proportional(11.5)).color(pal.text_primary).strong());
                                    });
                                });
                            });
                        }
                        if let Some(ref nt) = self.ball_norm_tex {
                            ui.allocate_ui(Vec2::new(tile_w, img_dim + 46.0), |tui| {
                                app_well_frame(&pal).show(tui, |vui| {
                                    vui.set_min_width((tile_w - 22.0).max(80.0));
                                    vui.vertical_centered(|vui| {
                                        vui.image((nt.id(), Vec2::splat(img_dim)));
                                        vui.add_space(6.0);
                                        vui.label(RichText::new("Normal Map").font(FontId::proportional(11.5)).color(pal.accent_green).strong());
                                    });
                                });
                            });
                        } else {
                            ui.allocate_ui(Vec2::new(tile_w, img_dim + 46.0), |tui| {
                                app_well_frame(&pal).show(tui, |vui| {
                                    vui.set_min_size(Vec2::new((tile_w - 22.0).max(80.0), img_dim + 24.0));
                                    vui.vertical_centered(|vui| {
                                        vui.add_space((img_dim * 0.42).max(28.0));
                                        vui.label(RichText::new("Normal Map").font(FontId::proportional(11.5)).color(pal.text_muted));
                                        vui.label(RichText::new("Optional (Vanilla)").font(FontId::proportional(10.5)).color(pal.text_muted));
                                    });
                                });
                            });
                        }
                    });
                });
            });
        });
    }

    // ── 6. Map Swapper (96 Arenas — Pure Normal Swapping Only) ────────────────
    fn render_map_swapper(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let pal = AppPalette::from_ui(ui);
        let filter_list = |q: &str, cat: usize| -> Vec<usize> {
            let ql = q.to_lowercase();
            STANDARD_SOCCAR_MAPS
                .iter()
                .enumerate()
                .filter(|(_, m)| {
                    let cat_ok = match cat {
                        1 => m.category == "Standard",
                        2 => m.category == "Custom Default Size",
                        3 => !m.is_default_size,
                        _ => true,
                    };
                    cat_ok
                        && (ql.is_empty()
                            || m.display_name.to_lowercase().contains(&ql)
                            || m.arena_family.to_lowercase().contains(&ql)
                            || m.upk_name.to_lowercase().contains(&ql))
                })
                .map(|(i, _)| i)
                .collect()
        };

        let d_key = (self.map_donor_search.clone(), self.map_filter_cat);
        if self.cache_map_donor_key.as_ref() != Some(&d_key) {
            self.cached_map_donor_idxs = filter_list(&d_key.0, d_key.1);
            self.cache_map_donor_key = Some(d_key);
        }
        let t_key = (self.map_target_search.clone(), self.map_filter_cat);
        if self.cache_map_target_key.as_ref() != Some(&t_key) {
            self.cached_map_target_idxs = filter_list(&t_key.0, t_key.1);
            self.cache_map_target_key = Some(t_key);
        }

        let donor_spec = &STANDARD_SOCCAR_MAPS[self.map_donor_idx.min(STANDARD_SOCCAR_MAPS.len() - 1)];
        let target_spec = &STANDARD_SOCCAR_MAPS[self.map_target_idx.min(STANDARD_SOCCAR_MAPS.len() - 1)];

        ui.horizontal(|ui| {
            ui.label(RichText::new("Maps").font(FontId::proportional(20.0)).color(pal.text_primary).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let cta_lbl = if self.is_busy {
                    "Swapping..."
                } else if self.map_override_all_default {
                    "Override Default Size"
                } else {
                    "Swap Map"
                };
                if ui.add_enabled(!self.is_busy, primary_action_button(cta_lbl, 138.0)).clicked() {
                    self.is_busy = true;
                    let cdir = PathBuf::from(&self.cooked_dir);
                    let d_upk = donor_spec.upk_name.to_string();
                    let d_disp = donor_spec.display_name.to_string();
                    let t_upk = target_spec.upk_name.to_string();
                    let t_disp = target_spec.display_name.to_string();
                    let ov_all = self.map_override_all_default;

                    self.state.last_stadium_setup = Some(SavedStadiumSpec {
                        donor_upk: Some(d_upk.clone()),
                        target_upk: t_upk.clone(),
                        override_all_57: ov_all,
                    });
                    self.save_state();

                    let tx = self.tx.clone();
                    thread::spawn(move || match swap_standard_map(&cdir, &d_upk, &t_upk, ov_all) {
                        Ok(msg) => {
                            let recs = if ov_all {
                                STANDARD_SOCCAR_MAPS
                                    .iter()
                                    .filter(|m| m.is_default_size)
                                    .map(|m| ActiveSwapRecord {
                                        target_pkg: m.upk_name.to_string(),
                                        donor_desc: d_disp.clone(),
                                        target_desc: m.display_name.to_string(),
                                        kind: "MAP".into(),
                                        slot: String::new(),
                                        donor_pkg: d_upk.clone(),
                                        donor_asset_path: String::new(),
                                        target_asset_path: String::new(),
                                        paint_hex: None,
                                        paint_name: None,
                                    })
                                    .collect()
                            } else {
                                vec![ActiveSwapRecord {
                                    target_pkg: t_upk,
                                    donor_desc: d_disp,
                                    target_desc: t_disp,
                                    kind: "MAP".into(),
                                    slot: String::new(),
                                    donor_pkg: d_upk,
                                    donor_asset_path: String::new(),
                                    target_asset_path: String::new(),
                                    paint_hex: None,
                                    paint_name: None,
                                }]
                            };
                            let _ = tx.send(WorkerEvent::OperationDone {
                                success: true,
                                message: msg,
                                swapped_pkgs: recs,
                                cleared_pkgs: vec![],
                            });
                        }
                        Err(e) => {
                            let _ = tx.send(WorkerEvent::OperationDone {
                                success: false,
                                message: e,
                                swapped_pkgs: vec![],
                                cleared_pkgs: vec![],
                            });
                        }
                    });
                }

                if ui.add(secondary_action_button(&pal, "Reset All", pal.accent_red)).clicked() && !self.is_busy {
                    let cnt = reset_all_standard_maps(Path::new(&self.cooked_dir));
                    for m in STANDARD_SOCCAR_MAPS {
                        self.state.active_swaps.remove(&m.upk_name.to_lowercase());
                    }
                    self.save_state();
                    self.refresh_health_report();
                    self.status_text = format!("Restored {} maps", cnt);
                }
            });
        });

        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            for (cid, lbl) in [(0usize, "All"), (1, "Standard"), (2, "Default Size"), (3, "Labs")] {
                if choice_chip(ui, self.map_filter_cat == cid, lbl) {
                    self.map_filter_cat = cid;
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                app_switch(rui, &mut self.map_override_all_default, "Override All Default-Size Maps");
            });
        });

        // Clean Donor -> Target Preview Card
        let d_idx = self.map_donor_idx.min(STANDARD_SOCCAR_MAPS.len() - 1);
        let t_idx = self.map_target_idx.min(STANDARD_SOCCAR_MAPS.len() - 1);
        if self.map_donor_photo.0 != d_idx || self.map_donor_photo.1.is_none() {
            let im = extract_map_preview_image(Path::new(&self.cooked_dir), &STANDARD_SOCCAR_MAPS[d_idx], 176, 92);
            self.map_donor_photo = (d_idx, Some(ctx.load_texture("map_d_p", rgba_to_egui_image(&im), TextureOptions::LINEAR)));
        }
        if !self.map_override_all_default && (self.map_target_photo.0 != t_idx || self.map_target_photo.1.is_none()) {
            let im = extract_map_preview_image(Path::new(&self.cooked_dir), &STANDARD_SOCCAR_MAPS[t_idx], 176, 92);
            self.map_target_photo = (t_idx, Some(ctx.load_texture("map_t_p", rgba_to_egui_image(&im), TextureOptions::LINEAR)));
        }

        ui.add_space(6.0);
        app_card_full(ui, |ui| {
            let side_w = ((ui.available_width() - 52.0) * 0.5).max(160.0);
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(Vec2::new(side_w, 72.0), Layout::left_to_right(Align::Center), |lui| {
                    lui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                    let (ir, _) = lui.allocate_exact_size(Vec2::new(128.0, 68.0), Sense::hover());
                    if let Some(ref tex) = self.map_donor_photo.1 {
                        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                        lui.painter().image(tex.id(), ir, uv, Color32::WHITE);
                        lui.painter().rect_stroke(ir, Rounding::same(4.0), Stroke::new(1.0_f32, pal.divider));
                    }
                    lui.add_space(10.0);
                    lui.vertical(|vui| {
                        vui.label(RichText::new("PLAY IN").font(FontId::proportional(10.0)).color(pal.accent_blue).strong());
                        vui.add(egui::Label::new(RichText::new(donor_spec.display_name).font(FontId::proportional(13.5)).color(pal.text_primary).strong()).truncate());
                        vui.label(RichText::new(donor_spec.category).font(FontId::proportional(11.0)).color(pal.text_muted));
                    });
                });
                paint_arrow_badge(ui, 30.0);
                ui.allocate_ui_with_layout(Vec2::new(side_w, 72.0), Layout::left_to_right(Align::Center), |rui| {
                    rui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                    if self.map_override_all_default {
                        let def_cnt = STANDARD_SOCCAR_MAPS.iter().filter(|m| m.is_default_size).count();
                        rui.vertical(|vui| {
                            vui.label(RichText::new("TARGETS").font(FontId::proportional(10.0)).color(pal.accent_green).strong());
                            vui.label(RichText::new(format!("{} Default-Size Arenas", def_cnt)).font(FontId::proportional(13.5)).color(pal.text_primary).strong());
                            vui.label(RichText::new("Non-standard & Labs untouched").font(FontId::proportional(11.0)).color(pal.text_muted));
                        });
                    } else {
                        let (ir, _) = rui.allocate_exact_size(Vec2::new(128.0, 68.0), Sense::hover());
                        if let Some(ref tex) = self.map_target_photo.1 {
                            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                            rui.painter().image(tex.id(), ir, uv, Color32::WHITE);
                            rui.painter().rect_stroke(ir, Rounding::same(4.0), Stroke::new(1.0_f32, pal.divider));
                        }
                        rui.add_space(10.0);
                        rui.vertical(|vui| {
                            vui.label(RichText::new("REPLACE").font(FontId::proportional(10.0)).color(pal.accent_green).strong());
                            vui.add(egui::Label::new(RichText::new(target_spec.display_name).font(FontId::proportional(13.5)).color(pal.text_primary).strong()).truncate());
                            vui.label(RichText::new(target_spec.category).font(FontId::proportional(11.0)).color(pal.text_muted));
                        });
                    }
                });
            });
        });

        ui.add_space(8.0);
        let d_idxs = self.cached_map_donor_idxs.clone();
        let t_idxs = self.cached_map_target_idxs.clone();
        ui.columns(2, |cols| {
            self.render_map_picker_col(&mut cols[0], true, &d_idxs);
            cols[1].add_enabled_ui(!self.map_override_all_default, |c_ui| {
                self.render_map_picker_col(c_ui, false, &t_idxs);
            });
        });
    }

    fn render_map_picker_col(&mut self, col: &mut egui::Ui, is_donor: bool, indices: &[usize]) {
        let pal = AppPalette::from_ui(col);
        if is_donor {
            search_box(col, &mut self.map_donor_search, "Search donor map...");
        } else {
            search_box(col, &mut self.map_target_search, "Search target map...");
        }
        col.add_space(6.0);
        let avail_h = col.available_height().max(220.0);
        app_list_card(&pal).show(col, |ui| {
            egui::ScrollArea::vertical()
                .id_salt(if is_donor { "map_d_scroll" } else { "map_t_scroll" })
                .auto_shrink([false, false])
                .max_height((avail_h - 16.0).max(160.0))
                .show_rows(ui, 30.0, indices.len(), |ui, row_range| {
                    for r in row_range {
                        let real_idx = indices[r];
                        let m: &StandardMapSpec = &STANDARD_SOCCAR_MAPS[real_idx];
                        let is_sel = if is_donor {
                            self.map_donor_idx == real_idx
                        } else {
                            self.map_target_idx == real_idx
                        };
                        let badge = if !is_donor && self.is_pkg_swapped(m.upk_name) {
                            Some(("Swapped", pal.accent_green))
                        } else if m.category == "Custom Default Size" {
                            Some(("Custom", pal.accent_blue))
                        } else if !m.is_default_size {
                            Some(("Labs", pal.text_muted))
                        } else {
                            None
                        };
                        if clipped_row(ui, is_sel, 28.0, None, 0.0, m.display_name, badge) {
                            if is_donor {
                                self.map_donor_idx = real_idx;
                            } else {
                                self.map_target_idx = real_idx;
                            }
                        }
                    }
                });
        });
    }

    // ── 7. Workshop Maps (Continuous Smooth Scroll + Streaming Download Bar) ─
    fn render_workshop_maps_studio(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let pal = AppPalette::from_ui(ui);
        let underpass_active = self.is_pkg_swapped(WORKSHOP_SAFE_SLOT);

        ui.horizontal(|ui| {
            ui.label(RichText::new("Workshop").font(FontId::proportional(20.0)).color(pal.text_primary).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if underpass_active && ui.add(secondary_action_button(&pal, "Restore Underpass", pal.accent_red)).clicked() {
                    if restore_workshop_underpass(Path::new(&self.cooked_dir)) {
                        self.state.active_swaps.remove(&WORKSHOP_SAFE_SLOT.to_lowercase());
                        self.workshop_missing_deps.clear();
                        self.save_state();
                        self.refresh_health_report();
                        self.status_text = "Restored Underpass".into();
                    }
                }
            });
        });

        ui.add_space(6.0);
        app_card_full(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.add(secondary_action_button(&pal, "Import Map (.UDK / .UPK / .ZIP)", pal.accent_green)).clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Map", &["udk", "upk", "zip"]).pick_file() {
                        match import_workshop_file_or_zip(&p, None, None) {
                            Ok(entry) => {
                                self.workshop_library = list_library_maps();
                                if let Some(pos) = self.workshop_library.iter().position(|e| e.id == entry.id) {
                                    self.workshop_selected_idx = pos;
                                }
                                self.workshop_missing_deps = scan_upk_external_dependencies(
                                    &get_workshop_library_dir().join(&entry.id).join(&entry.map_file_rel),
                                    Path::new(&self.cooked_dir),
                                );
                                self.status_is_error = false;
                                self.status_text = format!("Imported '{}'", entry.name);
                            }
                            Err(e) => {
                                self.status_is_error = true;
                                self.status_text = e;
                            }
                        }
                    }
                }
                if ui.add(secondary_action_button(&pal, "Install Textures Folder", pal.accent_blue)).clicked() {
                    if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                        match install_workshop_textures_folder(&folder, Path::new(&self.cooked_dir)) {
                            Ok((copied, _)) => {
                                self.status_is_error = false;
                                self.status_text = format!("Installed {} texture packages", copied);
                                if let Some(sel) = self.workshop_library.get(self.workshop_selected_idx) {
                                    self.workshop_missing_deps = scan_upk_external_dependencies(
                                        &get_workshop_library_dir().join(&sel.id).join(&sel.map_file_rel),
                                        Path::new(&self.cooked_dir),
                                    );
                                }
                            }
                            Err(e) => {
                                self.status_is_error = true;
                                self.status_text = e;
                            }
                        }
                    }
                }
                if ui
                    .add_enabled(!self.workshop_online_loading, secondary_action_button(&pal, "Refresh", pal.text_secondary))
                    .clicked()
                {
                    self.workshop_online_loading = true;
                    let tx = self.tx.clone();
                    thread::spawn(move || {
                        let _ = tx.send(WorkerEvent::WorkshopCatalogDone(fetch_all_online_workshop_maps(true)));
                    });
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                    rui.label(RichText::new("Free Play > Underpass Slot").font(FontId::proportional(11.5)).color(pal.text_muted));
                });
            });
            if !self.workshop_missing_deps.is_empty() {
                ui.add_space(4.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(format!("Missing packages: {}", self.workshop_missing_deps.join(", ")))
                            .font(FontId::proportional(11.0))
                            .color(pal.accent_amber),
                    )
                    .truncate(),
                );
            }
        });

        if let Some(ref dl) = self.workshop_dl_progress {
            ui.add_space(6.0);
            app_card_full(ui, |ui| {
                let fmt_mb = |b: u64| -> String { format!("{:.1} MB", (b as f64) / 1_048_576.0) };
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(format!("{} · {}", dl.name, dl.phase))
                                .font(FontId::proportional(12.5))
                                .color(pal.text_primary)
                                .strong(),
                        )
                        .truncate(),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                        let stats = if let Some(tot) = dl.total.filter(|&t| t > 0) {
                            let pct = ((dl.downloaded as f64 / tot as f64) * 100.0).min(100.0);
                            format!("{} / {} ({:.0}%)", fmt_mb(dl.downloaded), fmt_mb(tot), pct)
                        } else if dl.downloaded > 0 {
                            fmt_mb(dl.downloaded)
                        } else {
                            dl.phase.clone()
                        };
                        rui.label(RichText::new(stats).font(FontId::monospace(11.5)).color(pal.accent_blue).strong());
                    });
                });
                ui.add_space(5.0);
                let frac = match dl.total {
                    Some(tot) if tot > 0 => ((dl.downloaded as f32) / (tot as f32)).clamp(0.0, 1.0),
                    _ => {
                        let t = ctx.input(|i| i.time) as f32;
                        0.18 + 0.72 * ((t * 0.9).sin() * 0.5 + 0.5)
                    }
                };
                let bar = egui::ProgressBar::new(frac)
                    .fill(pal.accent_blue)
                    .desired_height(8.0)
                    .rounding(Rounding::same(3.0))
                    .animate(dl.total.is_none());
                ui.add(bar);
            });
        }

        ui.add_space(8.0);
        if self.workshop_online_items.is_empty() && !self.workshop_online_loading && !self.status_is_error {
            self.workshop_online_loading = true;
            let tx = self.tx.clone();
            thread::spawn(move || {
                let _ = tx.send(WorkerEvent::WorkshopCatalogDone(fetch_all_online_workshop_maps(false)));
            });
        }

        let ws_key = (
            self.workshop_online_search.to_lowercase(),
            self.workshop_direct_only,
            self.workshop_online_items.len(),
        );
        if self.cache_ws_key != ws_key {
            let q = &ws_key.0;
            let dir_only = ws_key.1;
            self.cached_ws_idxs = self
                .workshop_online_items
                .iter()
                .enumerate()
                .filter(|(_, m)| {
                    (!dir_only || m.direct_download)
                        && (q.is_empty() || m.name.to_lowercase().contains(q) || m.source.to_lowercase().contains(q))
                })
                .map(|(i, _)| i)
                .collect();
            self.cache_ws_key = ws_key;
        }
        let ws_idxs = self.cached_ws_idxs.clone();

        ui.columns(2, |cols| {
            cols[0].allocate_ui_with_layout(Vec2::new(cols[0].available_width(), 30.0), Layout::left_to_right(Align::Center), |lui| {
                lui.label(RichText::new(format!("Local Library ({})", self.workshop_library.len())).font(FontId::proportional(13.0)).color(pal.text_primary).strong());
            });
            cols[0].add_space(6.0);
            let avail_h = cols[0].available_height().max(240.0);
            app_list_card(&pal).show(&mut cols[0], |ui| {
                if self.workshop_library.is_empty() {
                    ui.set_min_size(Vec2::new(ui.available_width(), (avail_h - 14.0).max(160.0)));
                    ui.vertical_centered(|vui| {
                        vui.add_space(60.0);
                        vui.label(RichText::new("No local maps yet").color(pal.text_muted));
                        vui.label(RichText::new("Import a map or download from catalog").font(FontId::proportional(11.0)).color(pal.text_muted));
                    });
                } else {
                    let lib_len = self.workshop_library.len();
                    let lib_snap = self.workshop_library.clone();
                    egui::ScrollArea::vertical()
                        .id_salt("ws_lib_scroll")
                        .auto_shrink([false, false])
                        .max_height((avail_h - 76.0).max(120.0))
                        .show_rows(ui, 30.0, lib_len, |ui, row_range| {
                            for i in row_range {
                                if clipped_row(ui, self.workshop_selected_idx == i, 28.0, None, 0.0, &lib_snap[i].name, None) {
                                    self.workshop_selected_idx = i;
                                    self.workshop_missing_deps = scan_upk_external_dependencies(
                                        &get_workshop_library_dir().join(&lib_snap[i].id).join(&lib_snap[i].map_file_rel),
                                        Path::new(&self.cooked_dir),
                                    );
                                }
                            }
                        });
                    if let Some(sel) = self.workshop_library.get(self.workshop_selected_idx).cloned() {
                        ui.separator();
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            let load_w = (ui.available_width() - 84.0).max(140.0);
                            if ui.add_enabled(!self.is_busy, primary_action_button("Load in Underpass", load_w)).clicked() {
                                self.is_busy = true;
                                let cdir = PathBuf::from(&self.cooked_dir);
                                let tx = self.tx.clone();
                                let entry_cl = sel.clone();
                                thread::spawn(move || match load_workshop_map_into_underpass(&cdir, &entry_cl) {
                                    Ok((msg, _)) => {
                                        let rec = ActiveSwapRecord {
                                            target_pkg: WORKSHOP_SAFE_SLOT.into(),
                                            donor_desc: entry_cl.name,
                                            target_desc: "Underpass".into(),
                                            kind: "WORKSHOP".into(),
                                            slot: String::new(),
                                            donor_pkg: String::new(),
                                            donor_asset_path: String::new(),
                                            target_asset_path: String::new(),
                                            paint_hex: None,
                                            paint_name: None,
                                        };
                                        let _ = tx.send(WorkerEvent::OperationDone {
                                            success: true,
                                            message: msg,
                                            swapped_pkgs: vec![rec],
                                            cleared_pkgs: vec![],
                                        });
                                    }
                                    Err(e) => {
                                        let _ = tx.send(WorkerEvent::OperationDone {
                                            success: false,
                                            message: e,
                                            swapped_pkgs: vec![],
                                            cleared_pkgs: vec![],
                                        });
                                    }
                                });
                            }
                            if ui.add(secondary_action_button(&pal, "Delete", pal.accent_red).min_size(Vec2::new(72.0, 33.0))).clicked() {
                                let _ = delete_library_map(&sel);
                                self.workshop_library = list_library_maps();
                                self.workshop_selected_idx = 0;
                            }
                        });
                    }
                }
            });

            cols[1].horizontal(|ui| {
                let sw = (ui.available_width() - 92.0).max(120.0);
                ui.allocate_ui(Vec2::new(sw, 30.0), |sui| {
                    search_box(sui, &mut self.workshop_online_search, &format!("Browse {} maps...", ws_idxs.len()));
                });
                ui.add_space(4.0);
                app_switch(ui, &mut self.workshop_direct_only, "1-Click");
            });
            cols[1].add_space(6.0);
            let avail_r = cols[1].available_height().max(240.0);
            app_list_card(&pal).show(&mut cols[1], |ui| {
                if self.workshop_online_loading || ws_idxs.is_empty() {
                    ui.set_min_size(Vec2::new(ui.available_width(), (avail_r - 16.0).max(160.0)));
                    ui.vertical_centered(|vui| {
                        vui.add_space(64.0);
                        if self.workshop_online_loading {
                            vui.label(RichText::new("Loading online catalog...").color(pal.text_secondary));
                        } else {
                            vui.label(RichText::new("No matching maps").color(pal.text_muted));
                        }
                    });
                } else {
                    let online_len = ws_idxs.len();
                    egui::ScrollArea::vertical()
                        .id_salt("ws_online_scroll")
                        .auto_shrink([false, false])
                        .max_height((avail_r - 16.0).max(160.0))
                        .show_rows(ui, 56.0, online_len, |ui, row_range| {
                            for r in row_range {
                                let item = self.workshop_online_items[ws_idxs[r]].clone();
                                if let Some(ref tu) = item.thumb_url {
                                    if !self.workshop_online_thumbs.contains_key(tu) && !self.workshop_online_pending_thumbs.contains(tu) {
                                        self.workshop_online_pending_thumbs.insert(tu.clone());
                                        let u_cl = tu.clone();
                                        let tx = self.tx.clone();
                                        thread::spawn(move || {
                                            if let Some(im) = fetch_or_cache_thumb(&u_cl) {
                                                let _ = tx.send(WorkerEvent::WorkshopThumbReady { url: u_cl, rgba: im });
                                            }
                                        });
                                    }
                                }
                                let avail_w = ui.available_width().max(120.0);
                                let (rect, resp) = ui.allocate_exact_size(Vec2::new(avail_w, 52.0), Sense::hover());
                                if ui.is_rect_visible(rect) {
                                    if resp.hovered() {
                                        ui.painter().rect_filled(rect, Rounding::same(4.0), pal.hover);
                                    }
                                    let inner = rect.shrink2(Vec2::new(6.0, 5.0));
                                    let mut rui = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(Layout::left_to_right(Align::Center)));
                                    rui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                                    let (tr, _) = rui.allocate_exact_size(Vec2::new(72.0, 42.0), Sense::hover());
                                    rui.painter().rect_filled(tr, Rounding::same(3.5), pal.inset);
                                    rui.painter().rect_stroke(tr, Rounding::same(3.5), Stroke::new(1.0_f32, pal.divider));
                                    if let Some(tex) = item.thumb_url.as_ref().and_then(|u| self.workshop_online_thumbs.get(u)) {
                                        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                                        rui.painter().image(tex.id(), tr.shrink(1.0), uv, Color32::WHITE);
                                    }
                                    rui.add_space(8.0);
                                    let text_w = (rui.available_width() - 94.0).max(40.0);
                                    rui.allocate_ui_with_layout(Vec2::new(text_w, inner.height()), Layout::top_down(Align::LEFT), |tui| {
                                        tui.add_space(4.0);
                                        tui.add(egui::Label::new(RichText::new(&item.name).color(pal.text_primary).strong()).truncate());
                                        tui.add(egui::Label::new(RichText::new(&item.source).font(FontId::proportional(11.0)).color(pal.text_muted)).truncate());
                                    });
                                    rui.with_layout(Layout::right_to_left(Align::Center), |bui| {
                                        if item.direct_download {
                                            if bui.add_enabled(!self.is_busy, secondary_action_button(&pal, "Download", pal.accent_green)).clicked() {
                                                self.is_busy = true;
                                                let it_cl = item.clone();
                                                let tx = self.tx.clone();
                                                let ctx_cl = ctx.clone();
                                                self.workshop_dl_progress = Some(WorkshopDownloadProgress {
                                                    name: it_cl.name.clone(),
                                                    downloaded: 0,
                                                    total: None,
                                                    phase: "Connecting...".into(),
                                                });
                                                self.status_text = format!("Downloading '{}'...", it_cl.name);
                                                thread::spawn(move || {
                                                    let name_cl = it_cl.name.clone();
                                                    let mut last_sent = 0u64;
                                                    let mut last_phase = String::new();
                                                    let res = download_online_workshop_map_with_progress(&it_cl, |dl, tot, phase| {
                                                        if dl == 0 || phase != last_phase || dl.saturating_sub(last_sent) >= 65536 {
                                                            last_sent = dl;
                                                            last_phase = phase.to_string();
                                                            let _ = tx.send(WorkerEvent::WorkshopDownloadProgress(WorkshopDownloadProgress {
                                                                name: name_cl.clone(),
                                                                downloaded: dl,
                                                                total: tot,
                                                                phase: phase.to_string(),
                                                            }));
                                                            ctx_cl.request_repaint();
                                                        }
                                                    });
                                                    let _ = tx.send(WorkerEvent::WorkshopDownloadDone(res));
                                                    ctx_cl.request_repaint();
                                                });
                                            }
                                        } else if bui.add(secondary_action_button(&pal, "Steam", pal.accent_blue)).clicked() {
                                            let _ = std::process::Command::new("cmd").args(["/C", "start", "", &item.download_url]).spawn();
                                        }
                                    });
                                }
                            }
                        });
                }
            });
        });
    }

    // ── 8. Presets ───────────────────────────────────────────────────────────
    fn render_presets_tab(&mut self, ui: &mut egui::Ui) {
        let pal = AppPalette::from_ui(ui);
        ui.label(RichText::new("Presets").font(FontId::proportional(20.0)).color(pal.text_primary).strong());
        ui.add_space(6.0);

        app_card_full(ui, |ui| {
            ui.horizontal(|ui| {
                ui.allocate_ui(Vec2::new(220.0, 29.0), |nui| {
                    styled_input_box(nui, &mut self.new_preset_name, "Preset name...", false);
                });
                ui.add_space(6.0);
                if ui.add(primary_action_button("Save Preset", 114.0)).clicked() {
                    let item_swaps: Vec<ActiveSwapRecord> = self
                        .state
                        .active_swaps
                        .values()
                        .filter(|r| !r.donor_pkg.is_empty() && !r.target_pkg.is_empty())
                        .cloned()
                        .collect();
                    let preset = LoadoutPreset {
                        name: self.new_preset_name.trim().to_string(),
                        created_at: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
                        item_swaps,
                        stadium: self.state.last_stadium_setup.clone(),
                    };
                    self.presets.retain(|p| !p.name.eq_ignore_ascii_case(&preset.name));
                    self.presets.push(preset);
                    self.save_presets();
                    self.status_is_error = false;
                    self.status_text = format!("Saved preset '{}'", self.new_preset_name);
                }
                if ui.add(secondary_action_button(&pal, "Import JSON", pal.accent_blue)).clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Preset JSON", &["json"]).pick_file() {
                        if let Ok(txt) = fs::read_to_string(&p) {
                            if let Ok(pr) = serde_json::from_str::<LoadoutPreset>(&txt) {
                                self.presets.push(pr);
                                self.save_presets();
                            }
                        }
                    }
                }
            });
        });

        ui.add_space(8.0);
        let presets_cl = self.presets.clone();
        egui::ScrollArea::vertical()
            .id_salt("presets_list")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if presets_cl.is_empty() {
                    ui.label(RichText::new("No presets saved").color(pal.text_muted));
                }
                for (idx, pr) in presets_cl.iter().enumerate() {
                    app_card(&pal).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let info_w = (ui.available_width() - 250.0).max(80.0);
                            ui.allocate_ui_with_layout(Vec2::new(info_w, 36.0), Layout::top_down(Align::LEFT), |iui| {
                                iui.add(egui::Label::new(RichText::new(&pr.name).font(FontId::proportional(14.0)).color(pal.text_primary).strong()).truncate());
                                iui.add(
                                    egui::Label::new(
                                        RichText::new(format!("{} swaps · {}", pr.item_swaps.len(), pr.created_at))
                                            .font(FontId::proportional(11.0))
                                            .color(pal.text_muted),
                                    )
                                    .truncate(),
                                );
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.add(secondary_action_button(&pal, "Delete", pal.accent_red)).clicked() {
                                    self.presets.remove(idx);
                                    self.save_presets();
                                }
                                if ui.add(secondary_action_button(&pal, "Export", pal.text_secondary)).clicked() {
                                    if let Some(sp) = rfd::FileDialog::new().set_file_name(&format!("{}.json", pr.name.replace(' ', "_"))).save_file() {
                                        if let Ok(js) = serde_json::to_string_pretty(pr) {
                                            let _ = fs::write(sp, js);
                                        }
                                    }
                                }
                                if ui.add_enabled(!self.is_busy, primary_action_button("Apply", 88.0)).clicked() {
                                    self.is_busy = true;
                                    let pr_cl = pr.clone();
                                    let cdir = PathBuf::from(&self.cooked_dir);
                                    let tx = self.tx.clone();
                                    thread::spawn(move || {
                                        let mut applied_recs = Vec::new();
                                        let mut err_list = Vec::new();
                                        for item_rec in &pr_cl.item_swaps {
                                            let donor = CatalogItem {
                                                id: 0,
                                                product: item_rec.donor_desc.clone(),
                                                slot: item_rec.slot.clone(),
                                                asset_package: item_rec.donor_pkg.clone(),
                                                asset_path: item_rec.donor_asset_path.clone(),
                                                quality: String::new(),
                                            };
                                            let target = CatalogItem {
                                                id: 0,
                                                product: item_rec.target_desc.clone(),
                                                slot: item_rec.slot.clone(),
                                                asset_package: item_rec.target_pkg.clone(),
                                                asset_path: item_rec.target_asset_path.clone(),
                                                quality: String::new(),
                                            };
                                            let paint = item_rec.paint_hex.as_deref().map(|hx| PaintColorSpec {
                                                name: item_rec.paint_name.clone().unwrap_or_else(|| hx.to_string()),
                                                hex_srgb: hx.to_string(),
                                                linear_rgb: PaintColorSpec::srgb_hex_to_linear(hx).1,
                                                is_custom: true,
                                            });
                                            match swap_item_packages(&cdir, &donor, &target, paint.as_ref()) {
                                                Ok(_) => applied_recs.push(item_rec.clone()),
                                                Err(e) => err_list.push(e),
                                            }
                                        }
                                        if let Some(ref st) = pr_cl.stadium {
                                            let d = st.donor_upk.as_deref().unwrap_or(&st.target_upk);
                                            if let Err(e) = swap_standard_map(&cdir, d, &st.target_upk, st.override_all_57) {
                                                err_list.push(e);
                                            }
                                        }
                                        let _ = tx.send(WorkerEvent::OperationDone {
                                            success: err_list.is_empty(),
                                            message: if err_list.is_empty() {
                                                format!("Applied '{}'", pr_cl.name)
                                            } else {
                                                err_list.join("; ")
                                            },
                                            swapped_pkgs: applied_recs,
                                            cleared_pkgs: vec![],
                                        });
                                    });
                                }
                            });
                        });
                    });
                    ui.add_space(6.0);
                }
            });
    }

    // ── 9. Active Swaps & Game Integrity ─────────────────────────────────────
    fn render_active_health_tab(&mut self, ui: &mut egui::Ui) {
        let pal = AppPalette::from_ui(ui);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Active & Health").font(FontId::proportional(20.0)).color(pal.text_primary).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add(secondary_action_button(&pal, "Reset All Vanilla", pal.accent_red)).clicked() {
                    self.reset_all_swaps();
                }
                if ui.add(secondary_action_button(&pal, "Repair", pal.accent_blue)).clicked() {
                    let (synced, healed) = repair_cooked_health(Path::new(&self.cooked_dir));
                    self.refresh_health_report();
                    self.status_text = format!("Repaired {} backups · {} borders", synced, healed);
                }
                if ui.add(secondary_action_button(&pal, "Clear Cache", pal.text_secondary)).clicked() {
                    clear_rl_texture_cache();
                    self.refresh_health_report();
                    self.status_text = "Cleared RL cache".into();
                }
            });
        });

        ui.add_space(6.0);
        if let Some(ref rep) = self.health_report {
            app_card_full(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("Backups: {}", rep.total_bak_files)).color(pal.text_primary).strong());
                    ui.label(RichText::new(format!("· Cache: {}", rep.cache_file_count)).color(pal.text_secondary));
                    ui.label(
                        RichText::new(format!(
                            "· Underpass: {}",
                            if rep.underpass_swapped { "Workshop" } else { "Vanilla" }
                        ))
                        .color(pal.text_secondary),
                    );
                    if !rep.damaged_avatar_borders.is_empty() {
                        ui.label(RichText::new(format!("· Damaged Borders: {}", rep.damaged_avatar_borders.len())).color(pal.accent_amber));
                    }
                });
            });
        }

        ui.add_space(8.0);
        let keys: Vec<String> = self.state.active_swaps.keys().cloned().collect();
        let disk_baks = self.health_report.as_ref().map(|r| r.bak_filenames.clone()).unwrap_or_default();

        app_list_card(&pal).show(ui, |ui| {
            let avail_h = ui.available_height().max(220.0);
            egui::ScrollArea::vertical()
                .id_salt("active_health_list")
                .auto_shrink([false, false])
                .max_height((avail_h - 14.0).max(160.0))
                .show(ui, |ui| {
                    if keys.is_empty() && disk_baks.is_empty() {
                        ui.label(RichText::new("All files vanilla").color(pal.accent_green));
                    }
                    for k in &keys {
                        if let Some(rec) = self.state.active_swaps.get(k).cloned() {
                            let label = format!("{} → {}", rec.donor_desc, rec.target_desc);
                            ui.horizontal(|ui| {
                                let w = (ui.available_width() - 90.0).max(80.0);
                                ui.allocate_ui(Vec2::new(w, 24.0), |lui| {
                                    lui.add(egui::Label::new(RichText::new(label).color(pal.text_primary)).truncate());
                                });
                                ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                                    if rui.add(secondary_action_button(&pal, "Restore", pal.accent_red)).clicked() {
                                        restore_single_package(&Path::new(&self.cooked_dir).join(&rec.target_pkg));
                                        clear_rl_texture_cache();
                                        self.state.active_swaps.remove(k);
                                        self.save_state();
                                        self.refresh_health_report();
                                    }
                                });
                            });
                        }
                    }
                    for pkg in &disk_baks {
                        if !self.state.active_swaps.contains_key(&pkg.to_lowercase()) {
                            ui.horizontal(|ui| {
                                let w = (ui.available_width() - 90.0).max(80.0);
                                ui.allocate_ui(Vec2::new(w, 24.0), |lui| {
                                    lui.add(egui::Label::new(RichText::new(pkg).color(pal.text_secondary)).truncate());
                                });
                                ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                                    if rui.add(secondary_action_button(&pal, "Restore", pal.accent_red)).clicked() {
                                        restore_single_package(&Path::new(&self.cooked_dir).join(pkg));
                                        clear_rl_texture_cache();
                                        self.refresh_health_report();
                                    }
                                });
                            });
                        }
                    }
                });
        });
    }

    // ── Quick Reset Modal ────────────────────────────────────────────────────
    fn render_reset_modal(&mut self, ctx: &egui::Context) {
        let pal = AppPalette::from_dark(ctx.theme() == egui::Theme::Dark);
        let mut open = self.show_reset_modal;
        egui::Window::new("Active Swaps")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .fixed_size(Vec2::new(500.0, 340.0))
            .show(ctx, |ui| {
                let keys: Vec<String> = self.state.active_swaps.keys().cloned().collect();
                if keys.is_empty() {
                    ui.label(RichText::new("No active swaps").color(pal.accent_green));
                } else {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .max_height(240.0)
                        .show(ui, |ui| {
                            for k in &keys {
                                if let Some(rec) = self.state.active_swaps.get(k).cloned() {
                                    ui.horizontal(|ui| {
                                        let w = (ui.available_width() - 80.0).max(80.0);
                                        ui.allocate_ui(Vec2::new(w, 24.0), |lui| {
                                            lui.add(egui::Label::new(RichText::new(format!("{} → {}", rec.donor_desc, rec.target_desc)).color(pal.text_primary)).truncate());
                                        });
                                        if ui.add(secondary_action_button(&pal, "Restore", pal.accent_red)).clicked() {
                                            restore_single_package(&Path::new(&self.cooked_dir).join(&rec.target_pkg));
                                            clear_rl_texture_cache();
                                            self.state.active_swaps.remove(k);
                                            self.save_state();
                                            self.refresh_health_report();
                                        }
                                    });
                                }
                            }
                        });
                    ui.add_space(10.0);
                    if ui.add(secondary_action_button(&pal, "Reset All to Vanilla", pal.accent_red).min_size(Vec2::new(480.0, 32.0))).clicked() {
                        self.reset_all_swaps();
                    }
                }
            });
        self.show_reset_modal = open;
    }
}

fn main() -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("RL Swapper")
        .with_inner_size([1160.0, 740.0])
        .with_min_inner_size([920.0, 580.0]);

    let logo_bytes = include_bytes!("../../assets/logo.png");
    if let Some(rounded_rgba) = make_rounded_icon_rgba(logo_bytes, 64) {
        viewport = viewport.with_icon(Arc::new(egui::IconData {
            rgba: rounded_rgba.into_raw(),
            width: 64,
            height: 64,
        }));
    }

    eframe::run_native(
        "RL Swapper",
        eframe::NativeOptions { viewport, ..Default::default() },
        Box::new(|cc| Ok(Box::new(RLSwapperGui::new(cc)))),
    )
}
