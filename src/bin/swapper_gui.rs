#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use base64::Engine;
use eframe::egui::{
    self, Align, Color32, ColorImage, FontId, Layout, RichText, Rounding, Sense, Stroke,
    TextureHandle, TextureOptions, Vec2,
};
use image::{imageops, imageops::FilterType, ImageFormat, RgbaImage};
use rl_swapper::custom_studio::{
    compose_pfp_with_border_preview, inject_alphaconsole_ball, inject_alphaconsole_decal_at,
    inject_custom_banner, inject_custom_pfp, list_avatar_border_packages_with_catalog,
    list_player_banner_packages_with_catalog, parse_alphaconsole_ball_source,
    parse_alphaconsole_decal_source, AlphaConsoleBallPack, AlphaConsoleDecalPack,
};
use rl_swapper::icon_system::{
    composite_icon_with_paint, extract_ball_texture_preview, IconCacheManager,
};
use rl_swapper::templates::{
    bake_name_onto_banner, crop_and_resize_rgba, compute_crop_rect, generate_ball_template,
    generate_banner_template, generate_decal_template_for_car, render_2d_ball_orb_preview,
    render_2d_banner_scoreboard_preview, render_2d_car_decal_with_mask_preview,
};
use rl_swapper::upk_engine::{
    append_swap_history_log, bak_path_for, body_hitbox_for_item, clear_rl_texture_cache,
    detect_car_uv_label, get_appdata_dir, get_custom_items_dir, get_presets_dir,
    get_swapper_hub_dir, inspect_cooked_health, is_avatar_border_modified, is_rocket_league_running,
    load_catalog_items, official_paints, repair_cooked_health, restore_single_package,
    swap_item_packages, vault_backup_path, CarHitbox, CatalogItem, CookedHealthReport,
    PaintColorSpec,
};
use rl_swapper::workshop::{
    count_cached_workshop_textures, delete_library_map,
    download_and_install_workshop_textures_with_progress, download_online_workshop_map_with_progress,
    fetch_all_online_workshop_maps, fetch_or_cache_thumb, find_library_map_by_id_or_slug,
    get_workshop_library_dir, get_workshop_textures_dir, import_workshop_file_or_zip,
    install_workshop_textures_folder, is_workshop_textures_item, list_library_maps,
    load_workshop_map_into_underpass, restore_workshop_underpass, scan_upk_external_dependencies,
    OnlineWorkshopItem, WorkshopMapEntry, WORKSHOP_SAFE_SLOT,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

// ── Ultra-Minimalist Barebones Black Tokens ──────────────────────────────────
const BG_CANVAS: Color32 = Color32::from_rgb(7, 7, 10);
const BG_SURFACE: Color32 = Color32::from_rgb(16, 17, 22);
const BG_CARD: Color32 = Color32::from_rgb(22, 23, 30);
const BG_WELL: Color32 = Color32::from_rgb(11, 12, 16);
const BG_HOVER: Color32 = Color32::from_rgb(31, 33, 43);
const BG_SELECTED: Color32 = Color32::from_rgb(28, 30, 40);
const BORDER_SUBTLE: Color32 = Color32::from_rgb(33, 35, 46);
const BORDER_ACTIVE: Color32 = Color32::from_rgb(78, 83, 102);

const TEXT_PRIMARY: Color32 = Color32::from_rgb(244, 245, 249);
const TEXT_SECONDARY: Color32 = Color32::from_rgb(148, 153, 172);
const TEXT_MUTED: Color32 = Color32::from_rgb(92, 96, 114);

const WHITE_CTA_BG: Color32 = Color32::from_rgb(245, 246, 250);
const WHITE_CTA_HOVER: Color32 = Color32::WHITE;
const WHITE_CTA_ACTIVE: Color32 = Color32::from_rgb(214, 217, 226);
const WHITE_CTA_FG: Color32 = Color32::from_rgb(9, 10, 14);

const MINT: Color32 = Color32::from_rgb(112, 243, 152);
const LEMON: Color32 = Color32::from_rgb(245, 232, 104);
const CORAL: Color32 = Color32::from_rgb(242, 104, 104);

const MAX_GPU_ICONS: usize = 160;
const MAX_WS_THUMBS: usize = 96;
const MAX_FAILED_THUMBS: usize = 256;
const MAX_INFLIGHT_ICONS: usize = 8;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveTab {
    Cosmetics,
    Workshop,
    Decals,
    AvatarPfp,
    Banner,
    MatchBall,
    Presets,
    Health,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CropTarget {
    Pfp,
    Banner,
}

impl CropTarget {
    fn aspect(self) -> f32 {
        match self {
            Self::Pfp => 1.0,
            Self::Banner => 4.2,
        }
    }

    fn output_dims(self, rw: u32) -> (u32, u32) {
        match self {
            Self::Pfp => {
                let d = rw.clamp(128, 512);
                (d, d)
            }
            Self::Banner => (840, 200),
        }
    }
}

struct ImageCropModal {
    target: CropTarget,
    filename: String,
    source_path: Option<PathBuf>,
    source_img: RgbaImage,
    preview_tex: TextureHandle,
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
}

struct IconJob {
    cache_key: String,
    cooked_dir: PathBuf,
    item: CatalogItem,
    paint: Option<PaintColorSpec>,
    sz: u32,
}

#[derive(Debug, Clone)]
struct WorkshopDownloadProgress {
    pub name: String,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub phase: String,
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
struct ActiveCustomDecalRef {
    pub name: String,
    pub target_pkg: String,
    pub target_desc: String,
    #[serde(default)]
    pub target_asset_path: String,
    #[serde(default)]
    pub preferred_donor_pkg: Option<String>,
    #[serde(default)]
    pub skin_file: Option<String>,
    #[serde(default)]
    pub diffuse_base_file: Option<String>,
    #[serde(default)]
    pub mask_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ActiveCustomPfpRef {
    pub border_pkg: String,
    pub border_desc: String,
    pub image_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ActiveCustomBannerRef {
    pub banner_pkg: String,
    pub banner_desc: String,
    pub image_file: String,
    #[serde(default)]
    pub player_name: String,
    #[serde(default)]
    pub bake_name: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ActiveCustomBallRef {
    pub name: String,
    pub diffuse_file: String,
    #[serde(default)]
    pub normal_file: Option<String>,
    #[serde(default)]
    pub mask_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ActiveStudioState {
    #[serde(default)]
    pub decal: Option<ActiveCustomDecalRef>,
    #[serde(default)]
    pub pfp: Option<ActiveCustomPfpRef>,
    #[serde(default)]
    pub banner: Option<ActiveCustomBannerRef>,
    #[serde(default)]
    pub ball: Option<ActiveCustomBallRef>,
    #[serde(default)]
    pub workshop_map_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct AppPersistentState {
    pub cooked_dir: String,
    pub rl_exe_mtime: u64,
    pub rl_exe_size: u64,
    pub active_swaps: HashMap<String, ActiveSwapRecord>,
    pub swap_history: Vec<String>,
    #[serde(default)]
    pub active_studio: ActiveStudioState,
    #[serde(default)]
    pub favorite_pkgs: HashSet<String>,
    #[serde(default)]
    pub active_preset_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PresetCustomDecal {
    pub name: String,
    pub target_pkg: String,
    pub target_desc: String,
    #[serde(default)]
    pub target_asset_path: String,
    #[serde(default)]
    pub preferred_donor_pkg: Option<String>,
    #[serde(default)]
    pub skin_png_b64: Option<String>,
    #[serde(default)]
    pub diffuse_base_png_b64: Option<String>,
    #[serde(default)]
    pub mask_png_b64: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PresetCustomPfp {
    pub border_pkg: String,
    pub border_desc: String,
    pub image_png_b64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PresetCustomBanner {
    pub banner_pkg: String,
    pub banner_desc: String,
    pub image_png_b64: String,
    #[serde(default)]
    pub player_name: String,
    #[serde(default)]
    pub bake_name: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PresetCustomBall {
    pub name: String,
    pub diffuse_png_b64: String,
    #[serde(default)]
    pub normal_png_b64: Option<String>,
    #[serde(default)]
    pub mask_png_b64: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LoadoutPreset {
    pub name: String,
    pub created_at: String,
    #[serde(default)]
    pub item_swaps: Vec<ActiveSwapRecord>,
    #[serde(default)]
    pub custom_decal: Option<PresetCustomDecal>,
    #[serde(default)]
    pub custom_pfp: Option<PresetCustomPfp>,
    #[serde(default)]
    pub custom_banner: Option<PresetCustomBanner>,
    #[serde(default)]
    pub custom_ball: Option<PresetCustomBall>,
    #[serde(default)]
    pub workshop_map_id: Option<String>,
}

#[derive(Default)]
struct StudioStateDelta {
    decal: Option<ActiveCustomDecalRef>,
    pfp: Option<ActiveCustomPfpRef>,
    banner: Option<ActiveCustomBannerRef>,
    ball: Option<ActiveCustomBallRef>,
    workshop_map_id: Option<String>,
}

enum WorkerEvent {
    HealthReady(CookedHealthReport),
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
    WorkshopThumbDone {
        url: String,
        rgba: Option<RgbaImage>,
    },
    WorkshopDownloadProgress(WorkshopDownloadProgress),
    WorkshopDownloadDone(Result<(WorkshopMapEntry, Vec<String>), String>),
    WorkshopTexturesDone(Result<(usize, usize, usize), String>),
    WorkshopLoadDone {
        success: bool,
        message: String,
        rec: Option<ActiveSwapRecord>,
        missing_deps: Vec<String>,
        workshop_map_id: Option<String>,
    },
    OperationDone {
        success: bool,
        message: String,
        swapped_pkgs: Vec<ActiveSwapRecord>,
        cleared_pkgs: Vec<String>,
        studio_delta: Option<StudioStateDelta>,
    },
}

fn configure_minimal_black_style(ctx: &egui::Context) {
    let mut style = egui::Style::default();
    style.wrap_mode = Some(egui::TextWrapMode::Truncate);
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(10.0, 5.0);
    style.spacing.interact_size.y = 28.0;
    style.spacing.window_margin = egui::Margin::same(14.0);
    style.spacing.combo_height = 280.0;
    style.spacing.scroll = egui::style::ScrollStyle {
        floating: false,
        bar_width: 4.0,
        handle_min_length: 24.0,
        bar_inner_margin: 4.0,
        bar_outer_margin: 4.0,
        floating_width: 4.0,
        floating_allocated_width: 4.0,
        foreground_color: false,
        dormant_background_opacity: 0.0,
        active_background_opacity: 0.16,
        interact_background_opacity: 0.25,
        dormant_handle_opacity: 0.26,
        active_handle_opacity: 0.58,
        interact_handle_opacity: 0.85,
    };

    let mut v = egui::Visuals::dark();
    v.dark_mode = true;
    v.window_fill = BG_SURFACE;
    v.panel_fill = BG_CANVAS;
    v.faint_bg_color = BG_WELL;
    v.extreme_bg_color = BG_WELL;
    v.window_rounding = Rounding::same(10.0);
    v.window_stroke = Stroke::new(1.0_f32, BORDER_SUBTLE);
    v.menu_rounding = Rounding::same(8.0);
    v.popup_shadow = egui::Shadow::NONE;
    v.window_shadow = egui::Shadow::NONE;

    v.widgets.noninteractive.bg_fill = BG_SURFACE;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER_SUBTLE);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT_SECONDARY);
    v.widgets.noninteractive.rounding = Rounding::same(7.0);

    v.widgets.inactive.bg_fill = BG_CARD;
    v.widgets.inactive.weak_bg_fill = BG_CARD;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER_SUBTLE);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT_SECONDARY);
    v.widgets.inactive.rounding = Rounding::same(7.0);

    v.widgets.hovered.bg_fill = BG_HOVER;
    v.widgets.hovered.weak_bg_fill = BG_HOVER;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, BORDER_ACTIVE);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, TEXT_PRIMARY);
    v.widgets.hovered.rounding = Rounding::same(7.0);

    v.widgets.active.bg_fill = BG_SELECTED;
    v.widgets.active.weak_bg_fill = BG_SELECTED;
    v.widgets.active.bg_stroke = Stroke::new(1.0_f32, BORDER_ACTIVE);
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, TEXT_PRIMARY);
    v.widgets.active.rounding = Rounding::same(7.0);

    v.selection.bg_fill = BG_HOVER;
    v.selection.stroke = Stroke::new(1.0_f32, TEXT_PRIMARY);

    style.visuals = v;
    ctx.set_style(style);
}

fn rgba_to_egui_image(img: &RgbaImage) -> ColorImage {
    ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw())
}

fn make_rounded_icon_rgba(bytes: &[u8], size: u32) -> Option<RgbaImage> {
    let mut out = image::load_from_memory(bytes)
        .ok()?
        .resize_exact(size, size, FilterType::Triangle)
        .to_rgba8();
    let r = (size as f32) * 0.22;
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

/// Zero-texture top dark ambiance (4-vertex quad mesh fading smoothly into pure pitch black).
fn paint_top_vignette(ctx: &egui::Context) {
    let sr = ctx.screen_rect();
    if !sr.is_positive() {
        return;
    }
    let painter = ctx.layer_painter(egui::LayerId::background());
    painter.rect_filled(sr, Rounding::ZERO, BG_CANVAS);
    let mid_y = (sr.top() + 260.0).min(sr.bottom());
    let top_col = Color32::from_rgb(16, 13, 24);
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(sr.left_top(), top_col);
    mesh.colored_vertex(sr.right_top(), top_col);
    mesh.colored_vertex(egui::pos2(sr.right(), mid_y), BG_CANVAS);
    mesh.colored_vertex(egui::pos2(sr.left(), mid_y), BG_CANVAS);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

fn surface_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(BG_SURFACE)
        .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
        .rounding(Rounding::same(10.0))
        .inner_margin(egui::Margin::same(14.0))
}

fn list_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(BG_WELL)
        .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
        .rounding(Rounding::same(9.0))
        .inner_margin(egui::Margin::same(8.0))
}

fn well_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(BG_WELL)
        .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
        .rounding(Rounding::same(8.0))
        .inner_margin(egui::Margin::same(8.0))
}

fn folder_tab_btn(ui: &mut egui::Ui, active: bool, label: &str) -> bool {
    let w = (label.len() as f32 * 7.2 + 24.0).clamp(58.0, 136.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 28.0), Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if ui.is_rect_visible(rect) {
        let fill = if active {
            BG_SURFACE
        } else if resp.hovered() {
            BG_CARD
        } else {
            Color32::from_rgb(12, 13, 17)
        };
        let stroke = if active { BORDER_ACTIVE } else { BORDER_SUBTLE };
        ui.painter().rect_filled(rect, Rounding::same(8.0), fill);
        ui.painter().rect_stroke(rect, Rounding::same(8.0), Stroke::new(1.0_f32, stroke));
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            FontId::proportional(12.2),
            if active { TEXT_PRIMARY } else { TEXT_SECONDARY },
        );
    }
    resp.clicked()
}

fn white_cta_button(ui: &mut egui::Ui, enabled: bool, text: &str, min_w: f32) -> bool {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(min_w.max(86.0), 32.0), sense);
    if enabled && resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if ui.is_rect_visible(rect) {
        let (fill, stroke, tc) = if !enabled {
            (BG_CARD, Stroke::new(1.0_f32, BORDER_SUBTLE), TEXT_MUTED)
        } else if resp.is_pointer_button_down_on() {
            (WHITE_CTA_ACTIVE, Stroke::NONE, WHITE_CTA_FG)
        } else if resp.hovered() {
            (WHITE_CTA_HOVER, Stroke::NONE, WHITE_CTA_FG)
        } else {
            (WHITE_CTA_BG, Stroke::NONE, WHITE_CTA_FG)
        };
        ui.painter().rect_filled(rect, Rounding::same(16.0), fill);
        if !enabled {
            ui.painter().rect_stroke(rect, Rounding::same(16.0), stroke);
        }
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            FontId::proportional(12.5),
            tc,
        );
    }
    enabled && resp.clicked()
}

fn pill_button<'a>(text: &str, color: Color32) -> egui::Button<'a> {
    egui::Button::new(RichText::new(text).font(FontId::proportional(11.8)).color(color))
        .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
        .rounding(Rounding::same(14.0))
        .min_size(Vec2::new(0.0, 28.0))
}

fn nav_tab_button<'a>(text: &str, active: bool) -> egui::Button<'a> {
    egui::Button::new(
        RichText::new(text)
            .font(FontId::proportional(12.3))
            .color(if active { TEXT_PRIMARY } else { TEXT_SECONDARY }),
    )
    .fill(if active { BG_CARD } else { Color32::TRANSPARENT })
    .stroke(if active { Stroke::new(1.0_f32, BORDER_SUBTLE) } else { Stroke::NONE })
    .rounding(Rounding::same(14.0))
    .min_size(Vec2::new(0.0, 28.0))
}

fn status_badge(ui: &mut egui::Ui, text: &str, color: Color32, solid_tint: bool) {
    let fill = if solid_tint {
        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 26)
    } else {
        BG_CARD
    };
    let stroke = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 72);
    egui::Frame::none()
        .fill(fill)
        .stroke(Stroke::new(1.0_f32, stroke))
        .rounding(Rounding::same(14.0))
        .inner_margin(egui::Margin::symmetric(10.0, 4.0))
        .show(ui, |ui| {
            ui.add(
                egui::Label::new(RichText::new(text).font(FontId::proportional(10.5)).color(color).strong())
                    .selectable(false),
            );
        });
}

fn paint_dot(ui: &mut egui::Ui, color: Color32, radius: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(radius * 2.0 + 4.0), Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().circle_filled(rect.center(), radius, color);
    }
}

fn meta_col(ui: &mut egui::Ui, label: &str, value: &str, val_col: Color32) -> egui::Response {
    ui.vertical(|vui| {
        vui.spacing_mut().item_spacing.y = 0.0;
        vui.label(RichText::new(label).font(FontId::proportional(9.5)).color(TEXT_MUTED));
        vui.add(egui::Label::new(RichText::new(value).font(FontId::proportional(12.0)).color(val_col)).truncate());
    })
    .response
}

fn minimal_switch(ui: &mut egui::Ui, checked: &mut bool, label: &str) -> bool {
    let mut toggled = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 7.0;
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(32.0, 17.0), Sense::click());
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            *checked = !*checked;
            toggled = true;
        }
        if ui.is_rect_visible(rect) {
            let bg = if *checked { WHITE_CTA_BG } else { BG_CARD };
            let br = if *checked { WHITE_CTA_BG } else if resp.hovered() { BORDER_ACTIVE } else { BORDER_SUBTLE };
            ui.painter().rect_filled(rect, Rounding::same(8.5), bg);
            ui.painter().rect_stroke(rect, Rounding::same(8.5), Stroke::new(1.0_f32, br));
            let kx = if *checked { rect.right() - 14.0 } else { rect.left() + 3.0 };
            let knob = egui::Rect::from_min_size(egui::pos2(kx, rect.center().y - 5.5), Vec2::splat(11.0));
            ui.painter().rect_filled(
                knob,
                Rounding::same(5.5),
                if *checked { WHITE_CTA_FG } else { TEXT_SECONDARY },
            );
        }
        let lr = ui.add(
            egui::Label::new(
                RichText::new(label)
                    .font(FontId::proportional(11.8))
                    .color(if *checked { TEXT_PRIMARY } else { TEXT_SECONDARY }),
            )
            .sense(Sense::click()),
        );
        if lr.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if lr.clicked() {
            *checked = !*checked;
            toggled = true;
        }
    });
    toggled
}

/// Hairline search field styled cleanly without heavy outer boxes.
fn search_line(ui: &mut egui::Ui, buf: &mut String, hint: &str) -> bool {
    let mut changed = false;
    let avail = ui.available_width().max(120.0);
    let fid = ui.id().with(buf as *const String as usize);
    let focused = ui.data(|d| d.get_temp::<bool>(fid).unwrap_or(false));
    let mut now_focus = false;

    let resp = egui::Frame::none()
        .fill(Color32::TRANSPARENT)
        .inner_margin(egui::Margin::symmetric(2.0, 5.0))
        .show(ui, |ui| {
            ui.set_width((avail - 4.0).max(80.0));
            ui.horizontal(|ui| {
                let (ir, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                if ui.is_rect_visible(ir) {
                    let c = ir.center() - Vec2::splat(1.2);
                    let st = Stroke::new(1.3_f32, if focused { TEXT_PRIMARY } else { TEXT_MUTED });
                    ui.painter().circle_stroke(c, 4.0, st);
                    ui.painter().line_segment([c + Vec2::splat(2.8), c + Vec2::splat(5.8)], st);
                }
                ui.add_space(4.0);
                let tw = (ui.available_width() - if buf.is_empty() { 0.0 } else { 18.0 }).max(40.0);
                let te = egui::TextEdit::singleline(buf)
                    .hint_text(RichText::new(hint).color(TEXT_MUTED))
                    .frame(false)
                    .desired_width(tw);
                let tr = ui.add(te);
                now_focus = tr.has_focus();
                if tr.changed() {
                    changed = true;
                }
                if !buf.is_empty() {
                    let (cr, cresp) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::click());
                    if cresp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if ui.is_rect_visible(cr) {
                        let st = Stroke::new(1.3_f32, if cresp.hovered() { TEXT_PRIMARY } else { TEXT_MUTED });
                        let c = cr.center();
                        let d = 3.0;
                        ui.painter().line_segment([egui::pos2(c.x - d, c.y - d), egui::pos2(c.x + d, c.y + d)], st);
                        ui.painter().line_segment([egui::pos2(c.x + d, c.y - d), egui::pos2(c.x - d, c.y + d)], st);
                    }
                    if cresp.clicked() {
                        buf.clear();
                        changed = true;
                    }
                }
            });
        });

    let r = resp.response.rect;
    let line_col = if focused { TEXT_SECONDARY } else { BORDER_SUBTLE };
    ui.painter().line_segment(
        [egui::pos2(r.left(), r.bottom()), egui::pos2(r.right(), r.bottom())],
        Stroke::new(1.0_f32, line_col),
    );
    ui.data_mut(|d| d.insert_temp(fid, now_focus));
    changed
}

fn minimal_text_box(ui: &mut egui::Ui, buf: &mut String, hint: &str) -> bool {
    let mut changed = false;
    let avail = ui.available_width().max(90.0);
    egui::Frame::none()
        .fill(BG_CARD)
        .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
        .rounding(Rounding::same(7.0))
        .inner_margin(egui::Margin::symmetric(9.0, 5.0))
        .show(ui, |ui| {
            let te = egui::TextEdit::singleline(buf)
                .hint_text(RichText::new(hint).color(TEXT_MUTED))
                .frame(false)
                .desired_width((avail - 18.0).max(50.0));
            if ui.add(te).changed() {
                changed = true;
            }
        });
    changed
}

fn minimal_row_ex(
    ui: &mut egui::Ui,
    selected: bool,
    row_h: f32,
    icon: Option<egui::TextureId>,
    icon_sz: f32,
    title: &str,
    tag: Option<(&str, Color32)>,
) -> (bool, bool) {
    let avail_w = ui.available_width().max(80.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(avail_w, row_h), Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if ui.is_rect_visible(rect) {
        if selected {
            ui.painter().rect_filled(rect, Rounding::same(7.0), BG_SELECTED);
            ui.painter().rect_stroke(rect, Rounding::same(7.0), Stroke::new(1.0_f32, BORDER_ACTIVE));
        } else if resp.hovered() {
            ui.painter().rect_filled(rect, Rounding::same(7.0), BG_HOVER);
        }

        let inner = rect.shrink2(Vec2::new(8.0, 3.0));
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(inner)
                .layout(Layout::left_to_right(Align::Center)),
        );
        child.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);

        if icon_sz > 0.0 {
            let (ir, _) = child.allocate_exact_size(Vec2::splat(icon_sz), Sense::hover());
            child.painter().rect_filled(ir, Rounding::same(5.0), BG_WELL);
            child.painter().rect_stroke(ir, Rounding::same(5.0), Stroke::new(1.0_f32, BORDER_SUBTLE));
            if let Some(tid) = icon {
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                child.painter().image(tid, ir.shrink(1.5), uv, Color32::WHITE);
            }
            child.add_space(8.0);
        }

        let tag_w = tag.as_ref().map(|(t, _)| t.len() as f32 * 6.5 + 18.0).unwrap_or(0.0);
        let title_w = (child.available_width() - tag_w).max(40.0);
        child.allocate_ui_with_layout(Vec2::new(title_w, inner.height()), Layout::left_to_right(Align::Center), |tui| {
            tui.add(
                egui::Label::new(
                    RichText::new(title)
                        .font(FontId::proportional(12.4))
                        .color(if selected { TEXT_PRIMARY } else { TEXT_SECONDARY }),
                )
                .truncate()
                .selectable(false),
            );
        });

        if let Some((t_txt, t_col)) = tag {
            child.with_layout(Layout::right_to_left(Align::Center), |rui| {
                rui.add(
                    egui::Label::new(RichText::new(t_txt).font(FontId::proportional(10.5)).color(t_col))
                        .selectable(false),
                );
            });
        }
    }
    if resp.hovered() && title.len() > 30 {
        resp.clone().on_hover_text(title);
    }
    (resp.clicked(), resp.double_clicked())
}

fn minimal_row(
    ui: &mut egui::Ui,
    selected: bool,
    row_h: f32,
    icon: Option<egui::TextureId>,
    icon_sz: f32,
    title: &str,
    tag: Option<(&str, Color32)>,
) -> bool {
    minimal_row_ex(ui, selected, row_h, icon, icon_sz, title, tag).0
}

/// Renders a dropdown menu row with a 10x10 paint color swatch indicator.
fn paint_swatch_row(ui: &mut egui::Ui, selected: bool, label: &str, rgb: Option<[u8; 3]>) -> bool {
    let avail_w = ui.available_width().max(110.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(avail_w, 24.0), Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if ui.is_rect_visible(rect) {
        if selected {
            ui.painter().rect_filled(rect, Rounding::same(5.0), BG_SELECTED);
        } else if resp.hovered() {
            ui.painter().rect_filled(rect, Rounding::same(5.0), BG_HOVER);
        }
        let inner = rect.shrink2(Vec2::new(8.0, 3.0));
        let sw = egui::Rect::from_min_size(egui::pos2(inner.left(), inner.center().y - 5.0), Vec2::splat(10.0));
        if let Some([r, g, b]) = rgb {
            ui.painter().rect_filled(sw, Rounding::same(3.0), Color32::from_rgb(r, g, b));
            ui.painter().rect_stroke(sw, Rounding::same(3.0), Stroke::new(1.0_f32, BORDER_ACTIVE));
        } else {
            ui.painter().rect_filled(sw, Rounding::same(3.0), BG_WELL);
            ui.painter().rect_stroke(sw, Rounding::same(3.0), Stroke::new(1.0_f32, BORDER_SUBTLE));
        }
        ui.painter().text(
            egui::pos2(sw.right() + 8.0, inner.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            FontId::proportional(12.0),
            if selected { TEXT_PRIMARY } else { TEXT_SECONDARY },
        );
    }
    resp.clicked()
}

fn paint_empty_preview(ui: &mut egui::Ui, size: Vec2, title: &str, subtitle: &str) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        let c = rect.center();
        ui.painter().circle_stroke(egui::pos2(c.x, c.y - 18.0), 22.0, Stroke::new(1.0_f32, BORDER_SUBTLE));
        ui.painter().text(egui::pos2(c.x, c.y + 18.0), egui::Align2::CENTER_CENTER, title, FontId::proportional(12.5), TEXT_SECONDARY);
        ui.painter().text(egui::pos2(c.x, c.y + 36.0), egui::Align2::CENTER_CENTER, subtitle, FontId::proportional(11.0), TEXT_MUTED);
    }
}

fn find_item_by_id_in_slots(slots: &HashMap<String, Vec<CatalogItem>>, id: i64) -> Option<CatalogItem> {
    slots.values().flatten().find(|it| it.id == id).cloned()
}

/// Inserts a cached key into `map` while maintaining bounded FIFO eviction without clearing hot items.
fn evict_fifo_insert<V>(map: &mut HashMap<String, V>, order: &mut VecDeque<String>, max_len: usize, key: String, val: V) {
    if !map.contains_key(&key) {
        while map.len() >= max_len {
            if let Some(old) = order.pop_front() {
                map.remove(&old);
            } else {
                map.clear();
                break;
            }
        }
        order.push_back(key.clone());
    }
    map.insert(key, val);
}

fn fmt_compact_count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", (n as f64) / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", (n as f64) / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Checks whether a package has either an active swap record or a `.upk.bak` backup in `cooked_dir`.
fn is_pkg_swapped_in_state(pkg_name: &str, active_swaps: &HashMap<String, ActiveSwapRecord>, cooked_dir: &Path) -> bool {
    let low = pkg_name.to_lowercase();
    active_swaps.contains_key(&low)
        || bak_path_for(&cooked_dir.join(pkg_name)).exists()
        || (low.starts_with("avatarborder_") && is_avatar_border_modified(&cooked_dir.join(pkg_name)))
}

/// Filters catalog item indices by search query, optional Body hitbox, and optional Swapped-Only state.
fn filter_cosmetic_indices(
    list: &[CatalogItem],
    query_low: &str,
    hb_filter: Option<CarHitbox>,
    swapped_only: bool,
    active_swaps: &HashMap<String, ActiveSwapRecord>,
    cooked_dir: &Path,
) -> Vec<usize> {
    list.iter()
        .enumerate()
        .filter(|(_, it)| {
            let q_ok = query_low.is_empty()
                || it.product.to_lowercase().contains(query_low)
                || it.asset_package.to_lowercase().contains(query_low);
            let hb_ok = hb_filter.map_or(true, |hb| body_hitbox_for_item(&it.product, &it.asset_package) == hb);
            let sw_ok = !swapped_only || is_pkg_swapped_in_state(&it.asset_package, active_swaps, cooked_dir);
            q_ok && hb_ok && sw_ok
        })
        .map(|(i, _)| i)
        .collect()
}

/// Filters and sorts online workshop map indices by query, 1-click availability, source, and sort order.
fn filter_sort_workshop_indices(
    items: &[OnlineWorkshopItem],
    query_low: &str,
    direct_only: bool,
    source_filter: usize,
    sort_mode: usize,
) -> Vec<usize> {
    let mut idxs: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, m)| {
            let dir_ok = !direct_only || m.direct_download;
            let src_low = m.source.to_lowercase();
            let src_ok = match source_filter {
                1 => src_low.contains("bakkes"),
                2 => src_low.contains("lethamyr"),
                3 => src_low.contains("steam"),
                _ => true,
            };
            let q_ok = query_low.is_empty()
                || m.name.to_lowercase().contains(query_low)
                || src_low.contains(query_low)
                || m.short_desc.to_lowercase().contains(query_low);
            dir_ok && src_ok && q_ok
        })
        .map(|(i, _)| i)
        .collect();

    match sort_mode {
        1 => idxs.sort_by(|&a, &b| items[b].downloads.cmp(&items[a].downloads).then_with(|| a.cmp(&b))),
        2 => idxs.sort_by(|&a, &b| items[a].name.to_lowercase().cmp(&items[b].name.to_lowercase())),
        _ => {}
    }
    idxs
}

/// Stable-sorts `idxs` so items pinned in `favorite_pkgs` appear at the top of the list.
fn sort_cosmetic_indices_with_favorites(
    idxs: &mut [usize],
    list: &[CatalogItem],
    favorite_pkgs: &HashSet<String>,
) {
    if favorite_pkgs.is_empty() {
        return;
    }
    idxs.sort_by_key(|&i| {
        let is_fav = list
            .get(i)
            .map_or(false, |it| favorite_pkgs.contains(&it.asset_package.to_lowercase()));
        (!is_fav, i)
    });
}

fn active_studio_dir() -> PathBuf {
    let d = get_appdata_dir().join("Active Studio");
    let _ = fs::create_dir_all(&d);
    d
}

fn decode_b64_png_to_rgba(b64: &str) -> Option<RgbaImage> {
    let raw = base64::engine::general_purpose::STANDARD.decode(b64.trim()).ok()?;
    image::load_from_memory(&raw).ok().map(|im| im.to_rgba8())
}

#[cfg(test)]
fn encode_rgba_to_b64_png(img: &RgbaImage) -> Option<String> {
    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    image::DynamicImage::ImageRgba8(img.clone())
        .write_to(&mut cursor, ImageFormat::Png)
        .ok()?;
    Some(base64::engine::general_purpose::STANDARD.encode(buf))
}

fn save_studio_png_file(filename: &str, img: &RgbaImage, max_dim: u32) -> Option<String> {
    let (w, h) = img.dimensions();
    let scaled = if w > max_dim || h > max_dim {
        image::DynamicImage::ImageRgba8(img.clone())
            .resize(max_dim, max_dim, FilterType::Lanczos3)
            .to_rgba8()
    } else {
        img.clone()
    };
    let path = active_studio_dir().join(filename);
    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    image::DynamicImage::ImageRgba8(scaled)
        .write_to(&mut cursor, ImageFormat::Png)
        .ok()?;
    fs::write(&path, buf).ok()?;
    Some(filename.to_string())
}

fn load_studio_png_b64(filename: &str) -> Option<String> {
    let bytes = fs::read(active_studio_dir().join(filename)).ok()?;
    Some(base64::engine::general_purpose::STANDARD.encode(bytes))
}

fn load_studio_png_rgba(filename: &str) -> Option<RgbaImage> {
    image::open(active_studio_dir().join(filename))
        .ok()
        .map(|im| im.to_rgba8())
}

// ── Per-Preset Folder (`Downloads\RL Swapper\Presets\<Name>\`) & Custom Items Library ──

fn sanitize_folder_label(raw: &str) -> String {
    let cleaned: String = raw
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('_').to_string();
    if trimmed.is_empty() {
        "Preset".into()
    } else {
        trimmed
    }
}

fn preset_folder_for_name(name: &str) -> PathBuf {
    get_presets_dir().join(sanitize_folder_label(name))
}

fn write_b64_png_to_path(b64: &str, dst: &Path) {
    if let Ok(raw) = base64::engine::general_purpose::STANDARD.decode(b64.trim()) {
        let _ = fs::write(dst, raw);
    }
}

fn read_path_to_b64_png(p: &Path) -> Option<String> {
    if !p.is_file() {
        return None;
    }
    let bytes = fs::read(p).ok()?;
    Some(base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// Saves a preset into its own dedicated folder (`Downloads\RL Swapper\Presets\<Name>\`)
/// with real `.png` texture files (`decal_skin.png`, `pfp.png`, `banner.png`, `ball_diffuse.png`)
/// alongside `preset.json`.
fn save_preset_to_folder(pr: &LoadoutPreset) -> Option<PathBuf> {
    let dir = preset_folder_for_name(&pr.name);
    fs::create_dir_all(&dir).ok()?;

    if let Some(ref dec) = pr.custom_decal {
        if let Some(ref b64) = dec.skin_png_b64 {
            write_b64_png_to_path(b64, &dir.join("decal_skin.png"));
        }
        if let Some(ref b64) = dec.diffuse_base_png_b64 {
            write_b64_png_to_path(b64, &dir.join("decal_base.png"));
        }
        if let Some(ref b64) = dec.mask_png_b64 {
            write_b64_png_to_path(b64, &dir.join("decal_mask.png"));
        }
    }
    if let Some(ref pfp) = pr.custom_pfp {
        write_b64_png_to_path(&pfp.image_png_b64, &dir.join("pfp.png"));
    }
    if let Some(ref ban) = pr.custom_banner {
        write_b64_png_to_path(&ban.image_png_b64, &dir.join("banner.png"));
    }
    if let Some(ref ball) = pr.custom_ball {
        write_b64_png_to_path(&ball.diffuse_png_b64, &dir.join("ball_diffuse.png"));
        if let Some(ref b64) = ball.normal_png_b64 {
            write_b64_png_to_path(b64, &dir.join("ball_normal.png"));
        }
        if let Some(ref b64) = ball.mask_png_b64 {
            write_b64_png_to_path(b64, &dir.join("ball_mask.png"));
        }
    }
    if let Some(ref wid) = pr.workshop_map_id {
        if let Some(entry) = find_library_map_by_id_or_slug(wid) {
            if let Some(ref th) = entry.thumb_file_rel {
                let src_th = get_workshop_library_dir().join(&entry.id).join(th);
                if src_th.is_file() {
                    let _ = fs::copy(&src_th, dir.join("workshop_preview.png"));
                }
            }
        }
    }

    let json = serde_json::to_string_pretty(pr).ok()?;
    fs::write(dir.join("preset.json"), json).ok()?;
    Some(dir)
}

/// Loads a `LoadoutPreset` from `Downloads\RL Swapper\Presets\<Name>\`, automatically picking up
/// any edits made directly to the `.png` files inside that preset folder.
fn load_preset_from_folder(dir: &Path) -> Option<LoadoutPreset> {
    let json_p = dir.join("preset.json");
    let txt = fs::read_to_string(&json_p).ok()?;
    let mut pr: LoadoutPreset = serde_json::from_str(&txt).ok()?;

    if let Some(ref mut dec) = pr.custom_decal {
        if let Some(b64) = read_path_to_b64_png(&dir.join("decal_skin.png")) {
            dec.skin_png_b64 = Some(b64);
        }
        if let Some(b64) = read_path_to_b64_png(&dir.join("decal_base.png")) {
            dec.diffuse_base_png_b64 = Some(b64);
        }
        if let Some(b64) = read_path_to_b64_png(&dir.join("decal_mask.png")) {
            dec.mask_png_b64 = Some(b64);
        }
    }
    if let Some(ref mut pfp) = pr.custom_pfp {
        if let Some(b64) = read_path_to_b64_png(&dir.join("pfp.png")) {
            pfp.image_png_b64 = b64;
        }
    }
    if let Some(ref mut ban) = pr.custom_banner {
        if let Some(b64) = read_path_to_b64_png(&dir.join("banner.png")) {
            ban.image_png_b64 = b64;
        }
    }
    if let Some(ref mut ball) = pr.custom_ball {
        if let Some(b64) = read_path_to_b64_png(&dir.join("ball_diffuse.png")) {
            ball.diffuse_png_b64 = b64;
        }
        if let Some(b64) = read_path_to_b64_png(&dir.join("ball_normal.png")) {
            ball.normal_png_b64 = Some(b64);
        }
        if let Some(b64) = read_path_to_b64_png(&dir.join("ball_mask.png")) {
            ball.mask_png_b64 = Some(b64);
        }
    }
    Some(pr)
}

fn load_all_presets_from_hub() -> Vec<LoadoutPreset> {
    let mut map: Vec<LoadoutPreset> = Vec::new();
    let presets_root = get_presets_dir();
    if let Ok(entries) = fs::read_dir(&presets_root) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                if let Some(pr) = load_preset_from_folder(&p) {
                    if !map.iter().any(|x| x.name.eq_ignore_ascii_case(&pr.name)) {
                        map.push(pr);
                    }
                }
            }
        }
    }

    // Also merge any legacy presets from `presets_v4.json` and unpack them into per-preset folders
    if let Ok(raw) = fs::read_to_string(presets_file_path()) {
        if let Ok(legacy_list) = serde_json::from_str::<Vec<LoadoutPreset>>(&raw) {
            for pr in legacy_list {
                if !map.iter().any(|x| x.name.eq_ignore_ascii_case(&pr.name)) {
                    let _ = save_preset_to_folder(&pr);
                    map.push(pr);
                }
            }
        }
    }
    map
}

fn save_custom_image_to_library(category: &str, label: &str, img: &RgbaImage) -> Option<PathBuf> {
    let dir = get_custom_items_dir(category);
    let base = sanitize_folder_label(
        Path::new(label)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(label),
    );
    let dst = dir.join(format!("{}.png", base));
    img.save(&dst).ok()?;
    Some(dst)
}

fn list_saved_custom_images(category: &str) -> Vec<(String, PathBuf)> {
    let dir = get_custom_items_dir(category);
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_file() && p.extension().and_then(|s| s.to_str()).unwrap_or("").eq_ignore_ascii_case("png") {
                let nm = p.file_stem().unwrap_or_default().to_string_lossy().to_string();
                out.push((nm, p));
            }
        }
    }
    out.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    out
}

fn save_custom_decal_to_library(pack: &AlphaConsoleDecalPack, car_label: &str) {
    let root = get_custom_items_dir("Decals");
    let sub = root.join(sanitize_folder_label(&pack.name));
    if fs::create_dir_all(&sub).is_err() {
        return;
    }
    if let Some(ref im) = pack.skin_img {
        let _ = im.save(sub.join("skin.png"));
    }
    if let Some(ref im) = pack.diffuse_base_img {
        let _ = im.save(sub.join("diffuse_base.png"));
    }
    if let Some(ref im) = pack.mask_img {
        let _ = im.save(sub.join("mask.png"));
    }
    let meta = serde_json::json!({
        "name": pack.name,
        "car_label": car_label,
        "body_id": pack.body_id,
        "skin_id": pack.skin_id,
    });
    let _ = fs::write(sub.join("decal_meta.json"), serde_json::to_string_pretty(&meta).unwrap_or_default());
}

fn list_saved_custom_decals() -> Vec<(String, PathBuf)> {
    let root = get_custom_items_dir("Decals");
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(&root) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() && (p.join("skin.png").is_file() || p.join("diffuse_base.png").is_file()) {
                let nm = e.file_name().to_string_lossy().to_string();
                out.push((nm, p));
            }
        }
    }
    out.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    out
}

fn save_custom_ball_to_library(pack: &AlphaConsoleBallPack) {
    let root = get_custom_items_dir("Balls");
    let sub = root.join(sanitize_folder_label(&pack.name));
    if fs::create_dir_all(&sub).is_err() {
        return;
    }
    let _ = pack.diffuse_img.save(sub.join("diffuse.png"));
    if let Some(ref im) = pack.normal_img {
        let _ = im.save(sub.join("normal.png"));
    }
    if let Some(ref im) = pack.mask_img {
        let _ = im.save(sub.join("mask.png"));
    }
}

fn list_saved_custom_balls() -> Vec<(String, PathBuf)> {
    let root = get_custom_items_dir("Balls");
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(&root) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() && p.join("diffuse.png").is_file() {
                let nm = e.file_name().to_string_lossy().to_string();
                out.push((nm, p));
            }
        }
    }
    out.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    out
}

fn format_preset_summary(pr: &LoadoutPreset) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !pr.item_swaps.is_empty() {
        let preview: Vec<String> = pr
            .item_swaps
            .iter()
            .take(3)
            .map(|r| match &r.paint_name {
                Some(pn) => format!("{} [{}]", r.donor_desc, pn),
                None => r.donor_desc.clone(),
            })
            .collect();
        parts.push(format!("{} swaps ({})", pr.item_swaps.len(), preview.join(", ")));
    }
    if let Some(ref d) = pr.custom_decal {
        parts.push(format!("Decal: {}", d.target_desc));
    }
    if let Some(ref p) = pr.custom_pfp {
        parts.push(format!("PFP: {}", p.border_desc));
    }
    if let Some(ref b) = pr.custom_banner {
        parts.push(format!("Banner: {}", b.banner_desc));
    }
    if pr.custom_ball.is_some() {
        parts.push("Custom Ball".into());
    }
    if pr.workshop_map_id.is_some() {
        parts.push("Workshop".into());
    }
    if parts.is_empty() {
        format!("Empty preset · {}", pr.created_at)
    } else {
        format!("{} · {}", parts.join(" · "), pr.created_at)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DroppedFileKind {
    WorkshopMap,
    PresetJson,
    DecalOrBallPack,
    ImageAsset,
    Unsupported,
}

/// Classifies an OS-dropped file path by extension and container headers for automatic routing.
fn classify_dropped_file(path: &Path) -> DroppedFileKind {
    if path.is_dir() {
        return DroppedFileKind::DecalOrBallPack;
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "udk" | "upk" => DroppedFileKind::WorkshopMap,
        "png" | "jpg" | "jpeg" | "webp" => DroppedFileKind::ImageAsset,
        "json" => {
            if let Ok(txt) = fs::read_to_string(path) {
                if serde_json::from_str::<LoadoutPreset>(&txt).is_ok() {
                    return DroppedFileKind::PresetJson;
                }
            }
            DroppedFileKind::DecalOrBallPack
        }
        "zip" => {
            if let Ok(file) = fs::File::open(path) {
                if let Ok(mut archive) = zip::ZipArchive::new(file) {
                    for i in 0..archive.len() {
                        if let Ok(entry) = archive.by_index(i) {
                            let low = entry.name().to_ascii_lowercase();
                            if low.ends_with(".udk") || low.ends_with(".upk") {
                                return DroppedFileKind::WorkshopMap;
                            }
                        }
                    }
                }
            }
            DroppedFileKind::DecalOrBallPack
        }
        _ => DroppedFileKind::Unsupported,
    }
}

fn detect_cooked_dir_checked() -> Option<PathBuf> {
    for c in [
        r"C:\Program Files\Epic Games\rocketleague\TAGame\CookedPCConsole",
        r"C:\Program Files (x86)\Steam\steamapps\common\rocketleague\TAGame\CookedPCConsole",
        r"D:\Epic Games\rocketleague\TAGame\CookedPCConsole",
        r"D:\SteamLibrary\steamapps\common\rocketleague\TAGame\CookedPCConsole",
        r"E:\Epic Games\rocketleague\TAGame\CookedPCConsole",
        r"E:\SteamLibrary\steamapps\common\rocketleague\TAGame\CookedPCConsole",
    ] {
        let p = PathBuf::from(c);
        if p.is_dir() {
            return Some(p);
        }
    }
    None
}

fn detect_default_cooked_dir() -> String {
    detect_cooked_dir_checked()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| r"C:\Program Files\Epic Games\rocketleague\TAGame\CookedPCConsole".to_string())
}

fn presets_file_path() -> PathBuf {
    get_appdata_dir().join("presets_v4.json")
}

struct RLSwapperGui {
    cooked_dir: String,
    state: AppPersistentState,
    active_tab: ActiveTab,
    active_slot_idx: usize,

    // Memory-compact catalog (moved directly into slot buckets; no duplicate full Vec)
    items_by_slot: HashMap<String, Vec<CatalogItem>>,
    avatar_borders: Vec<(String, String)>,
    player_banners: Vec<(String, String)>,
    paints: Vec<PaintColorSpec>,
    selected_paint_idx: Option<usize>,
    custom_hex_color: [u8; 3],
    use_custom_hex: bool,

    // Cosmetic Slot State
    want_search: String,
    have_search: String,
    selected_want_idx: usize,
    selected_have_idx: usize,
    body_hitbox_filter: Option<CarHitbox>,
    cosmetic_swapped_only: bool,
    cache_slot_key: (String, Option<CarHitbox>, String),
    cached_want_idxs: Vec<usize>,
    cache_have_key: (String, Option<CarHitbox>, String, bool, u64),
    cached_have_idxs: Vec<usize>,

    // Bounded Async Icons & Background Channel Wakeup
    egui_ctx: Option<egui::Context>,
    icon_mgr: Arc<IconCacheManager>,
    icon_job_tx: Sender<IconJob>,
    tx: Sender<WorkerEvent>,
    rx: Receiver<WorkerEvent>,
    gpu_icons: HashMap<String, TextureHandle>,
    gpu_icon_order: VecDeque<String>,
    pending_icons: HashSet<String>,
    is_busy: bool,
    status_text: String,
    status_is_error: bool,

    // Studio: Decals (lazy init)
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
    decal_initialized: bool,

    // Studio: Avatar PFP
    pfp_border_idx: usize,
    pfp_search: String,
    cache_pfp_q: Option<String>,
    cached_pfp_idxs: Vec<usize>,
    pfp_custom_img: Option<RgbaImage>,
    pfp_composited_img: Option<RgbaImage>,
    pfp_custom_path: String,
    pfp_source_path: Option<PathBuf>,
    pfp_preview_tex: Option<TextureHandle>,
    pfp_preview_dirty: bool,

    // Studio: Banner (lazy init)
    banner_target_idx: usize,
    banner_search: String,
    cache_banner_q: Option<String>,
    cached_banner_idxs: Vec<usize>,
    banner_custom_img: Option<RgbaImage>,
    banner_custom_path: String,
    banner_source_path: Option<PathBuf>,
    banner_player_name: String,
    banner_bake_name: bool,
    banner_preview_tex: Option<TextureHandle>,
    banner_preview_dirty: bool,
    banner_initialized: bool,

    // Studio: Match Ball (lazy init)
    ac_ball_packs: Vec<AlphaConsoleBallPack>,
    ac_ball_idx: usize,
    ball_custom_img: Option<RgbaImage>,
    ball_normal_img: Option<RgbaImage>,
    ball_mask_img: Option<RgbaImage>,
    ball_custom_path: String,
    ball_orb_tex: Option<TextureHandle>,
    ball_flat_tex: Option<TextureHandle>,
    ball_norm_tex: Option<TextureHandle>,
    ball_mask_tex: Option<TextureHandle>,
    ball_orb_dirty: bool,
    ball_initialized: bool,

    // Interactive Image Cutter Modal (PFP 1:1 & Banner 14:3)
    crop_modal: Option<ImageCropModal>,

    // Workshop
    workshop_library: Vec<WorkshopMapEntry>,
    workshop_selected_idx: usize,
    workshop_selected_online_idx: Option<usize>,
    workshop_local_thumb: (String, Option<TextureHandle>),
    workshop_missing_deps: Vec<String>,
    workshop_online_items: Vec<OnlineWorkshopItem>,
    workshop_online_loading: bool,
    workshop_online_search: String,
    workshop_direct_only: bool,
    workshop_source_filter: usize,
    workshop_sort_mode: usize,
    cache_ws_key: (String, bool, usize, usize, usize),
    cached_ws_idxs: Vec<usize>,
    workshop_online_thumbs: HashMap<String, TextureHandle>,
    ws_thumb_order: VecDeque<String>,
    workshop_online_pending_thumbs: HashSet<String>,
    workshop_online_failed_thumbs: HashSet<String>,
    ws_thumb_tx: Sender<String>,
    workshop_dl_progress: Option<WorkshopDownloadProgress>,

    // System
    presets: Vec<LoadoutPreset>,
    new_preset_name: String,
    preset_clean_apply: bool,
    health_report: Option<CookedHealthReport>,
    swaps_revision: u64,
    show_reset_modal: bool,
    rl_running: bool,
    last_proc_check: Instant,
    app_logo_tex: Option<TextureHandle>,
    #[allow(dead_code)]
    snap_state: Option<(PathBuf, String, usize, u8)>,
}

impl RLSwapperGui {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_minimal_black_style(&cc.egui_ctx);

        let state_path = get_appdata_dir().join("swapper_state_v4.json");
        let mut state: AppPersistentState = fs::read_to_string(&state_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        if state.cooked_dir.is_empty() || !Path::new(&state.cooked_dir).is_dir() {
            state.cooked_dir = detect_default_cooked_dir();
        }
        let cooked_path = PathBuf::from(&state.cooked_dir);

        let mut all_catalog = load_catalog_items(Some(&cooked_path));
        let avatar_borders = list_avatar_border_packages_with_catalog(&cooked_path, &all_catalog);
        let player_banners = list_player_banner_packages_with_catalog(&cooked_path, &all_catalog);

        // Move items into buckets rather than cloning ~8,000 structs
        let mut items_by_slot: HashMap<String, Vec<CatalogItem>> = HashMap::new();
        for it in all_catalog.drain(..) {
            if !it.slot.is_empty() {
                items_by_slot.entry(it.slot.clone()).or_default().push(it);
            }
        }
        for list in items_by_slot.values_mut() {
            list.sort_by(|a, b| a.product.to_lowercase().cmp(&b.product.to_lowercase()));
        }

        let icon_mgr = Arc::new(IconCacheManager::new());
        icon_mgr.index_cooked_dir(&cooked_path);
        let (tx, rx) = mpsc::channel();

        // Bounded persistent icon worker pool (2 threads, wakes UI via request_repaint)
        let (icon_job_tx, icon_job_rx) = mpsc::channel::<IconJob>();
        let icon_rx_shared = Arc::new(Mutex::new(icon_job_rx));
        for _ in 0..2 {
            let mgr = Arc::clone(&icon_mgr);
            let rx_w = Arc::clone(&icon_rx_shared);
            let tx_w = tx.clone();
            let ctx_w = cc.egui_ctx.clone();
            thread::spawn(move || {
                loop {
                    let job = {
                        let Ok(guard) = rx_w.lock() else { break };
                        guard.recv()
                    };
                    let Ok(j) = job else { break };
                    let (base_im, _) = mgr.get_icon_sync(&j.cooked_dir, &j.item, j.sz >= 48);
                    let comp = composite_icon_with_paint(&base_im, j.paint.as_ref(), j.sz);
                    let _ = tx_w.send(WorkerEvent::IconReady {
                        cache_key: j.cache_key,
                        rgba: comp,
                    });
                    ctx_w.request_repaint();
                }
            });
        }

        // Bounded persistent Workshop thumbnail worker pool (2 threads, wakes UI via request_repaint)
        let (ws_thumb_tx, ws_thumb_rx) = mpsc::channel::<String>();
        let ws_rx_shared = Arc::new(Mutex::new(ws_thumb_rx));
        for _ in 0..2 {
            let rx_w = Arc::clone(&ws_rx_shared);
            let tx_w = tx.clone();
            let ctx_w = cc.egui_ctx.clone();
            thread::spawn(move || {
                loop {
                    let next_url = {
                        let Ok(guard) = rx_w.lock() else { break };
                        guard.recv()
                    };
                    let Ok(url) = next_url else { break };
                    let rgba = fetch_or_cache_thumb(&url);
                    let _ = tx_w.send(WorkerEvent::WorkshopThumbDone { url, rgba });
                    ctx_w.request_repaint();
                }
            });
        }

        // Defer CookedPCConsole disk scan to background thread so window appears immediately
        let health_cdir = cooked_path.clone();
        let health_tx = tx.clone();
        let health_ctx = cc.egui_ctx.clone();
        thread::spawn(move || {
            let rep = inspect_cooked_health(&health_cdir);
            let _ = health_tx.send(WorkerEvent::HealthReady(rep));
            health_ctx.request_repaint();
        });

        let presets = load_all_presets_from_hub();
        let workshop_library = list_library_maps();
        let initial_ws_deps = workshop_library
            .first()
            .map(|e| scan_upk_external_dependencies(&get_workshop_library_dir().join(&e.id).join(&e.map_file_rel), &cooked_path))
            .unwrap_or_default();
        let app_logo_tex = make_rounded_icon_rgba(include_bytes!("../../assets/logo.png"), 44)
            .map(|im| cc.egui_ctx.load_texture("rl_swapper_brand_logo", rgba_to_egui_image(&im), TextureOptions::LINEAR));

        let mut app = Self {
            cooked_dir: state.cooked_dir.clone(),
            state,
            active_tab: ActiveTab::Cosmetics,
            active_slot_idx: 0,
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
            cosmetic_swapped_only: false,
            cache_slot_key: (String::new(), None, "\x01".into()),
            cached_want_idxs: Vec::new(),
            cache_have_key: (String::new(), None, "\x01".into(), false, 0),
            cached_have_idxs: Vec::new(),
            egui_ctx: Some(cc.egui_ctx.clone()),
            icon_mgr,
            icon_job_tx,
            tx,
            rx,
            gpu_icons: HashMap::new(),
            gpu_icon_order: VecDeque::new(),
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
            decal_custom_img: None,
            decal_mask_img: None,
            decal_custom_path: "Octane UV".into(),
            decal_primary_rgb: [30, 64, 175],
            decal_accent_rgb: [56, 189, 248],
            decal_preview_tex: None,
            decal_preview_dirty: true,
            decal_initialized: false,
            pfp_border_idx: 0,
            pfp_search: String::new(),
            cache_pfp_q: None,
            cached_pfp_idxs: Vec::new(),
            pfp_custom_img: None,
            pfp_composited_img: None,
            pfp_custom_path: String::new(),
            pfp_source_path: None,
            pfp_preview_tex: None,
            pfp_preview_dirty: true,
            banner_target_idx: 0,
            banner_search: String::new(),
            cache_banner_q: None,
            cached_banner_idxs: Vec::new(),
            banner_custom_img: None,
            banner_custom_path: "Banner Template".into(),
            banner_source_path: None,
            banner_player_name: String::new(),
            banner_bake_name: false,
            banner_preview_tex: None,
            banner_preview_dirty: true,
            banner_initialized: false,
            ac_ball_packs: Vec::new(),
            ac_ball_idx: 0,
            ball_custom_img: None,
            ball_normal_img: None,
            ball_mask_img: None,
            ball_custom_path: "Vanilla Match Ball".into(),
            ball_orb_tex: None,
            ball_flat_tex: None,
            ball_norm_tex: None,
            ball_mask_tex: None,
            ball_orb_dirty: true,
            ball_initialized: false,
            crop_modal: None,
            workshop_library,
            workshop_selected_idx: 0,
            workshop_selected_online_idx: None,
            workshop_local_thumb: (String::new(), None),
            workshop_missing_deps: initial_ws_deps,
            workshop_online_items: Vec::new(),
            workshop_online_loading: false,
            workshop_online_search: String::new(),
            workshop_direct_only: false,
            workshop_source_filter: 0,
            workshop_sort_mode: 0,
            cache_ws_key: (String::new(), false, 0, 0, usize::MAX),
            cached_ws_idxs: Vec::new(),
            workshop_online_thumbs: HashMap::new(),
            ws_thumb_order: VecDeque::new(),
            workshop_online_pending_thumbs: HashSet::new(),
            workshop_online_failed_thumbs: HashSet::new(),
            ws_thumb_tx,
            workshop_dl_progress: None,
            presets,
            new_preset_name: "Clean Preset".into(),
            preset_clean_apply: true,
            health_report: None,
            swaps_revision: 1,
            show_reset_modal: false,
            rl_running: is_rocket_league_running(),
            last_proc_check: Instant::now(),
            app_logo_tex,
            snap_state: std::env::var_os("RL_SWAPPER_SCREENSHOT_DIR").map(|d| {
                let pfx = std::env::var("RL_SWAPPER_SCREENSHOT_PREFIX").unwrap_or_else(|_| "ui".into());
                (PathBuf::from(d), pfx, 0usize, 0u8)
            }),
        };
        app.sync_rl_exe_patch_guard();
        app.sync_studio_from_active(false);
        app
    }

    fn current_slot(&self) -> &'static str {
        ORDERED_SLOTS[self.active_slot_idx.min(ORDERED_SLOTS.len() - 1)]
    }

    /// Recomputes `cached_want_idxs` & `cached_have_idxs` for the active slot/filters and clamps selections.
    fn sync_cosmetic_caches(&mut self) {
        let slot = self.current_slot();
        let is_body_slot = slot == "Body";
        let hb_filter = if is_body_slot { self.body_hitbox_filter } else { None };
        let cooked_p = Path::new(&self.cooked_dir);

        let want_key = (slot.to_string(), hb_filter, self.want_search.to_lowercase());
        if self.cache_slot_key != want_key {
            let empty_map = HashMap::new();
            let mut idxs = self
                .items_by_slot
                .get(slot)
                .map(|list| {
                    let mut v = filter_cosmetic_indices(list, &want_key.2, hb_filter, false, &empty_map, cooked_p);
                    sort_cosmetic_indices_with_favorites(&mut v, list, &self.state.favorite_pkgs);
                    v
                })
                .unwrap_or_default();
            idxs.shrink_to_fit();
            self.cached_want_idxs = idxs;
            self.cache_slot_key = want_key;
        }

        let have_key = (
            slot.to_string(),
            hb_filter,
            self.have_search.to_lowercase(),
            self.cosmetic_swapped_only,
            self.swaps_revision,
        );
        if self.cache_have_key != have_key {
            let mut idxs = self
                .items_by_slot
                .get(slot)
                .map(|list| {
                    let mut v = filter_cosmetic_indices(
                        list,
                        &have_key.2,
                        hb_filter,
                        have_key.3,
                        &self.state.active_swaps,
                        cooked_p,
                    );
                    sort_cosmetic_indices_with_favorites(&mut v, list, &self.state.favorite_pkgs);
                    v
                })
                .unwrap_or_default();
            idxs.shrink_to_fit();
            self.cached_have_idxs = idxs;
            self.cache_have_key = have_key;
        }

        if self.selected_want_idx >= self.cached_want_idxs.len() {
            self.selected_want_idx = 0;
        }
        if self.selected_have_idx >= self.cached_have_idxs.len() {
            self.selected_have_idx = 0;
        }
    }

    /// Switches the active Cosmetics slot, clears search queries, and immediately resyncs index caches.
    fn switch_cosmetic_slot(&mut self, next_slot: usize) {
        self.active_slot_idx = next_slot.min(ORDERED_SLOTS.len() - 1);
        self.selected_want_idx = 0;
        self.selected_have_idx = 0;
        self.want_search.clear();
        self.have_search.clear();
        self.sync_cosmetic_caches();
    }

    /// Resets selected Paint back to Default and clears Cosmetic search bars after each swap.
    fn reset_cosmetic_after_swap(&mut self, swapped: &[(String, String)]) {
        self.selected_paint_idx = None;
        self.use_custom_hex = false;
        self.want_search.clear();
        self.have_search.clear();
        self.sync_cosmetic_caches();
        if let Some((last_donor_pkg, last_target_pkg)) = swapped.last() {
            let slot = self.current_slot();
            if let Some(list) = self.items_by_slot.get(slot) {
                if let Some(w_pos) = self.cached_want_idxs.iter().position(|&raw_i| {
                    list.get(raw_i)
                        .map_or(false, |it| it.asset_package.eq_ignore_ascii_case(last_donor_pkg))
                }) {
                    self.selected_want_idx = w_pos;
                }
                if let Some(h_pos) = self.cached_have_idxs.iter().position(|&raw_i| {
                    list.get(raw_i)
                        .map_or(false, |it| it.asset_package.eq_ignore_ascii_case(last_target_pkg))
                }) {
                    self.selected_have_idx = h_pos;
                }
            }
        }
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
        let mut active_dirs = HashSet::new();
        for pr in &self.presets {
            if let Some(dir) = save_preset_to_folder(pr) {
                if let Some(nm) = dir.file_name().and_then(|s| s.to_str()) {
                    active_dirs.insert(nm.to_lowercase());
                }
            }
        }
        if let Ok(entries) = fs::read_dir(get_presets_dir()) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() && p.join("preset.json").is_file() {
                    let nm = e.file_name().to_string_lossy().to_lowercase();
                    if !active_dirs.contains(&nm) {
                        let _ = fs::remove_dir_all(p);
                    }
                }
            }
        }
    }

    fn toggle_favorite_pkg(&mut self, pkg_name: &str, item_label: &str) {
        let key = pkg_name.to_lowercase();
        let added = if self.state.favorite_pkgs.contains(&key) {
            self.state.favorite_pkgs.remove(&key);
            false
        } else {
            self.state.favorite_pkgs.insert(key);
            true
        };
        self.cache_slot_key.2.push('!');
        self.cache_have_key.2.push('!');
        self.sync_cosmetic_caches();
        self.save_state();
        self.status_is_error = false;
        self.status_text = if added {
            format!("Pinned '{}' to top of list", item_label)
        } else {
            format!("Unpinned '{}'", item_label)
        };
    }

    fn refresh_health_report(&mut self) {
        let cdir = PathBuf::from(&self.cooked_dir);
        let tx = self.tx.clone();
        thread::spawn(move || {
            let _ = tx.send(WorkerEvent::HealthReady(inspect_cooked_health(&cdir)));
        });
    }

    fn invalidate_caches(&mut self) {
        self.cache_slot_key.2.push('!');
        self.cache_have_key.2.push('!');
        self.cache_decal_key = None;
        self.cache_pfp_q = None;
        self.cache_banner_q = None;
        self.cache_ws_key.4 = usize::MAX;
        self.decal_initialized = false;
        self.decal_preview_dirty = true;
        self.pfp_preview_dirty = true;
        self.banner_initialized = false;
        self.banner_preview_dirty = true;
        self.ball_initialized = false;
        self.ball_orb_dirty = true;
        self.gpu_icons.clear();
        self.gpu_icon_order.clear();
        self.pending_icons.clear();
        if let Some(sel) = self.workshop_library.get(self.workshop_selected_idx) {
            self.workshop_missing_deps = scan_upk_external_dependencies(
                &get_workshop_library_dir().join(&sel.id).join(&sel.map_file_rel),
                Path::new(&self.cooked_dir),
            );
        }
    }

    fn apply_cooked_dir(&mut self, folder: PathBuf) {
        self.cooked_dir = folder.to_string_lossy().to_string();
        self.state.cooked_dir = self.cooked_dir.clone();
        self.icon_mgr.index_cooked_dir(&folder);
        let studio_cat: Vec<CatalogItem> = self
            .items_by_slot
            .iter()
            .filter(|(s, _)| s.as_str() == "Avatar Border" || s.as_str() == "Player Banner")
            .flat_map(|(_, v)| v.iter().cloned())
            .collect();
        self.avatar_borders = list_avatar_border_packages_with_catalog(&folder, &studio_cat);
        self.player_banners = list_player_banner_packages_with_catalog(&folder, &studio_cat);
        self.invalidate_caches();
        self.refresh_health_report();
        self.save_state();
        self.status_is_error = !folder.is_dir();
        self.status_text = format!("Game folder: {}", self.cooked_dir);
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
            // Do NOT overwrite freshly patched game files with old-patch .bak backups;
            // remove stale .bak & pristine vault copies for tracked swaps cleanly instead.
            for rec in self.state.active_swaps.values() {
                let p = cooked.join(&rec.target_pkg);
                let _ = fs::remove_file(bak_path_for(&p));
                let _ = fs::remove_file(vault_backup_path(&p));
            }
            self.state.active_swaps.clear();
            self.state.swap_history.clear();
            self.state.active_studio = ActiveStudioState::default();
            self.status_text = "Rocket League updated — cleared stale swap backups safely".into();
        }
        self.state.rl_exe_size = cur_size;
        self.state.rl_exe_mtime = cur_mtime;
        self.save_state();
    }

    fn spawn_op_with_repaint<F>(&self, work: F)
    where
        F: FnOnce(Sender<WorkerEvent>) + Send + 'static,
    {
        let tx = self.tx.clone();
        let ctx = self.egui_ctx.clone();
        thread::spawn(move || {
            work(tx);
            if let Some(c) = ctx {
                c.request_repaint();
            }
        });
    }

    fn sync_studio_from_active(&mut self, clear_unsaved: bool) {
        if let Some(d) = self.state.active_studio.decal.clone() {
            let loaded_skin = d
                .skin_file
                .as_deref()
                .and_then(load_studio_png_rgba)
                .or_else(|| d.diffuse_base_file.as_deref().and_then(load_studio_png_rgba));
            if let Some(im) = loaded_skin {
                let mask_im = d.mask_file.as_deref().and_then(load_studio_png_rgba);
                let base_im = d.diffuse_base_file.as_deref().and_then(load_studio_png_rgba);
                if let Some(lbl) = detect_car_uv_label(&format!("{} {}", d.target_desc, d.target_pkg)) {
                    self.decal_car_label = lbl;
                }
                let seed_pack = AlphaConsoleDecalPack {
                    name: if d.name.trim().is_empty() { d.target_desc.clone() } else { d.name.clone() },
                    body_id: None,
                    skin_id: None,
                    diffuse_base_img: base_im,
                    skin_img: Some(im.clone()),
                    mask_img: mask_im.clone(),
                    chassis_diffuse_img: None,
                    source_label: "Active".into(),
                };
                save_custom_decal_to_library(&seed_pack, self.decal_car_label);
                self.decal_custom_img = Some(im);
                self.decal_mask_img = mask_im;
                self.decal_custom_path = format!("{} (Active)", d.name);
                self.decal_target_search.clear();
                self.cache_decal_key = None;
                self.ensure_decal_target_cache();
                if let Some(decal_list) = self.items_by_slot.get("Decal") {
                    if let Some(pos) = self.cached_decal_idxs.iter().position(|&ri| {
                        decal_list
                            .get(ri)
                            .map_or(false, |it| it.asset_package.eq_ignore_ascii_case(&d.target_pkg))
                    }) {
                        self.decal_target_idx = pos;
                    }
                }
                self.decal_preview_dirty = true;
                self.decal_initialized = true;
            }
        } else if clear_unsaved {
            self.decal_initialized = false;
            self.decal_mask_img = None;
            self.decal_custom_path = format!("{} UV", self.decal_car_label);
            self.decal_preview_dirty = true;
        }

        if let Some(ref p) = self.state.active_studio.pfp {
            if let Some(im) = load_studio_png_rgba(&p.image_file) {
                let _ = save_custom_image_to_library("PFP", &p.border_desc, &im);
                self.pfp_custom_img = Some(im);
                self.pfp_custom_path = format!("Active: {}", p.border_desc);
                self.pfp_search.clear();
                self.cache_pfp_q = None;
                if let Some(pos) = self
                    .avatar_borders
                    .iter()
                    .position(|(_, pkg)| pkg.eq_ignore_ascii_case(&p.border_pkg))
                {
                    self.pfp_border_idx = pos;
                }
                self.pfp_preview_dirty = true;
            }
        } else if clear_unsaved {
            self.pfp_custom_img = None;
            self.pfp_composited_img = None;
            self.pfp_custom_path.clear();
            self.pfp_source_path = None;
            self.pfp_preview_dirty = true;
        }

        if let Some(ref b) = self.state.active_studio.banner {
            if let Some(im) = load_studio_png_rgba(&b.image_file) {
                let _ = save_custom_image_to_library("Banners", &b.banner_desc, &im);
                self.banner_custom_img = Some(im);
                self.banner_player_name = b.player_name.clone();
                self.banner_bake_name = b.bake_name;
                self.banner_custom_path = format!("Active: {}", b.banner_desc);
                self.banner_search.clear();
                self.cache_banner_q = None;
                if let Some(pos) = self
                    .player_banners
                    .iter()
                    .position(|(_, pkg)| pkg.eq_ignore_ascii_case(&b.banner_pkg))
                {
                    self.banner_target_idx = pos;
                }
                self.banner_preview_dirty = true;
                self.banner_initialized = true;
            }
        } else if clear_unsaved {
            self.banner_initialized = false;
            self.banner_source_path = None;
            self.banner_preview_dirty = true;
        }

        if let Some(ref bl) = self.state.active_studio.ball {
            if let Some(diff) = load_studio_png_rgba(&bl.diffuse_file) {
                let norm = bl.normal_file.as_deref().and_then(load_studio_png_rgba);
                let mask = bl.mask_file.as_deref().and_then(load_studio_png_rgba);
                save_custom_ball_to_library(&AlphaConsoleBallPack {
                    name: bl.name.clone(),
                    group: String::new(),
                    diffuse_img: diff.clone(),
                    normal_img: norm.clone(),
                    mask_img: mask.clone(),
                    source_label: "Active".into(),
                });
                self.ball_custom_img = Some(diff);
                self.ball_normal_img = norm;
                self.ball_mask_img = mask;
                self.ball_custom_path = format!("{} (Active)", bl.name);
                self.ball_orb_dirty = true;
                self.ball_initialized = true;
            }
        } else if clear_unsaved {
            self.ball_initialized = false;
            self.ball_normal_img = None;
            self.ball_mask_img = None;
            self.ball_mask_tex = None;
            self.ball_custom_path = "Vanilla Match Ball".into();
            self.ball_orb_dirty = true;
        }
    }

    fn open_image_cropper(
        &mut self,
        ctx: &egui::Context,
        target: CropTarget,
        source_path: Option<PathBuf>,
        raw_img: RgbaImage,
        filename: String,
    ) {
        let max_src = 2048u32;
        let source_img = if raw_img.width() > max_src || raw_img.height() > max_src {
            let (sw, sh) = (raw_img.width(), raw_img.height());
            let (nw, nh) = if sw >= sh {
                (max_src, (sh as u64 * max_src as u64 / sw as u64).max(1) as u32)
            } else {
                ((sw as u64 * max_src as u64 / sh as u64).max(1) as u32, max_src)
            };
            imageops::resize(&raw_img, nw, nh, FilterType::Lanczos3)
        } else {
            raw_img
        };

        let max_prev = 1024u32;
        let prev_img = if source_img.width() > max_prev || source_img.height() > max_prev {
            let (sw, sh) = (source_img.width(), source_img.height());
            let (nw, nh) = if sw >= sh {
                (max_prev, (sh as u64 * max_prev as u64 / sw as u64).max(1) as u32)
            } else {
                ((sw as u64 * max_prev as u64 / sh as u64).max(1) as u32, max_prev)
            };
            imageops::resize(&source_img, nw, nh, FilterType::Triangle)
        } else {
            source_img.clone()
        };
        let preview_tex = ctx.load_texture("crop_modal_source", rgba_to_egui_image(&prev_img), TextureOptions::LINEAR);
        self.crop_modal = Some(ImageCropModal {
            target,
            filename,
            source_path,
            source_img,
            preview_tex,
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
        });
    }

    fn apply_crop_result(
        &mut self,
        target: CropTarget,
        cropped: RgbaImage,
        filename: String,
        source_path: Option<PathBuf>,
    ) {
        match target {
            CropTarget::Pfp => {
                let _ = save_custom_image_to_library("PFP", &filename, &cropped);
                self.pfp_custom_img = Some(cropped);
                self.pfp_custom_path = filename.clone();
                if source_path.is_some() {
                    self.pfp_source_path = source_path;
                }
                self.pfp_preview_dirty = true;
                self.banner_preview_dirty = true;
                self.status_is_error = false;
                self.status_text = format!("Cut 1:1 avatar '{}' (saved to Custom Items/PFP)", filename);
            }
            CropTarget::Banner => {
                let _ = save_custom_image_to_library("Banners", &filename, &cropped);
                self.banner_custom_img = Some(cropped);
                self.banner_custom_path = filename.clone();
                if source_path.is_some() {
                    self.banner_source_path = source_path;
                }
                self.banner_preview_dirty = true;
                self.status_is_error = false;
                self.status_text = format!("Cut 14:3 banner '{}' (saved to Custom Items/Banners)", filename);
            }
        }
    }

    fn is_pkg_swapped(&self, pkg_name: &str) -> bool {
        is_pkg_swapped_in_state(pkg_name, &self.state.active_swaps, Path::new(&self.cooked_dir))
    }

    fn bump_swaps_revision(&mut self) {
        self.swaps_revision = self.swaps_revision.wrapping_add(1);
    }

    fn mark_package_restored_side_effects(&mut self, pkg_name: &str) {
        let low = pkg_name.to_lowercase();
        if self.state.active_studio.decal.as_ref().map_or(false, |d| d.target_pkg.eq_ignore_ascii_case(pkg_name)) {
            self.state.active_studio.decal = None;
            self.decal_initialized = false;
            self.decal_mask_img = None;
            self.decal_custom_path = format!("{} UV", self.decal_car_label);
            self.decal_preview_dirty = true;
        }
        if self.state.active_studio.pfp.as_ref().map_or(false, |p| p.border_pkg.eq_ignore_ascii_case(pkg_name)) {
            self.state.active_studio.pfp = None;
            self.pfp_custom_img = None;
            self.pfp_composited_img = None;
            self.pfp_custom_path.clear();
            self.pfp_source_path = None;
            self.pfp_preview_dirty = true;
        }
        if self.state.active_studio.banner.as_ref().map_or(false, |b| b.banner_pkg.eq_ignore_ascii_case(pkg_name)) {
            self.state.active_studio.banner = None;
            self.banner_initialized = false;
            self.banner_source_path = None;
            self.banner_preview_dirty = true;
        }
        if low.starts_with("avatarborder_") {
            self.pfp_preview_dirty = true;
        }
        if low == "gameinfo_soccar_sf.upk" {
            self.state.active_studio.ball = None;
            if self.ball_initialized {
                let cooked = Path::new(&self.cooked_dir);
                if self.ac_ball_packs.is_empty() {
                    self.ball_custom_img = Some(extract_ball_texture_preview(cooked).unwrap_or_else(|| generate_ball_template(None)));
                    self.ball_normal_img = None;
                    self.ball_mask_img = None;
                    self.ball_mask_tex = None;
                    self.ball_custom_path = "Vanilla Match Ball".into();
                    self.ball_orb_dirty = true;
                }
            }
        }
        if low == WORKSHOP_SAFE_SLOT.to_lowercase() {
            self.state.active_studio.workshop_map_id = None;
            self.workshop_missing_deps.clear();
        }
    }

    /// Restores a single target package to vanilla, updates persisted state, clears RL cache, and refreshes views.
    fn restore_target_pkg(&mut self, pkg_name: &str, display_label: &str) {
        restore_single_package(&Path::new(&self.cooked_dir).join(pkg_name));
        clear_rl_texture_cache();
        let low = pkg_name.to_lowercase();
        self.state.active_swaps.remove(&low);
        self.state.swap_history.retain(|x| x != &low);
        self.mark_package_restored_side_effects(pkg_name);
        append_swap_history_log(&format!("RESTORE {} ({})", display_label, pkg_name));
        self.bump_swaps_revision();
        self.save_state();
        self.refresh_health_report();
        self.status_is_error = false;
        self.status_text = format!("Restored {}", display_label);
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

    fn queue_item_icon(
        icon_job_tx: &Sender<IconJob>,
        cooked_dir: &str,
        gpu_icons: &HashMap<String, TextureHandle>,
        pending_icons: &mut HashSet<String>,
        item: &CatalogItem,
        paint: Option<&PaintColorSpec>,
        sz: u32,
    ) -> String {
        let base_key = format!("{}_{}", IconCacheManager::cache_key(item), sz);
        let full_key = match paint {
            Some(p) => format!("{}_{}", base_key, p.hex_srgb),
            None => base_key,
        };
        if gpu_icons.contains_key(&full_key)
            || pending_icons.contains(&full_key)
            || pending_icons.len() >= MAX_INFLIGHT_ICONS
        {
            return full_key;
        }
        pending_icons.insert(full_key.clone());
        let _ = icon_job_tx.send(IconJob {
            cache_key: full_key.clone(),
            cooked_dir: PathBuf::from(cooked_dir),
            item: item.clone(),
            paint: paint.cloned(),
            sz,
        });
        full_key
    }

    fn record_swap(&mut self, rec: ActiveSwapRecord) {
        let k = rec.target_pkg.to_lowercase();
        let paint_tag = rec
            .paint_name
            .as_deref()
            .map(|p| format!(" [{}]", p))
            .unwrap_or_default();
        append_swap_history_log(&format!(
            "{}: {}{} -> {} ({})",
            rec.kind, rec.donor_desc, paint_tag, rec.target_desc, rec.target_pkg
        ));
        self.state.active_swaps.insert(k.clone(), rec);
        self.state.swap_history.retain(|x| x != &k);
        self.state.swap_history.push(k);
        self.bump_swaps_revision();
        self.save_state();
    }

    fn peek_next_undo_rec(&self) -> Option<&ActiveSwapRecord> {
        self.state
            .swap_history
            .iter()
            .rev()
            .find_map(|k| self.state.active_swaps.get(k))
    }

    fn undo_last_swap(&mut self) {
        while let Some(last_key) = self.state.swap_history.pop() {
            if let Some(rec) = self.state.active_swaps.remove(&last_key) {
                restore_single_package(&Path::new(&self.cooked_dir).join(&rec.target_pkg));
                clear_rl_texture_cache();
                self.mark_package_restored_side_effects(&rec.target_pkg);
                append_swap_history_log(&format!("UNDO {} ({})", rec.target_desc, rec.target_pkg));
                self.bump_swaps_revision();
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
        let fresh_rep = inspect_cooked_health(&cdir);
        for pkg in &fresh_rep.bak_filenames {
            restore_single_package(&cdir.join(pkg));
        }
        clear_rl_texture_cache();
        self.state.active_swaps.clear();
        self.state.swap_history.clear();
        self.state.active_studio = ActiveStudioState::default();
        self.state.active_preset_name = None;
        self.sync_studio_from_active(true);
        self.workshop_missing_deps.clear();
        append_swap_history_log("RESET ALL VANILLA");
        self.bump_swaps_revision();
        self.save_state();
        self.refresh_health_report();
        self.status_is_error = false;
        self.status_text = "Restored all files to vanilla".into();
    }

    fn trigger_workshop_load(&mut self, sel: WorkshopMapEntry) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        self.status_is_error = false;
        self.status_text = format!("Loading '{}' into Underpass...", sel.name);
        let cdir = PathBuf::from(&self.cooked_dir);
        self.spawn_op_with_repaint(move |tx| {
            let ws_id = sel.id.clone();
            match load_workshop_map_into_underpass(&cdir, &sel) {
                Ok((msg, missing_deps)) => {
                    let rec = ActiveSwapRecord {
                        target_pkg: WORKSHOP_SAFE_SLOT.into(),
                        donor_desc: sel.name,
                        target_desc: "Underpass".into(),
                        kind: "WORKSHOP".into(),
                        slot: String::new(),
                        donor_pkg: String::new(),
                        donor_asset_path: String::new(),
                        target_asset_path: String::new(),
                        paint_hex: None,
                        paint_name: None,
                    };
                    let _ = tx.send(WorkerEvent::WorkshopLoadDone {
                        success: true,
                        message: msg,
                        rec: Some(rec),
                        missing_deps,
                        workshop_map_id: Some(ws_id),
                    });
                }
                Err(e) => {
                    let _ = tx.send(WorkerEvent::WorkshopLoadDone {
                        success: false,
                        message: e,
                        rec: None,
                        missing_deps: Vec::new(),
                        workshop_map_id: None,
                    });
                }
            }
        });
    }

    fn trigger_workshop_textures_download(&mut self, ctx: &egui::Context, force_redownload: bool) {
        if self.is_busy {
            return;
        }
        self.is_busy = true;
        self.status_is_error = false;
        self.workshop_dl_progress = Some(WorkshopDownloadProgress {
            name: "Workshop Textures".into(),
            downloaded: 0,
            total: None,
            phase: "Connecting...".into(),
        });
        self.status_text = "Downloading & installing Workshop Textures...".into();
        let tx_dl = self.tx.clone();
        let ctx_cl = ctx.clone();
        let cdir_dl = PathBuf::from(&self.cooked_dir);
        thread::spawn(move || {
            let mut last_sent = 0u64;
            let mut last_phase = String::new();
            let res = download_and_install_workshop_textures_with_progress(&cdir_dl, force_redownload, |dl, tot, phase| {
                if dl == 0 || phase != last_phase || dl.saturating_sub(last_sent) >= 65536 {
                    last_sent = dl;
                    last_phase = phase.to_string();
                    let _ = tx_dl.send(WorkerEvent::WorkshopDownloadProgress(WorkshopDownloadProgress {
                        name: "Workshop Textures".into(),
                        downloaded: dl,
                        total: tot,
                        phase: phase.to_string(),
                    }));
                    ctx_cl.request_repaint();
                }
            });
            let _ = tx_dl.send(WorkerEvent::WorkshopTexturesDone(res));
            ctx_cl.request_repaint();
        });
    }

    fn build_preset_from_active(&self, name: &str) -> LoadoutPreset {
        let item_swaps: Vec<ActiveSwapRecord> = self
            .state
            .active_swaps
            .values()
            .filter(|r| !r.donor_pkg.is_empty() && !r.target_pkg.is_empty())
            .cloned()
            .collect();

        let custom_decal = self.state.active_studio.decal.as_ref().map(|d| PresetCustomDecal {
            name: d.name.clone(),
            target_pkg: d.target_pkg.clone(),
            target_desc: d.target_desc.clone(),
            target_asset_path: d.target_asset_path.clone(),
            preferred_donor_pkg: d.preferred_donor_pkg.clone(),
            skin_png_b64: d.skin_file.as_deref().and_then(load_studio_png_b64),
            diffuse_base_png_b64: d.diffuse_base_file.as_deref().and_then(load_studio_png_b64),
            mask_png_b64: d.mask_file.as_deref().and_then(load_studio_png_b64),
        });

        let custom_pfp = self.state.active_studio.pfp.as_ref().and_then(|p| {
            load_studio_png_b64(&p.image_file).map(|b64| PresetCustomPfp {
                border_pkg: p.border_pkg.clone(),
                border_desc: p.border_desc.clone(),
                image_png_b64: b64,
            })
        });

        let custom_banner = self.state.active_studio.banner.as_ref().and_then(|b| {
            load_studio_png_b64(&b.image_file).map(|b64| PresetCustomBanner {
                banner_pkg: b.banner_pkg.clone(),
                banner_desc: b.banner_desc.clone(),
                image_png_b64: b64,
                player_name: b.player_name.clone(),
                bake_name: b.bake_name,
            })
        });

        let custom_ball = self.state.active_studio.ball.as_ref().and_then(|b| {
            load_studio_png_b64(&b.diffuse_file).map(|diff_b64| PresetCustomBall {
                name: b.name.clone(),
                diffuse_png_b64: diff_b64,
                normal_png_b64: b.normal_file.as_deref().and_then(load_studio_png_b64),
                mask_png_b64: b.mask_file.as_deref().and_then(load_studio_png_b64),
            })
        });

        LoadoutPreset {
            name: name.trim().to_string(),
            created_at: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
            item_swaps,
            custom_decal,
            custom_pfp,
            custom_banner,
            custom_ball,
            workshop_map_id: self.state.active_studio.workshop_map_id.clone(),
        }
    }

    fn apply_preset(&mut self, pr_raw: LoadoutPreset, clean_first: bool) {
        if self.is_busy {
            return;
        }
        // Reload from per-preset folder if present so direct edits to .png files inside `Presets\<Name>\` take effect immediately
        let pr_cl = load_preset_from_folder(&preset_folder_for_name(&pr_raw.name)).unwrap_or(pr_raw);
        self.state.active_preset_name = Some(pr_cl.name.clone());
        let mut stale_pkgs: Vec<String> = if clean_first {
            let mut set: HashSet<String> = self
                .state
                .active_swaps
                .values()
                .map(|r| r.target_pkg.clone())
                .collect();
            if let Some(ref rep) = self.health_report {
                for pkg in &rep.bak_filenames {
                    set.insert(pkg.clone());
                }
            }
            set.into_iter().collect()
        } else {
            Vec::new()
        };

        if clean_first {
            self.state.active_studio = ActiveStudioState::default();
        }
        self.is_busy = true;
        self.status_is_error = false;
        self.status_text = format!("Applying preset '{}'...", pr_cl.name);
        let cdir = PathBuf::from(&self.cooked_dir);
        self.spawn_op_with_repaint(move |tx| {
            for pkg in &stale_pkgs {
                restore_single_package(&cdir.join(pkg));
            }
            let mut applied_recs = Vec::new();
            let mut err_list = Vec::new();
            let mut studio_delta = StudioStateDelta::default();

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

            if let Some(ref dec) = pr_cl.custom_decal {
                let pack = AlphaConsoleDecalPack {
                    name: dec.name.clone(),
                    body_id: None,
                    skin_id: None,
                    diffuse_base_img: dec.diffuse_base_png_b64.as_deref().and_then(decode_b64_png_to_rgba),
                    skin_img: dec.skin_png_b64.as_deref().and_then(decode_b64_png_to_rgba),
                    mask_img: dec.mask_png_b64.as_deref().and_then(decode_b64_png_to_rgba),
                    chassis_diffuse_img: None,
                    source_label: "Preset".into(),
                };
                let ap = if dec.target_asset_path.is_empty() { None } else { Some(dec.target_asset_path.as_str()) };
                match inject_alphaconsole_decal_at(&cdir, &dec.target_pkg, ap, &pack, dec.preferred_donor_pkg.as_deref()) {
                    Ok(_) => {
                        applied_recs.push(ActiveSwapRecord {
                            target_pkg: dec.target_pkg.clone(),
                            donor_desc: dec.name.clone(),
                            target_desc: dec.target_desc.clone(),
                            kind: "DECAL".into(),
                            slot: "Decal".into(),
                            donor_pkg: String::new(),
                            donor_asset_path: String::new(),
                            target_asset_path: dec.target_asset_path.clone(),
                            paint_hex: None,
                            paint_name: None,
                        });
                        studio_delta.decal = Some(ActiveCustomDecalRef {
                            name: dec.name.clone(),
                            target_pkg: dec.target_pkg.clone(),
                            target_desc: dec.target_desc.clone(),
                            target_asset_path: dec.target_asset_path.clone(),
                            preferred_donor_pkg: dec.preferred_donor_pkg.clone(),
                            skin_file: pack.skin_img.as_ref().and_then(|im| save_studio_png_file("decal_skin.png", im, 1024)),
                            diffuse_base_file: pack.diffuse_base_img.as_ref().and_then(|im| save_studio_png_file("decal_base.png", im, 1024)),
                            mask_file: pack.mask_img.as_ref().and_then(|im| save_studio_png_file("decal_mask.png", im, 1024)),
                        });
                    }
                    Err(e) => err_list.push(e),
                }
            }

            if let Some(ref pfp) = pr_cl.custom_pfp {
                if let Some(img) = decode_b64_png_to_rgba(&pfp.image_png_b64) {
                    match inject_custom_pfp(&cdir, &pfp.border_pkg, &img) {
                        Ok(_) => {
                            applied_recs.push(ActiveSwapRecord {
                                target_pkg: pfp.border_pkg.clone(),
                                donor_desc: "Custom PFP".into(),
                                target_desc: pfp.border_desc.clone(),
                                kind: "PFP".into(),
                                slot: "Avatar Border".into(),
                                donor_pkg: String::new(),
                                donor_asset_path: String::new(),
                                target_asset_path: String::new(),
                                paint_hex: None,
                                paint_name: None,
                            });
                            if let Some(f) = save_studio_png_file("pfp.png", &img, 512) {
                                studio_delta.pfp = Some(ActiveCustomPfpRef {
                                    border_pkg: pfp.border_pkg.clone(),
                                    border_desc: pfp.border_desc.clone(),
                                    image_file: f,
                                });
                            }
                        }
                        Err(e) => err_list.push(e),
                    }
                }
            }

            if let Some(ref ban) = pr_cl.custom_banner {
                if let Some(img) = decode_b64_png_to_rgba(&ban.image_png_b64) {
                    let final_img = if ban.bake_name {
                        bake_name_onto_banner(&img, &ban.player_name)
                    } else {
                        img.clone()
                    };
                    match inject_custom_banner(&cdir, &ban.banner_pkg, &final_img) {
                        Ok(_) => {
                            applied_recs.push(ActiveSwapRecord {
                                target_pkg: ban.banner_pkg.clone(),
                                donor_desc: "Custom Banner".into(),
                                target_desc: ban.banner_desc.clone(),
                                kind: "BANNER".into(),
                                slot: "Player Banner".into(),
                                donor_pkg: String::new(),
                                donor_asset_path: String::new(),
                                target_asset_path: String::new(),
                                paint_hex: None,
                                paint_name: None,
                            });
                            if let Some(f) = save_studio_png_file("banner.png", &img, 840) {
                                studio_delta.banner = Some(ActiveCustomBannerRef {
                                    banner_pkg: ban.banner_pkg.clone(),
                                    banner_desc: ban.banner_desc.clone(),
                                    image_file: f,
                                    player_name: ban.player_name.clone(),
                                    bake_name: ban.bake_name,
                                });
                            }
                        }
                        Err(e) => err_list.push(e),
                    }
                }
            }

            if let Some(ref ball) = pr_cl.custom_ball {
                if let Some(diff_img) = decode_b64_png_to_rgba(&ball.diffuse_png_b64) {
                    let pack = AlphaConsoleBallPack {
                        name: ball.name.clone(),
                        group: String::new(),
                        diffuse_img: diff_img,
                        normal_img: ball.normal_png_b64.as_deref().and_then(decode_b64_png_to_rgba),
                        mask_img: ball.mask_png_b64.as_deref().and_then(decode_b64_png_to_rgba),
                        source_label: "Preset".into(),
                    };
                    match inject_alphaconsole_ball(&cdir, &pack) {
                        Ok(_) => {
                            applied_recs.push(ActiveSwapRecord {
                                target_pkg: "GameInfo_Soccar_SF.upk".into(),
                                donor_desc: ball.name.clone(),
                                target_desc: "Match Ball".into(),
                                kind: "BALL".into(),
                                slot: String::new(),
                                donor_pkg: String::new(),
                                donor_asset_path: String::new(),
                                target_asset_path: String::new(),
                                paint_hex: None,
                                paint_name: None,
                            });
                            if let Some(df) = save_studio_png_file("ball_diffuse.png", &pack.diffuse_img, 1024) {
                                studio_delta.ball = Some(ActiveCustomBallRef {
                                    name: ball.name.clone(),
                                    diffuse_file: df,
                                    normal_file: pack.normal_img.as_ref().and_then(|im| save_studio_png_file("ball_normal.png", im, 1024)),
                                    mask_file: pack.mask_img.as_ref().and_then(|im| save_studio_png_file("ball_mask.png", im, 1024)),
                                });
                            }
                        }
                        Err(e) => err_list.push(e),
                    }
                }
            }

            if let Some(ref wid) = pr_cl.workshop_map_id {
                if let Some(entry) = find_library_map_by_id_or_slug(wid) {
                    match load_workshop_map_into_underpass(&cdir, &entry) {
                        Ok(_) => {
                            let canonical_id = entry.id.clone();
                            applied_recs.push(ActiveSwapRecord {
                                target_pkg: WORKSHOP_SAFE_SLOT.into(),
                                donor_desc: entry.name,
                                target_desc: "Underpass".into(),
                                kind: "WORKSHOP".into(),
                                slot: String::new(),
                                donor_pkg: String::new(),
                                donor_asset_path: String::new(),
                                target_asset_path: String::new(),
                                paint_hex: None,
                                paint_name: None,
                            });
                            studio_delta.workshop_map_id = Some(canonical_id);
                        }
                        Err(e) => err_list.push(e),
                    }
                }
            }

            clear_rl_texture_cache();
            stale_pkgs.shrink_to_fit();
            let _ = tx.send(WorkerEvent::OperationDone {
                success: err_list.is_empty(),
                message: if err_list.is_empty() {
                    format!("Applied '{}'", pr_cl.name)
                } else {
                    err_list.join("; ")
                },
                swapped_pkgs: applied_recs,
                cleared_pkgs: stale_pkgs,
                studio_delta: Some(studio_delta),
            });
        });
    }

    fn import_preset_file(&mut self, p: &Path) {
        match fs::read_to_string(p)
            .map_err(|e| e.to_string())
            .and_then(|txt| serde_json::from_str::<LoadoutPreset>(&txt).map_err(|e| format!("Invalid preset JSON: {e}")))
        {
            Ok(pr) => {
                let name = pr.name.clone();
                self.presets.retain(|existing| !existing.name.eq_ignore_ascii_case(&pr.name));
                self.presets.push(pr);
                self.save_presets();
                self.active_tab = ActiveTab::Presets;
                self.status_is_error = false;
                self.status_text = format!("Imported preset '{}'", name);
            }
            Err(e) => {
                self.status_is_error = true;
                self.status_text = e;
            }
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|df| df.path.clone())
                .collect()
        });
        for p in dropped {
            match classify_dropped_file(&p) {
                DroppedFileKind::WorkshopMap => match import_workshop_file_or_zip(&p, None, None) {
                    Ok(entry) => {
                        self.workshop_library = list_library_maps();
                        if let Some(pos) = self.workshop_library.iter().position(|e| e.id == entry.id) {
                            self.workshop_selected_idx = pos;
                        }
                        self.workshop_missing_deps = scan_upk_external_dependencies(
                            &get_workshop_library_dir().join(&entry.id).join(&entry.map_file_rel),
                            Path::new(&self.cooked_dir),
                        );
                        self.active_tab = ActiveTab::Workshop;
                        self.status_is_error = false;
                        self.status_text = format!("Imported Workshop map '{}'", entry.name);
                    }
                    Err(e) => {
                        self.status_is_error = true;
                        self.status_text = e;
                    }
                },
                DroppedFileKind::PresetJson => self.import_preset_file(&p),
                DroppedFileKind::DecalOrBallPack => {
                    if self.active_tab == ActiveTab::MatchBall {
                        self.import_ball_file(&p);
                    } else if parse_alphaconsole_decal_source(&p).is_ok() {
                        self.active_tab = ActiveTab::Decals;
                        self.import_decal_file(&p);
                    } else if parse_alphaconsole_ball_source(&p).is_ok() {
                        self.active_tab = ActiveTab::MatchBall;
                        self.import_ball_file(&p);
                    } else {
                        self.active_tab = ActiveTab::Decals;
                        self.import_decal_file(&p);
                    }
                }
                DroppedFileKind::ImageAsset => match self.active_tab {
                    ActiveTab::AvatarPfp => {
                        if let Ok(im) = image::open(&p) {
                            let fname = p.file_name().unwrap_or_default().to_string_lossy().to_string();
                            self.open_image_cropper(ctx, CropTarget::Pfp, Some(p), im.to_rgba8(), fname);
                        }
                    }
                    ActiveTab::Banner => {
                        if let Ok(im) = image::open(&p) {
                            let fname = p.file_name().unwrap_or_default().to_string_lossy().to_string();
                            self.open_image_cropper(ctx, CropTarget::Banner, Some(p), im.to_rgba8(), fname);
                        }
                    }
                    ActiveTab::MatchBall => self.import_ball_file(&p),
                    _ => {
                        self.active_tab = ActiveTab::Decals;
                        self.import_decal_file(&p);
                    }
                },
                DroppedFileKind::Unsupported => {
                    self.status_is_error = true;
                    self.status_text = format!(
                        "Unsupported dropped file: {}",
                        p.file_name().unwrap_or_default().to_string_lossy()
                    );
                }
            }
        }
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
            self.decal_custom_img = if pack.mask_img.is_none() && pack.diffuse_base_img.is_some() {
                pack.diffuse_base_img.or(pack.skin_img)
            } else {
                pack.skin_img.or(pack.diffuse_base_img)
            };
            self.decal_mask_img = pack.mask_img;
            self.decal_custom_path = format!("{} ({})", pack.name, pack.source_label);

            if let Some(bid) = pack.body_id {
                if let Some(body_item) = find_item_by_id_in_slots(&self.items_by_slot, bid) {
                    if let Some(lbl) = detect_car_uv_label(&format!("{} {}", body_item.product, body_item.asset_package)) {
                        self.decal_car_label = lbl;
                    }
                }
            }
            if let Some(sid) = pack.skin_id {
                if let Some(skin_item) = find_item_by_id_in_slots(&self.items_by_slot, sid) {
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
            save_custom_decal_to_library(&self.ac_decal_packs[self.ac_decal_idx], self.decal_car_label);
            self.status_is_error = false;
            self.status_text = format!("Loaded decal '{}' (saved to Custom Items/Decals)", pack.name);
        }
    }

    fn apply_selected_ac_ball_pack(&mut self) {
        if let Some(pack) = self.ac_ball_packs.get(self.ac_ball_idx).cloned() {
            save_custom_ball_to_library(&pack);
            self.ball_custom_img = Some(pack.diffuse_img);
            self.ball_normal_img = pack.normal_img;
            self.ball_mask_img = pack.mask_img;
            self.ball_custom_path = format!("{} ({})", pack.name, pack.source_label);
            self.ball_orb_dirty = true;
            self.status_is_error = false;
            self.status_text = format!("Loaded ball '{}' (saved to Custom Items/Balls)", pack.name);
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
        if self.egui_ctx.is_none() {
            self.egui_ctx = Some(ctx.clone());
        }
        paint_top_vignette(ctx);

        // Drain worker events
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                WorkerEvent::HealthReady(rep) => {
                    self.health_report = Some(rep);
                }
                WorkerEvent::IconReady { cache_key, rgba } => {
                    self.pending_icons.remove(&cache_key);
                    let tex = ctx.load_texture(&cache_key, rgba_to_egui_image(&rgba), TextureOptions::LINEAR);
                    evict_fifo_insert(
                        &mut self.gpu_icons,
                        &mut self.gpu_icon_order,
                        MAX_GPU_ICONS,
                        cache_key,
                        tex,
                    );
                }
                WorkerEvent::CategorySyncDone { slot, local, web } => {
                    self.is_busy = false;
                    self.status_is_error = false;
                    self.gpu_icons.clear();
                    self.gpu_icon_order.clear();
                    self.pending_icons.clear();
                    self.status_text = format!("Synced {}: {} local · {} online", slot, local, web);
                }
                WorkerEvent::WorkshopCatalogDone(res) => {
                    self.workshop_online_loading = false;
                    match res {
                        Ok(items) => {
                            let cnt = items.len();
                            self.workshop_online_items = items;
                            self.cache_ws_key.4 = usize::MAX;
                            self.status_is_error = false;
                            self.status_text = format!("{} online maps ready", cnt);
                        }
                        Err(e) => {
                            self.status_is_error = true;
                            self.status_text = e;
                        }
                    }
                }
                WorkerEvent::WorkshopThumbDone { url, rgba } => {
                    self.workshop_online_pending_thumbs.remove(&url);
                    if let Some(im) = rgba {
                        let tex = ctx.load_texture(&format!("ws_{}", url), rgba_to_egui_image(&im), TextureOptions::LINEAR);
                        evict_fifo_insert(
                            &mut self.workshop_online_thumbs,
                            &mut self.ws_thumb_order,
                            MAX_WS_THUMBS,
                            url,
                            tex,
                        );
                    } else {
                        if self.workshop_online_failed_thumbs.len() >= MAX_FAILED_THUMBS {
                            self.workshop_online_failed_thumbs.clear();
                        }
                        self.workshop_online_failed_thumbs.insert(url);
                    }
                }
                WorkerEvent::WorkshopDownloadProgress(prog) => {
                    self.workshop_dl_progress = Some(prog);
                }
                WorkerEvent::WorkshopDownloadDone(res) => {
                    self.is_busy = false;
                    self.workshop_dl_progress = None;
                    match res {
                        Ok((entry, deps)) => {
                            self.workshop_library = list_library_maps();
                            if let Some(pos) = self.workshop_library.iter().position(|e| e.id == entry.id) {
                                self.workshop_selected_idx = pos;
                            }
                            self.workshop_missing_deps = deps;
                            self.workshop_local_thumb = (String::new(), None);
                            self.status_is_error = false;
                            self.status_text = format!("Downloaded '{}'", entry.name);
                        }
                        Err(e) => {
                            self.status_is_error = true;
                            self.status_text = e;
                        }
                    }
                }
                WorkerEvent::WorkshopTexturesDone(res) => {
                    self.is_busy = false;
                    self.workshop_dl_progress = None;
                    match res {
                        Ok((copied, _skipped, cached_total)) => {
                            if let Some(sel) = self.workshop_library.get(self.workshop_selected_idx) {
                                self.workshop_missing_deps = scan_upk_external_dependencies(
                                    &get_workshop_library_dir().join(&sel.id).join(&sel.map_file_rel),
                                    Path::new(&self.cooked_dir),
                                );
                            }
                            append_swap_history_log(&format!(
                                "WORKSHOP TEXTURES: installed {} new packages ({} cached in Hub)",
                                copied, cached_total
                            ));
                            self.status_is_error = false;
                            self.status_text = format!(
                                "Workshop Textures installed ({} new in CookedPCConsole · {} cached in Hub)",
                                copied, cached_total
                            );
                        }
                        Err(e) => {
                            self.status_is_error = true;
                            self.status_text = e;
                        }
                    }
                }
                WorkerEvent::WorkshopLoadDone { success, message, rec, missing_deps, workshop_map_id } => {
                    self.is_busy = false;
                    self.status_is_error = !success;
                    self.status_text = message;
                    self.workshop_missing_deps = missing_deps;
                    if success {
                        self.state.active_studio.workshop_map_id = workshop_map_id;
                    }
                    if let Some(r) = rec {
                        self.record_swap(r);
                    }
                    self.refresh_health_report();
                }
                WorkerEvent::OperationDone { success, message, swapped_pkgs, cleared_pkgs, studio_delta } => {
                    self.is_busy = false;
                    self.status_is_error = !success;
                    self.status_text = message;
                    let mut changed = false;
                    let mut cosmetic_pairs: Vec<(String, String)> = Vec::new();
                    for pkg in cleared_pkgs {
                        if self.state.active_swaps.remove(&pkg.to_lowercase()).is_some() {
                            self.state.swap_history.retain(|x| x != &pkg.to_lowercase());
                            changed = true;
                        }
                    }
                    for rec in swapped_pkgs {
                        if rec.kind == "ITEM" || rec.kind == "PAINT" {
                            cosmetic_pairs.push((rec.donor_pkg.clone(), rec.target_pkg.clone()));
                        }
                        self.record_swap(rec);
                        changed = true;
                    }
                    if let Some(sd) = studio_delta {
                        let had_studio = sd.decal.is_some()
                            || sd.pfp.is_some()
                            || sd.banner.is_some()
                            || sd.ball.is_some();
                        if let Some(d) = sd.decal {
                            self.state.active_studio.decal = Some(d);
                            changed = true;
                        }
                        if let Some(p) = sd.pfp {
                            self.state.active_studio.pfp = Some(p);
                            self.pfp_preview_dirty = true;
                            changed = true;
                        }
                        if let Some(b) = sd.banner {
                            self.state.active_studio.banner = Some(b);
                            changed = true;
                        }
                        if let Some(bl) = sd.ball {
                            self.state.active_studio.ball = Some(bl);
                            changed = true;
                        }
                        if let Some(wid) = sd.workshop_map_id {
                            self.state.active_studio.workshop_map_id = Some(wid);
                            changed = true;
                        }
                        if had_studio {
                            self.sync_studio_from_active(false);
                        }
                    }
                    if changed {
                        self.bump_swaps_revision();
                        self.save_state();
                    }
                    if success && !cosmetic_pairs.is_empty() {
                        self.reset_cosmetic_after_swap(&cosmetic_pairs);
                    }
                    self.refresh_health_report();
                }
            }
        }

        // Global shortcuts & drag-and-drop routing
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.crop_modal.is_some() {
                self.crop_modal = None;
            } else if self.show_reset_modal {
                self.show_reset_modal = false;
            }
        }
        if !ctx.wants_keyboard_input()
            && self.crop_modal.is_none()
            && !self.is_busy
            && !self.state.active_swaps.is_empty()
            && ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Z))
        {
            self.undo_last_swap();
        }
        if self.crop_modal.is_none() {
            self.handle_dropped_files(ctx);
        }

        if self.last_proc_check.elapsed() > Duration::from_secs(5) {
            self.rl_running = is_rocket_league_running();
            self.last_proc_check = Instant::now();
        }

        let swap_cnt = self
            .health_report
            .as_ref()
            .map(|r| r.total_bak_files.max(self.state.active_swaps.len()))
            .unwrap_or(self.state.active_swaps.len());
        let cooked_ok = Path::new(&self.cooked_dir).is_dir();

        // Minimalist Barebones Top Header (Zero Sidebar, Clean Dropdowns & Links)
        egui::TopBottomPanel::top("top_nav_bar")
            .exact_height(46.0)
            .frame(egui::Frame::none().fill(Color32::TRANSPARENT).inner_margin(egui::Margin::symmetric(24.0, 9.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // Bare monochrome emblem + title
                    let (lr, _) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
                    ui.painter().rect_filled(lr, Rounding::same(6.0), Color32::BLACK);
                    if let Some(ref tex) = self.app_logo_tex {
                        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                        ui.painter().image(tex.id(), lr, uv, Color32::WHITE);
                    }
                    ui.painter().rect_stroke(lr, Rounding::same(6.0), Stroke::new(1.0_f32, BORDER_SUBTLE));
                    ui.add_space(4.0);
                    ui.label(RichText::new("RL Swapper").font(FontId::proportional(13.5)).color(TEXT_PRIMARY).strong());
                    ui.label(RichText::new("/").font(FontId::proportional(13.0)).color(TEXT_MUTED));

                    // Top Navigation: Cosmetics + Workshop + Studio Dropdown + System Dropdown
                    if ui.add(nav_tab_button("Cosmetics", self.active_tab == ActiveTab::Cosmetics)).clicked() {
                        self.active_tab = ActiveTab::Cosmetics;
                    }
                    if ui.add(nav_tab_button("Workshop", self.active_tab == ActiveTab::Workshop)).clicked() {
                        self.active_tab = ActiveTab::Workshop;
                    }

                    let studio_lbl = match self.active_tab {
                        ActiveTab::Decals => "Studio: Decals",
                        ActiveTab::AvatarPfp => "Studio: Avatar PFP",
                        ActiveTab::Banner => "Studio: Banner",
                        ActiveTab::MatchBall => "Studio: Match Ball",
                        _ => "Studio",
                    };
                    egui::ComboBox::from_id_salt("nav_studio_menu")
                        .selected_text(RichText::new(studio_lbl).font(FontId::proportional(12.2)).color(
                            if matches!(self.active_tab, ActiveTab::Decals | ActiveTab::AvatarPfp | ActiveTab::Banner | ActiveTab::MatchBall) {
                                TEXT_PRIMARY
                            } else {
                                TEXT_SECONDARY
                            },
                        ))
                        .width(122.0)
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(self.active_tab == ActiveTab::Decals, "Decals").clicked() {
                                self.active_tab = ActiveTab::Decals;
                            }
                            if ui.selectable_label(self.active_tab == ActiveTab::AvatarPfp, "Avatar PFP").clicked() {
                                self.active_tab = ActiveTab::AvatarPfp;
                            }
                            if ui.selectable_label(self.active_tab == ActiveTab::Banner, "Banner").clicked() {
                                self.active_tab = ActiveTab::Banner;
                            }
                            if ui.selectable_label(self.active_tab == ActiveTab::MatchBall, "Match Ball").clicked() {
                                self.active_tab = ActiveTab::MatchBall;
                            }
                        });

                    let sys_lbl = match self.active_tab {
                        ActiveTab::Presets => match self.state.active_preset_name.as_deref() {
                            Some(pn) => format!("Presets: {}", pn),
                            None => "System: Presets".into(),
                        },
                        ActiveTab::Health => "System: Health".into(),
                        _ => "System".into(),
                    };
                    egui::ComboBox::from_id_salt("nav_system_menu")
                        .selected_text(RichText::new(sys_lbl).font(FontId::proportional(12.2)).color(
                            if matches!(self.active_tab, ActiveTab::Presets | ActiveTab::Health) {
                                TEXT_PRIMARY
                            } else {
                                TEXT_SECONDARY
                            },
                        ))
                        .width(124.0)
                        .show_ui(ui, |ui| {
                            let pr_menu_lbl = match self.state.active_preset_name.as_deref() {
                                Some(pn) => format!("Presets ({}) · {}", self.presets.len(), pn),
                                None => format!("Presets ({})", self.presets.len()),
                            };
                            if ui.selectable_label(self.active_tab == ActiveTab::Presets, pr_menu_lbl).clicked() {
                                self.active_tab = ActiveTab::Presets;
                            }
                            if ui.selectable_label(self.active_tab == ActiveTab::Health, &format!("Health ({})", swap_cnt)).clicked() {
                                self.refresh_health_report();
                                self.active_tab = ActiveTab::Health;
                            }
                        });

                    // Top-Right Quiet Utility Pills (Undo · Reset · Open Hub · Folder · Status Dot)
                    ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                        let (dot_col, state_lbl) = if self.rl_running { (LEMON, "Live") } else { (MINT, "Idle") };
                        let state_tip = if self.rl_running {
                            "Rocket League is running — close game before swapping .upk files"
                        } else {
                            "Rocket League is not running (safe to swap)"
                        };
                        rui.label(RichText::new(state_lbl).font(FontId::proportional(11.0)).color(dot_col))
                            .on_hover_text(state_tip);
                        paint_dot(rui, dot_col, 3.0);
                        rui.add_space(4.0);

                        let gf_resp = rui
                            .add(pill_button("Folder", if cooked_ok { TEXT_SECONDARY } else { LEMON }))
                            .on_hover_text(format!("CookedPCConsole:\n{}", self.cooked_dir));
                        if gf_resp.clicked() {
                            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                                self.apply_cooked_dir(folder);
                            }
                        }

                        if rui
                            .add(pill_button("Open Hub", TEXT_SECONDARY))
                            .on_hover_text(format!(
                                "Open Downloads\\RL Swapper Hub (Presets, Custom Items, Workshop Maps & Backups):\n{}",
                                get_swapper_hub_dir().display()
                            ))
                            .clicked()
                        {
                            let _ = std::process::Command::new("explorer").arg(get_swapper_hub_dir()).spawn();
                        }

                        if rui
                            .add(pill_button(&format!("Reset ({})", swap_cnt), if swap_cnt > 0 { CORAL } else { TEXT_SECONDARY }))
                            .clicked()
                        {
                            self.show_reset_modal = true;
                        }

                        let undo_tip = self
                            .peek_next_undo_rec()
                            .map(|r| format!("Undo: Restore {} ({}) · Ctrl+Z", r.target_desc, r.kind))
                            .unwrap_or_else(|| "Nothing to undo (Ctrl+Z)".into());
                        if rui
                            .add_enabled(
                                !self.state.active_swaps.is_empty() && !self.is_busy,
                                pill_button("Undo", TEXT_SECONDARY),
                            )
                            .on_hover_text(undo_tip)
                            .clicked()
                        {
                            self.undo_last_swap();
                        }
                    });
                });
            });

        // Subtle Minimal Footer Status Bar
        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(26.0)
            .frame(egui::Frame::none().fill(Color32::TRANSPARENT).inner_margin(egui::Margin::symmetric(24.0, 4.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let dot_col = if self.is_busy {
                        WHITE_CTA_BG
                    } else if self.status_is_error {
                        CORAL
                    } else {
                        MINT
                    };
                    paint_dot(ui, dot_col, 2.8);
                    let col = if self.status_is_error { CORAL } else { TEXT_MUTED };
                    let w = (ui.available_width() - 180.0).max(80.0);
                    ui.allocate_ui(Vec2::new(w, 18.0), |lui| {
                        lui.add(egui::Label::new(RichText::new(&self.status_text).font(FontId::proportional(11.0)).color(col)).truncate());
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                        rui.label(
                            RichText::new(if cooked_ok { "Game Ready" } else { "Game Path Missing" })
                                .font(FontId::proportional(10.8))
                                .color(if cooked_ok { TEXT_MUTED } else { LEMON }),
                        );
                    });
                });
            });

        // Main Single-Canvas Workspace
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::TRANSPARENT).inner_margin(egui::Margin::symmetric(24.0, 6.0)))
            .show(ctx, |ui| {
                ui.set_max_width(ui.available_width());
                if !cooked_ok {
                    egui::Frame::none()
                        .fill(Color32::from_rgba_unmultiplied(CORAL.r(), CORAL.g(), CORAL.b(), 20))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(CORAL.r(), CORAL.g(), CORAL.b(), 85)))
                        .rounding(Rounding::same(8.0))
                        .inner_margin(egui::Margin::symmetric(12.0, 7.0))
                        .show(ui, |wui| {
                            wui.horizontal(|hui| {
                                hui.label(RichText::new("CookedPCConsole folder not found — point RL Swapper to your game directory.").font(FontId::proportional(11.8)).color(TEXT_PRIMARY));
                                hui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                                    if rui.add(pill_button("Select Folder", WHITE_CTA_BG)).clicked() {
                                        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                                            self.apply_cooked_dir(folder);
                                        }
                                    }
                                    if rui.add(pill_button("Auto-Detect", TEXT_SECONDARY)).clicked() {
                                        if let Some(found) = detect_cooked_dir_checked() {
                                            self.apply_cooked_dir(found);
                                        } else {
                                            self.status_is_error = true;
                                            self.status_text = "Auto-detect failed — select CookedPCConsole manually".into();
                                        }
                                    }
                                });
                            });
                        });
                    ui.add_space(6.0);
                } else if self.rl_running {
                    egui::Frame::none()
                        .fill(Color32::from_rgba_unmultiplied(LEMON.r(), LEMON.g(), LEMON.b(), 16))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(LEMON.r(), LEMON.g(), LEMON.b(), 72)))
                        .rounding(Rounding::same(8.0))
                        .inner_margin(egui::Margin::symmetric(12.0, 5.0))
                        .show(ui, |wui| {
                            wui.label(
                                RichText::new("Rocket League is running — close the game before modifying .upk files so changes reload cleanly.")
                                    .font(FontId::proportional(11.3))
                                    .color(LEMON),
                            );
                        });
                    ui.add_space(6.0);
                }
                match self.active_tab {
                    ActiveTab::Cosmetics => self.render_cosmetics_view(ui),
                    ActiveTab::Workshop => self.render_workshop_maps_studio(ctx, ui),
                    ActiveTab::Decals => self.render_custom_decal_studio(ctx, ui),
                    ActiveTab::AvatarPfp => self.render_custom_pfp_studio(ctx, ui),
                    ActiveTab::Banner => self.render_custom_banner_studio(ctx, ui),
                    ActiveTab::MatchBall => self.render_custom_ball_studio(ctx, ui),
                    ActiveTab::Presets => self.render_presets_tab(ui),
                    ActiveTab::Health => self.render_active_health_tab(ui),
                }
            });

        if self.show_reset_modal {
            self.render_reset_modal(ctx);
        }
        if self.crop_modal.is_some() {
            self.render_image_crop_modal(ctx);
        }

        self.handle_env_screenshot(ctx);
    }
}

impl RLSwapperGui {
    fn handle_env_screenshot(&mut self, ctx: &egui::Context) {
        let Some((dir, pfx, idx, phase)) = self.snap_state.clone() else {
            return;
        };
        let shots: [(&str, ActiveTab, bool, Option<CropTarget>); 11] = [
            ("body", ActiveTab::Cosmetics, false, None),
            ("workshop", ActiveTab::Workshop, false, None),
            ("decal", ActiveTab::Decals, false, None),
            ("pfp", ActiveTab::AvatarPfp, false, None),
            ("banner", ActiveTab::Banner, false, None),
            ("ball", ActiveTab::MatchBall, false, None),
            ("presets", ActiveTab::Presets, false, None),
            ("health", ActiveTab::Health, false, None),
            ("reset_modal", ActiveTab::Cosmetics, true, None),
            ("crop_pfp", ActiveTab::AvatarPfp, false, Some(CropTarget::Pfp)),
            ("crop_banner", ActiveTab::Banner, false, Some(CropTarget::Banner)),
        ];
        let mut saved_now = false;
        ctx.input(|i| {
            for ev in &i.events {
                if let egui::Event::Screenshot { image, .. } = ev {
                    let _ = fs::create_dir_all(&dir);
                    if let Some((name, _, _, _)) = shots.get(idx) {
                        let [w, h] = image.size;
                        if let Some(rgba) = RgbaImage::from_raw(w as u32, h as u32, image.as_raw().to_vec()) {
                            let _ = rgba.save(dir.join(format!("{}_{}.png", pfx, name)));
                        }
                    }
                    saved_now = true;
                }
            }
        });
        let mut next_open_crop: Option<CropTarget> = None;
        if let Some(ref mut st) = self.snap_state {
            if saved_now {
                if st.2 + 1 >= shots.len() {
                    self.show_reset_modal = false;
                    self.crop_modal = None;
                    self.snap_state = None;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                } else {
                    st.2 += 1;
                    st.3 = 0;
                    self.active_tab = shots[st.2].1;
                    self.show_reset_modal = shots[st.2].2;
                    next_open_crop = shots[st.2].3;
                    if next_open_crop.is_none() {
                        self.crop_modal = None;
                    }
                }
            } else if phase < 6 {
                st.3 = phase + 1;
                self.active_tab = shots[idx].1;
                self.show_reset_modal = shots[idx].2;
                if let Some(ct) = shots[idx].3 {
                    if self.crop_modal.as_ref().map(|m| m.target) != Some(ct) {
                        next_open_crop = Some(ct);
                    }
                }
            } else if phase == 6 {
                st.3 = 7;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
            }
            ctx.request_repaint();
        }
        if let Some(ct) = next_open_crop {
            let sample = RgbaImage::from_fn(640, 360, |x, y| {
                let r = ((x as f32 / 640.0) * 210.0) as u8;
                let g = ((y as f32 / 360.0) * 190.0) as u8;
                image::Rgba([r.saturating_add(30), g.saturating_add(35), 140, 255])
            });
            self.open_image_cropper(ctx, ct, None, sample, "sample_16x9.png".into());
        }
    }

    // ── Shared Studio Sub-Tab Folder Strip (Like "Invoice lines | Details | Docs | Notes") ──
    fn render_studio_tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for (tab, lbl) in [
                (ActiveTab::Decals, "Decals"),
                (ActiveTab::AvatarPfp, "Avatar PFP"),
                (ActiveTab::Banner, "Banner"),
                (ActiveTab::MatchBall, "Match Ball"),
            ] {
                if folder_tab_btn(ui, self.active_tab == tab, lbl) {
                    self.active_tab = tab;
                }
            }
        });
        ui.add_space(6.0);
    }

    // ── 1. Cosmetics View (Zero-Clone Borrowed Pickers + Compact Dropdowns) ───
    fn render_cosmetics_view(&mut self, ui: &mut egui::Ui) {
        self.sync_cosmetic_caches();
        let slot = self.current_slot();
        let is_body_slot = slot == "Body";

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
        let total_slot_cnt = self.items_by_slot.get(slot).map(|v| v.len()).unwrap_or(0);

        // Hero Summary Strip (Crisp Large Heading + Metadata Columns + White Pill CTA)
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let donor_nm = want_item.as_ref().map(|w| w.product.as_str()).unwrap_or("Select Donor");
            let target_nm = have_item.as_ref().map(|h| h.product.as_str()).unwrap_or("Select Target");

            let preview_key = want_item.as_ref().map(|w| {
                Self::queue_item_icon(
                    &self.icon_job_tx,
                    &self.cooked_dir,
                    &self.gpu_icons,
                    &mut self.pending_icons,
                    w,
                    paint_spec.as_ref(),
                    64,
                )
            });
            let (ir, _) = ui.allocate_exact_size(Vec2::splat(44.0), Sense::hover());
            ui.painter().rect_filled(ir, Rounding::same(9.0), BG_WELL);
            ui.painter().rect_stroke(ir, Rounding::same(9.0), Stroke::new(1.0_f32, BORDER_SUBTLE));
            if let Some(tid) = preview_key.as_ref().and_then(|k| self.gpu_icons.get(k)) {
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                ui.painter().image(tid.id(), ir.shrink(4.0), uv, Color32::WHITE);
            }
            ui.add_space(10.0);

            let headline_w = (ui.available_width() - 470.0).max(160.0);
            ui.allocate_ui_with_layout(Vec2::new(headline_w, 44.0), Layout::top_down(Align::LEFT), |hui| {
                hui.spacing_mut().item_spacing.y = 1.0;
                let h_txt = if donor_nm == target_nm {
                    donor_nm.to_string()
                } else {
                    format!("{} -> {}", donor_nm, target_nm)
                };
                hui.add(egui::Label::new(RichText::new(h_txt).font(FontId::proportional(20.0)).color(TEXT_PRIMARY).strong()).truncate());
                hui.label(
                    RichText::new(format!("{} Items in {} · Select donor on left, owned target on right", total_slot_cnt, slot))
                        .font(FontId::proportional(11.0))
                        .color(TEXT_MUTED),
                );
            });

            // Sparse Metadata Columns
            let target_rec = have_item
                .as_ref()
                .and_then(|h| self.state.active_swaps.get(&h.asset_package.to_lowercase()));
            let target_swapped = have_item.as_ref().map_or(false, |h| self.is_pkg_swapped(&h.asset_package));
            ui.vertical(|pui| {
                pui.spacing_mut().item_spacing.y = 0.0;
                pui.label(RichText::new("Paint").font(FontId::proportional(9.5)).color(TEXT_MUTED));
                pui.horizontal(|hui| {
                    hui.spacing_mut().item_spacing.x = 5.0;
                    if let Some(ref ps) = paint_spec {
                        let [pr, pg, pb] = ps.rgb8();
                        paint_dot(hui, Color32::from_rgb(pr, pg, pb), 4.0);
                        hui.add(egui::Label::new(RichText::new(&ps.name).font(FontId::proportional(12.0)).color(TEXT_PRIMARY)).truncate());
                    } else {
                        hui.label(RichText::new("Default").font(FontId::proportional(12.0)).color(TEXT_PRIMARY));
                    }
                });
            });
            ui.add_space(14.0);
            if is_body_slot {
                if let (Some(w), Some(h)) = (&want_item, &have_item) {
                    let wh = body_hitbox_for_item(&w.product, &w.asset_package);
                    let hh = body_hitbox_for_item(&h.product, &h.asset_package);
                    let (hl, hc, htip) = if wh == hh {
                        (
                            format!("{} Match", hh.label()),
                            MINT,
                            "Same hitbox family as donor — visual mesh & physics align 1:1",
                        )
                    } else {
                        (
                            format!("{} Physics", hh.label()),
                            LEMON,
                            "Online matches use your owned target car's hitbox physics",
                        )
                    };
                    meta_col(ui, "Hitbox", &hl, hc).on_hover_text(htip);
                    ui.add_space(14.0);
                }
            }
            let status_val = if let Some(rec) = target_rec {
                match &rec.paint_name {
                    Some(pn) => format!("Swapped · {} [{}]", rec.donor_desc, pn),
                    None => format!("Swapped · {}", rec.donor_desc),
                }
            } else if target_swapped {
                "Swapped".into()
            } else {
                "Vanilla".into()
            };
            meta_col(ui, "Status", if target_swapped { "Swapped" } else { "Vanilla" }, if target_swapped { MINT } else { TEXT_SECONDARY })
                .on_hover_text(status_val);

            ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                let mut launch_pair: Option<(CatalogItem, CatalogItem)> = None;
                let same_pkg = match (&want_item, &have_item) {
                    (Some(w), Some(h)) => w.asset_package.eq_ignore_ascii_case(&h.asset_package) && w.asset_path == h.asset_path,
                    _ => false,
                };
                let can_swap = !self.is_busy
                    && want_item.is_some()
                    && have_item.is_some()
                    && (!same_pkg || paint_spec.is_some());
                let cta_lbl = if self.is_busy {
                    "Working..."
                } else if same_pkg && paint_spec.is_some() {
                    "Apply Paint"
                } else if same_pkg {
                    "Same Item"
                } else {
                    "Swap Item"
                };
                if white_cta_button(rui, can_swap, cta_lbl, 108.0) {
                    if let (Some(w), Some(h)) = (want_item.clone(), have_item.clone()) {
                        launch_pair = Some((w, h));
                    }
                }
                if target_swapped {
                    if let Some(h) = have_item.clone() {
                        if rui.add_enabled(!self.is_busy, pill_button("Restore", CORAL)).clicked() {
                            self.restore_target_pkg(&h.asset_package, &h.product);
                        }
                    }
                }
                if let Some((w_it, h_it)) = launch_pair {
                    self.is_busy = true;
                    let cdir = PathBuf::from(&self.cooked_dir);
                    let p_spec = self.active_paint_spec();
                    let slot_owned = slot.to_string();
                    self.spawn_op_with_repaint(move |tx| match swap_item_packages(&cdir, &w_it, &h_it, p_spec.as_ref()) {
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
                                studio_delta: None,
                            });
                        }
                        Err(e) => {
                            let _ = tx.send(WorkerEvent::OperationDone {
                                success: false,
                                message: e,
                                swapped_pkgs: vec![],
                                cleared_pkgs: vec![],
                                studio_delta: None,
                            });
                        }
                    });
                }
            });
        });
        ui.add_space(10.0);

        // Main Clean Card Containing Dropdown Controls + Dual Pickers
        surface_frame().show(ui, |ui| {
            ui.set_min_width(ui.available_width());

            // Compact Dropdown Filter Row (Replaces 12 Slot Rows + 7 Hitbox Chips + 15 Paint Swatches)
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Slot").font(FontId::proportional(11.5)).color(TEXT_MUTED));
                let cur_slot_idx = self.active_slot_idx;
                let mut next_slot_idx = None;
                egui::ComboBox::from_id_salt("cosmetic_slot_dd")
                    .selected_text(RichText::new(slot).color(TEXT_PRIMARY))
                    .width(134.0)
                    .show_ui(ui, |ui| {
                        for (idx, &s) in ORDERED_SLOTS.iter().enumerate() {
                            let cnt = self.items_by_slot.get(s).map(|v| v.len()).unwrap_or(0);
                            if ui.selectable_label(cur_slot_idx == idx, format!("{}  ({})", s, cnt)).clicked() {
                                next_slot_idx = Some(idx);
                            }
                        }
                    });
                if let Some(ns) = next_slot_idx {
                    self.switch_cosmetic_slot(ns);
                }

                let mut filters_changed = false;
                if self.current_slot() == "Body" {
                    ui.add_space(6.0);
                    ui.label(RichText::new("Hitbox").font(FontId::proportional(11.5)).color(TEXT_MUTED));
                    let hb_lbl = self.body_hitbox_filter.map(|h| h.label()).unwrap_or("All");
                    egui::ComboBox::from_id_salt("cosmetic_hb_dd")
                        .selected_text(RichText::new(hb_lbl).color(TEXT_PRIMARY))
                        .width(104.0)
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(self.body_hitbox_filter.is_none(), "All Hitboxes").clicked() {
                                self.body_hitbox_filter = None;
                                filters_changed = true;
                            }
                            for hb in [
                                CarHitbox::Octane,
                                CarHitbox::Dominus,
                                CarHitbox::Hybrid,
                                CarHitbox::Breakout,
                                CarHitbox::Plank,
                                CarHitbox::Merc,
                            ] {
                                if ui.selectable_label(self.body_hitbox_filter == Some(hb), hb.label()).clicked() {
                                    self.body_hitbox_filter = Some(hb);
                                    filters_changed = true;
                                }
                            }
                        });
                }

                ui.add_space(6.0);
                ui.label(RichText::new("Paint").font(FontId::proportional(11.5)).color(TEXT_MUTED));
                let paint_lbl = if self.use_custom_hex {
                    let [r, g, b] = self.custom_hex_color;
                    format!("Custom #{:02X}{:02X}{:02X}", r, g, b)
                } else {
                    self.selected_paint_idx
                        .and_then(|i| self.paints.get(i))
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| "Default".into())
                };
                egui::ComboBox::from_id_salt("cosmetic_paint_dd")
                    .selected_text(RichText::new(paint_lbl).color(TEXT_PRIMARY))
                    .width(156.0)
                    .show_ui(ui, |ui| {
                        let def_sel = self.selected_paint_idx.is_none() && !self.use_custom_hex;
                        if paint_swatch_row(ui, def_sel, "Default", None) {
                            self.selected_paint_idx = None;
                            self.use_custom_hex = false;
                        }
                        for (idx, p) in self.paints.iter().enumerate() {
                            let is_sel = !self.use_custom_hex && self.selected_paint_idx == Some(idx);
                            if paint_swatch_row(ui, is_sel, &p.name, Some(p.rgb8())) {
                                self.selected_paint_idx = Some(idx);
                                self.use_custom_hex = false;
                            }
                        }
                        if paint_swatch_row(ui, self.use_custom_hex, "Custom HEX...", Some(self.custom_hex_color)) {
                            self.use_custom_hex = true;
                        }
                    });
                if self.use_custom_hex {
                    ui.color_edit_button_srgb(&mut self.custom_hex_color);
                }

                let mut toggle_fav: Option<(String, String)> = None;
                ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                    if rui.add_enabled(!self.is_busy, pill_button("Sync Icons", TEXT_SECONDARY)).clicked() {
                        self.is_busy = true;
                        self.status_text = format!("Syncing {} icons...", slot);
                        let mgr = Arc::clone(&self.icon_mgr);
                        let cdir = PathBuf::from(&self.cooked_dir);
                        let slot_str = self.current_slot().to_string();
                        let list_cl = self.items_by_slot.get(slot_str.as_str()).cloned().unwrap_or_default();
                        self.spawn_op_with_repaint(move |tx| {
                            let (l, w, _) = mgr.bulk_sync_category(&cdir, &list_cl, true);
                            let _ = tx.send(WorkerEvent::CategorySyncDone { slot: slot_str, local: l, web: w });
                        });
                    }
                    if let Some(ref w) = want_item {
                        let is_fav = self.state.favorite_pkgs.contains(&w.asset_package.to_lowercase());
                        let fav_lbl = if is_fav { "★ Pinned" } else { "☆ Pin" };
                        let fav_col = if is_fav { LEMON } else { TEXT_SECONDARY };
                        if rui
                            .add(pill_button(fav_lbl, fav_col))
                            .on_hover_text("Pin or unpin selected donor item to top of list")
                            .clicked()
                        {
                            toggle_fav = Some((w.asset_package.clone(), w.product.clone()));
                        }
                    }
                    rui.add_space(6.0);
                    if minimal_switch(rui, &mut self.cosmetic_swapped_only, "Swapped Only") {
                        filters_changed = true;
                    }
                    if !self.want_search.is_empty()
                        || !self.have_search.is_empty()
                        || self.body_hitbox_filter.is_some()
                        || self.cosmetic_swapped_only
                    {
                        if rui.add(pill_button("Clear Filters", TEXT_MUTED)).clicked() {
                            self.want_search.clear();
                            self.have_search.clear();
                            self.body_hitbox_filter = None;
                            self.cosmetic_swapped_only = false;
                            filters_changed = true;
                        }
                    }
                });
                if let Some((pkg, prod)) = toggle_fav {
                    self.toggle_favorite_pkg(&pkg, &prod);
                } else if filters_changed {
                    self.sync_cosmetic_caches();
                }
            });

            ui.add_space(10.0);

            // Split Pickers Without Any Per-Frame Catalog Cloning!
            let cur_slot = self.current_slot();
            let is_body_now = cur_slot == "Body";
            let list_ref = self.items_by_slot.get(cur_slot);
            let mut new_want = None;
            let mut new_have = None;
            let cooked_dir_str = self.cooked_dir.as_str();
            let icon_job_tx = &self.icon_job_tx;
            let gpu_icons = &self.gpu_icons;
            let pending_icons = &mut self.pending_icons;
            let active_swaps = &self.state.active_swaps;
            let fav_pkgs = &self.state.favorite_pkgs;
            let sel_w = self.selected_want_idx;
            let sel_h = self.selected_have_idx;

            ui.columns(2, |cols| {
                // Donor Column
                cols[0].allocate_ui_with_layout(Vec2::new(cols[0].available_width(), 30.0), Layout::left_to_right(Align::Center), |sui| {
                    search_line(sui, &mut self.want_search, &format!("Search {} donor items...", self.cached_want_idxs.len()));
                });
                cols[0].add_space(6.0);
                let ah0 = cols[0].available_height().max(200.0);
                let want_h = (ah0 - 18.0).max(140.0);
                list_frame().show(&mut cols[0], |ui| {
                    if self.cached_want_idxs.is_empty() {
                        ui.set_min_size(Vec2::new(ui.available_width(), want_h));
                        paint_empty_preview(ui, Vec2::new(ui.available_width(), want_h), "No Matching Items", "Adjust search or hitbox filter");
                    } else {
                        egui::ScrollArea::vertical()
                            .id_salt("min_want_scroll")
                            .auto_shrink([false, false])
                            .max_height(want_h)
                            .show_rows(ui, 36.0, self.cached_want_idxs.len(), |ui, range| {
                                let Some(list) = list_ref else { return };
                                for row_i in range {
                                    let Some(&raw_idx) = self.cached_want_idxs.get(row_i) else { continue };
                                    let Some(it) = list.get(raw_idx) else { continue };
                                    let is_fav = fav_pkgs.contains(&it.asset_package.to_lowercase());
                                    let fk = Self::queue_item_icon(icon_job_tx, cooked_dir_str, gpu_icons, pending_icons, it, None, 40);
                                    let tid = gpu_icons.get(&fk).map(|t| t.id());
                                    let hb = if is_fav && !is_body_now {
                                        Some(("★", LEMON))
                                    } else if is_body_now {
                                        Some((body_hitbox_for_item(&it.product, &it.asset_package).label(), if is_fav { LEMON } else { TEXT_MUTED }))
                                    } else {
                                        None
                                    };
                                    let disp_title = if is_fav {
                                        format!("★ {}", it.product)
                                    } else {
                                        it.product.clone()
                                    };
                                    if minimal_row(ui, sel_w == row_i, 34.0, tid, 26.0, &disp_title, hb) {
                                        new_want = Some(row_i);
                                    }
                                }
                            });
                    }
                });

                // Target Column
                cols[1].allocate_ui_with_layout(Vec2::new(cols[1].available_width(), 30.0), Layout::left_to_right(Align::Center), |sui| {
                    search_line(sui, &mut self.have_search, &format!("Search {} owned targets...", self.cached_have_idxs.len()));
                });
                cols[1].add_space(6.0);
                let ah1 = cols[1].available_height().max(200.0);
                let have_h = (ah1 - 18.0).max(140.0);
                list_frame().show(&mut cols[1], |ui| {
                    if self.cached_have_idxs.is_empty() {
                        ui.set_min_size(Vec2::new(ui.available_width(), have_h));
                        paint_empty_preview(ui, Vec2::new(ui.available_width(), have_h), "No Matching Targets", "Adjust search, hitbox, or Swapped Only");
                    } else {
                        egui::ScrollArea::vertical()
                            .id_salt("min_have_scroll")
                            .auto_shrink([false, false])
                            .max_height(have_h)
                            .show_rows(ui, 36.0, self.cached_have_idxs.len(), |ui, range| {
                                let Some(list) = list_ref else { return };
                                for row_i in range {
                                    let Some(&raw_idx) = self.cached_have_idxs.get(row_i) else { continue };
                                    let Some(it) = list.get(raw_idx) else { continue };
                                    let is_fav = fav_pkgs.contains(&it.asset_package.to_lowercase());
                                    let swapped = active_swaps.contains_key(&it.asset_package.to_lowercase())
                                        || bak_path_for(&Path::new(cooked_dir_str).join(&it.asset_package)).exists();
                                    let fk = Self::queue_item_icon(icon_job_tx, cooked_dir_str, gpu_icons, pending_icons, it, None, 40);
                                    let tid = gpu_icons.get(&fk).map(|t| t.id());
                                    let tag = if swapped {
                                        Some(("Swapped", MINT))
                                    } else if is_body_now {
                                        Some((body_hitbox_for_item(&it.product, &it.asset_package).label(), if is_fav { LEMON } else { TEXT_MUTED }))
                                    } else if is_fav {
                                        Some(("★", LEMON))
                                    } else {
                                        None
                                    };
                                    let disp_title = if is_fav {
                                        format!("★ {}", it.product)
                                    } else {
                                        it.product.clone()
                                    };
                                    if minimal_row(ui, sel_h == row_i, 34.0, tid, 26.0, &disp_title, tag) {
                                        new_have = Some(row_i);
                                    }
                                }
                            });
                    }
                });
            });

            if let Some(w) = new_want {
                self.selected_want_idx = w;
            }
            if let Some(h) = new_have {
                self.selected_have_idx = h;
            }
        });
    }

    // ── 2. Custom Decal Studio ───────────────────────────────────────────────
    fn render_custom_decal_studio(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if !self.decal_initialized {
            self.decal_custom_img = Some(generate_decal_template_for_car(self.decal_car_label, Some(Path::new(&self.cooked_dir))));
            self.decal_preview_dirty = true;
            self.decal_initialized = true;
        }
        self.ensure_decal_target_cache();
        if self.decal_target_idx >= self.cached_decal_idxs.len() {
            self.decal_target_idx = 0;
        }
        let target_decal = self
            .cached_decal_idxs
            .get(self.decal_target_idx)
            .and_then(|&i| self.items_by_slot.get("Decal").and_then(|v| v.get(i)))
            .cloned();
        let has_img = self.decal_custom_img.is_some() || self.decal_mask_img.is_some();

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.vertical(|vui| {
                vui.spacing_mut().item_spacing.y = 1.0;
                vui.label(RichText::new("Decal Studio").font(FontId::proportional(21.0)).color(TEXT_PRIMARY).strong());
                vui.label(RichText::new("AlphaConsole .ZIP / .JSON / .PNG · 1:1 UV injection").font(FontId::proportional(11.0)).color(TEXT_MUTED));
            });
            ui.add_space(20.0);
            meta_col(ui, "UV Layout", self.decal_car_label, TEXT_PRIMARY);
            ui.add_space(16.0);
            if let Some(ref td) = target_decal {
                meta_col(ui, "Target Slot", &td.product, TEXT_SECONDARY);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                let cta_lbl = if self.is_busy { "Applying..." } else { "Apply Decal" };
                if white_cta_button(rui, !self.is_busy && target_decal.is_some() && has_img, cta_lbl, 112.0) {
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
                            .and_then(|sid| find_item_by_id_in_slots(&self.items_by_slot, sid))
                            .map(|it| it.asset_package);
                        save_custom_decal_to_library(&active_pack, self.decal_car_label);
                        self.is_busy = true;
                        let cdir = PathBuf::from(&self.cooked_dir);
                        self.spawn_op_with_repaint(move |tx| {
                            match inject_alphaconsole_decal_at(&cdir, &dec.asset_package, Some(&dec.asset_path), &active_pack, pref_donor.as_deref()) {
                                Ok(msg) => {
                                    let rec = ActiveSwapRecord {
                                        target_pkg: dec.asset_package.clone(),
                                        donor_desc: active_pack.name.clone(),
                                        target_desc: dec.product.clone(),
                                        kind: "DECAL".into(),
                                        slot: "Decal".into(),
                                        donor_pkg: String::new(),
                                        donor_asset_path: String::new(),
                                        target_asset_path: dec.asset_path.clone(),
                                        paint_hex: None,
                                        paint_name: None,
                                    };
                                    let studio_ref = ActiveCustomDecalRef {
                                        name: active_pack.name,
                                        target_pkg: dec.asset_package,
                                        target_desc: dec.product,
                                        target_asset_path: dec.asset_path,
                                        preferred_donor_pkg: pref_donor,
                                        skin_file: active_pack.skin_img.as_ref().and_then(|im| save_studio_png_file("decal_skin.png", im, 1024)),
                                        diffuse_base_file: active_pack.diffuse_base_img.as_ref().and_then(|im| save_studio_png_file("decal_base.png", im, 1024)),
                                        mask_file: active_pack.mask_img.as_ref().and_then(|im| save_studio_png_file("decal_mask.png", im, 1024)),
                                    };
                                    let mut sd = StudioStateDelta::default();
                                    sd.decal = Some(studio_ref);
                                    let _ = tx.send(WorkerEvent::OperationDone {
                                        success: true,
                                        message: msg,
                                        swapped_pkgs: vec![rec],
                                        cleared_pkgs: vec![],
                                        studio_delta: Some(sd),
                                    });
                                }
                                Err(e) => {
                                    let _ = tx.send(WorkerEvent::OperationDone {
                                        success: false,
                                        message: e,
                                        swapped_pkgs: vec![],
                                        cleared_pkgs: vec![],
                                        studio_delta: None,
                                    });
                                }
                            }
                        });
                    }
                }
                let mut restore_decal: Option<(String, String)> = None;
                if let Some(ref td) = target_decal {
                    if self.is_pkg_swapped(&td.asset_package) && rui.add(pill_button("Reset", CORAL)).clicked() {
                        restore_decal = Some((td.asset_package.clone(), td.product.clone()));
                    }
                }
                if let Some((pkg, prod)) = restore_decal {
                    self.restore_target_pkg(&pkg, &prod);
                }
            });
        });
        ui.add_space(10.0);

        self.render_studio_tabs(ui);

        surface_frame().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new("Car").font(FontId::proportional(11.5)).color(TEXT_MUTED));
                let mut next_car = None;
                egui::ComboBox::from_id_salt("decal_car_dd")
                    .selected_text(RichText::new(self.decal_car_label).color(TEXT_PRIMARY))
                    .width(96.0)
                    .show_ui(ui, |ui| {
                        for label in ["Octane", "Fennec", "Dominus"] {
                            if ui.selectable_label(self.decal_car_label == label, label).clicked() {
                                next_car = Some(label);
                            }
                        }
                    });
                if let Some(car) = next_car {
                    self.decal_car_label = car;
                    self.decal_target_idx = 0;
                    self.cache_decal_key = None;
                    self.ensure_decal_target_cache();
                    if self.decal_custom_path.ends_with("UV") {
                        self.decal_custom_img = Some(generate_decal_template_for_car(car, Some(Path::new(&self.cooked_dir))));
                        self.decal_custom_path = format!("{} UV", car);
                    }
                    self.decal_preview_dirty = true;
                }

                if ui.add(pill_button("Import File", TEXT_PRIMARY)).clicked() {
                    if let Some(p) = rfd::FileDialog::new()
                        .add_filter("AlphaConsole Decal or Image", &["zip", "json", "png", "jpg", "jpeg", "webp"])
                        .pick_file()
                    {
                        self.import_decal_file(&p);
                    }
                }
                if ui.add(pill_button("Import Folder", TEXT_SECONDARY)).clicked() {
                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        self.import_decal_file(&dir);
                    }
                }
                if ui.add(pill_button("Export UV", TEXT_SECONDARY)).clicked() {
                    let tpl = generate_decal_template_for_car(self.decal_car_label, Some(Path::new(&self.cooked_dir)));
                    if let Some(sp) = rfd::FileDialog::new().set_file_name(&format!("{}_UV.png", self.decal_car_label)).save_file() {
                        let _ = tpl.save(&sp);
                        self.status_text = format!("Saved {} UV", self.decal_car_label);
                    }
                }
                if ui.add(pill_button("Reset Guide", TEXT_MUTED)).clicked() {
                    self.ac_decal_packs.clear();
                    self.decal_mask_img = None;
                    self.decal_custom_img = Some(generate_decal_template_for_car(self.decal_car_label, Some(Path::new(&self.cooked_dir))));
                    self.decal_custom_path = format!("{} UV", self.decal_car_label);
                    self.decal_preview_dirty = true;
                }

                let saved_decals = list_saved_custom_decals();
                if !saved_decals.is_empty() {
                    let mut picked_saved = None;
                    egui::ComboBox::from_id_salt("saved_dec_lib_dd")
                        .selected_text(RichText::new("Saved Decals").color(TEXT_SECONDARY))
                        .width(128.0)
                        .show_ui(ui, |ui| {
                            for (nm, dir) in &saved_decals {
                                if ui.selectable_label(false, nm).clicked() {
                                    picked_saved = Some(dir.clone());
                                }
                            }
                        });
                    if let Some(dir) = picked_saved {
                        self.import_decal_file(&dir);
                    }
                }

                if self.ac_decal_packs.len() > 1 {
                    let cur_nm = self.ac_decal_packs[self.ac_decal_idx].name.clone();
                    let mut picked = None;
                    egui::ComboBox::from_id_salt("ac_dec_preset")
                        .selected_text(cur_nm)
                        .width(150.0)
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

                ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                    if rui
                        .color_edit_button_srgb(&mut self.decal_accent_rgb)
                        .on_hover_text("2D UV preview trim/accent color")
                        .changed()
                    {
                        self.decal_preview_dirty = true;
                    }
                    rui.label(RichText::new("Accent").font(FontId::proportional(10.5)).color(TEXT_MUTED));
                    if rui
                        .color_edit_button_srgb(&mut self.decal_primary_rgb)
                        .on_hover_text("2D UV preview primary team color")
                        .changed()
                    {
                        self.decal_preview_dirty = true;
                    }
                    rui.label(RichText::new("Primary").font(FontId::proportional(10.5)).color(TEXT_MUTED));
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
                    280,
                );
                self.decal_preview_tex = Some(ctx.load_texture("decal_2d_vp", rgba_to_egui_image(&rendered), TextureOptions::LINEAR));
                self.decal_preview_dirty = false;
            }

            let mut next_target = None;
            let sel_idx = self.decal_target_idx;

            ui.columns(2, |cols| {
                let prev_dim = (cols[0].available_width() - 32.0).min(cols[0].available_height() - 42.0).clamp(200.0, 320.0);
                cols[0].vertical_centered(|ui| {
                    ui.allocate_ui(Vec2::splat(prev_dim + 16.0), |fui| {
                        well_frame().show(fui, |wui| {
                            if let Some(ref tex) = self.decal_preview_tex {
                                wui.image((tex.id(), Vec2::splat(prev_dim)));
                            }
                        });
                    });
                    ui.add_space(6.0);
                    ui.add(egui::Label::new(RichText::new(&self.decal_custom_path).font(FontId::proportional(11.5)).color(TEXT_SECONDARY)).truncate());
                });

                cols[1].allocate_ui_with_layout(Vec2::new(cols[1].available_width(), 30.0), Layout::left_to_right(Align::Center), |ui| {
                    let sw = (ui.available_width() - 96.0).max(120.0);
                    ui.allocate_ui(Vec2::new(sw, 28.0), |sui| {
                        search_line(sui, &mut self.decal_target_search, "Filter target decal...");
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                        if minimal_switch(rui, &mut self.decal_filter_car_only, "Car Only") {
                            self.decal_target_idx = 0;
                            self.cache_decal_key = None;
                        }
                    });
                });
                self.ensure_decal_target_cache();
                if self.decal_target_idx >= self.cached_decal_idxs.len() {
                    self.decal_target_idx = 0;
                }
                let list_ref = self.items_by_slot.get("Decal");
                let active_swaps = &self.state.active_swaps;
                let cooked_dir_str = self.cooked_dir.as_str();
                cols[1].add_space(6.0);
                let ah = cols[1].available_height().max(200.0);
                list_frame().show(&mut cols[1], |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("min_decal_list")
                        .auto_shrink([false, false])
                        .max_height((ah - 18.0).max(140.0))
                        .show_rows(ui, 30.0, self.cached_decal_idxs.len(), |ui, range| {
                            let Some(list) = list_ref else { return };
                            for i in range {
                                let Some(&raw_i) = self.cached_decal_idxs.get(i) else { continue };
                                let Some(d) = list.get(raw_i) else { continue };
                                let swapped = active_swaps.contains_key(&d.asset_package.to_lowercase())
                                    || bak_path_for(&Path::new(cooked_dir_str).join(&d.asset_package)).exists();
                                let tag = if swapped { Some(("Custom", MINT)) } else { None };
                                if minimal_row(ui, sel_idx == i, 28.0, None, 0.0, &d.product, tag) {
                                    next_target = Some((i, d.product.clone(), d.asset_package.clone()));
                                }
                            }
                        });
                });
            });

            if let Some((idx, prod, pkg)) = next_target {
                self.decal_target_idx = idx;
                if let Some(car) = detect_car_uv_label(&format!("{} {}", prod, pkg)) {
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
        });
    }

    // ── 3. Custom PFP Studio ─────────────────────────────────────────────────
    fn render_custom_pfp_studio(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let q_low = self.pfp_search.to_lowercase();
        if self.cache_pfp_q.as_deref() != Some(&q_low) {
            self.cached_pfp_idxs = self
                .avatar_borders
                .iter()
                .enumerate()
                .filter(|(_, (lbl, pkg))| q_low.is_empty() || lbl.to_lowercase().contains(&q_low) || pkg.to_lowercase().contains(&q_low))
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

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.vertical(|vui| {
                vui.spacing_mut().item_spacing.y = 1.0;
                vui.label(RichText::new("Avatar PFP").font(FontId::proportional(21.0)).color(TEXT_PRIMARY).strong());
                vui.label(RichText::new(format!("{} borders · Live cutout compositor", self.avatar_borders.len())).font(FontId::proportional(11.0)).color(TEXT_MUTED));
            });
            ui.add_space(20.0);
            meta_col(ui, "Image", if self.pfp_custom_path.is_empty() { "None" } else { &self.pfp_custom_path }, TEXT_SECONDARY);
            ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                let can_apply = !self.is_busy && selected_border.is_some() && self.pfp_custom_img.is_some();
                if white_cta_button(rui, can_apply, "Apply PFP", 108.0) {
                    if let (Some((lbl, pkg)), Some(img)) = (selected_border.clone(), self.pfp_custom_img.clone()) {
                        let _ = save_custom_image_to_library("PFP", &lbl, &img);
                        self.is_busy = true;
                        let cdir = PathBuf::from(&self.cooked_dir);
                        self.spawn_op_with_repaint(move |tx| match inject_custom_pfp(&cdir, &pkg, &img) {
                            Ok(msg) => {
                                let rec = ActiveSwapRecord {
                                    target_pkg: pkg.clone(),
                                    donor_desc: "Custom PFP".into(),
                                    target_desc: lbl.clone(),
                                    kind: "PFP".into(),
                                    slot: "Avatar Border".into(),
                                    donor_pkg: String::new(),
                                    donor_asset_path: String::new(),
                                    target_asset_path: String::new(),
                                    paint_hex: None,
                                    paint_name: None,
                                };
                                let mut sd = StudioStateDelta::default();
                                if let Some(f) = save_studio_png_file("pfp.png", &img, 512) {
                                    sd.pfp = Some(ActiveCustomPfpRef {
                                        border_pkg: pkg,
                                        border_desc: lbl,
                                        image_file: f,
                                    });
                                }
                                let _ = tx.send(WorkerEvent::OperationDone {
                                    success: true,
                                    message: msg,
                                    swapped_pkgs: vec![rec],
                                    cleared_pkgs: vec![],
                                    studio_delta: Some(sd),
                                });
                            }
                            Err(e) => {
                                let _ = tx.send(WorkerEvent::OperationDone {
                                    success: false,
                                    message: e,
                                    swapped_pkgs: vec![],
                                    cleared_pkgs: vec![],
                                    studio_delta: None,
                                });
                            }
                        });
                    }
                }
                let mut restore_pfp: Option<(String, String)> = None;
                if let Some((ref lbl, ref pkg)) = selected_border {
                    if self.is_pkg_swapped(pkg) && rui.add(pill_button("Reset", CORAL)).clicked() {
                        restore_pfp = Some((pkg.clone(), lbl.clone()));
                    }
                }
                if let Some((pkg, lbl)) = restore_pfp {
                    self.restore_target_pkg(&pkg, &lbl);
                }
            });
        });
        ui.add_space(10.0);

        self.render_studio_tabs(ui);

        if self.pfp_preview_dirty || self.pfp_preview_tex.is_none() {
            let composited = selected_border
                .as_ref()
                .and_then(|(_, pkg)| compose_pfp_with_border_preview(Path::new(&self.cooked_dir), pkg, self.pfp_custom_img.as_ref()))
                .or_else(|| self.pfp_custom_img.clone());
            if let Some(ref c_im) = composited {
                let scaled = image::DynamicImage::ImageRgba8(c_im.clone()).resize_exact(216, 216, FilterType::Triangle).to_rgba8();
                self.pfp_preview_tex = Some(ctx.load_texture("pfp_prev", rgba_to_egui_image(&scaled), TextureOptions::LINEAR));
            } else {
                self.pfp_preview_tex = None;
            }
            self.pfp_composited_img = composited;
            self.pfp_preview_dirty = false;
            self.banner_preview_dirty = true;
        }

        surface_frame().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                if ui.add(pill_button("Load Image", TEXT_PRIMARY)).clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Image", &["png", "jpg", "jpeg", "webp"]).pick_file() {
                        if let Ok(im) = image::open(&p) {
                            let fname = p.file_name().unwrap_or_default().to_string_lossy().to_string();
                            self.open_image_cropper(ctx, CropTarget::Pfp, Some(p), im.to_rgba8(), fname);
                        }
                    }
                }
                let can_crop_pfp = self.pfp_custom_img.is_some() || self.pfp_source_path.is_some();
                if ui.add_enabled(can_crop_pfp, pill_button("Cut / Crop", TEXT_SECONDARY)).clicked() {
                    let reloaded = self
                        .pfp_source_path
                        .as_ref()
                        .and_then(|p| image::open(p).ok().map(|im| im.to_rgba8()));
                    if let Some(src) = reloaded.or_else(|| self.pfp_custom_img.clone()) {
                        let nm = self
                            .pfp_source_path
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| {
                                if self.pfp_custom_path.is_empty() { "Custom PFP".into() } else { self.pfp_custom_path.clone() }
                            });
                        self.open_image_cropper(ctx, CropTarget::Pfp, self.pfp_source_path.clone(), src, nm);
                    }
                }
                if self.pfp_custom_img.is_some() && ui.add(pill_button("Clear", TEXT_MUTED)).clicked() {
                    self.pfp_custom_img = None;
                    self.pfp_composited_img = None;
                    self.pfp_custom_path.clear();
                    self.pfp_source_path = None;
                    self.pfp_preview_dirty = true;
                    self.banner_preview_dirty = true;
                }
                let saved_pfps = list_saved_custom_images("PFP");
                if !saved_pfps.is_empty() {
                    let mut picked_pfp: Option<(String, PathBuf)> = None;
                    egui::ComboBox::from_id_salt("saved_pfp_lib_dd")
                        .selected_text(RichText::new("Saved PFPs").color(TEXT_SECONDARY))
                        .width(118.0)
                        .show_ui(ui, |ui| {
                            for (nm, p) in &saved_pfps {
                                if ui.selectable_label(false, nm).clicked() {
                                    picked_pfp = Some((nm.clone(), p.clone()));
                                }
                            }
                        });
                    if let Some((nm, p)) = picked_pfp {
                        if let Ok(im) = image::open(&p) {
                            self.pfp_custom_img = Some(im.to_rgba8());
                            self.pfp_custom_path = nm;
                            self.pfp_source_path = Some(p);
                            self.pfp_preview_dirty = true;
                            self.banner_preview_dirty = true;
                        }
                    }
                }
                ui.label(
                    RichText::new(if self.pfp_custom_path.is_empty() { "PNG / JPG / WEBP · 1:1 Cut" } else { &self.pfp_custom_path })
                        .font(FontId::proportional(11.5))
                        .color(TEXT_MUTED),
                );
            });
            ui.add_space(10.0);

            let mut picked_border = None;
            let sel_b = self.pfp_border_idx;
            let borders = &self.avatar_borders;
            let cached = &self.cached_pfp_idxs;
            let active_swaps = &self.state.active_swaps;
            let cooked_dir_str = self.cooked_dir.as_str();

            ui.columns(2, |cols| {
                let lh = cols[0].available_height().max(260.0);
                cols[0].vertical_centered(|ui| {
                    ui.add_space(((lh - 290.0) * 0.35).max(16.0));
                    ui.allocate_ui(Vec2::splat(232.0), |fui| {
                        well_frame().show(fui, |wui| {
                            if let Some(ref tex) = self.pfp_preview_tex {
                                wui.image((tex.id(), Vec2::splat(216.0)));
                            } else {
                                paint_empty_preview(wui, Vec2::splat(216.0), "No Avatar", "Click Load Image");
                            }
                        });
                    });
                    ui.add_space(10.0);
                    if let Some((ref lbl, _)) = selected_border {
                        ui.add(egui::Label::new(RichText::new(lbl).font(FontId::proportional(13.0)).color(TEXT_PRIMARY).strong()).truncate());
                    }
                });

                cols[1].allocate_ui_with_layout(Vec2::new(cols[1].available_width(), 30.0), Layout::left_to_right(Align::Center), |ui| {
                    search_line(ui, &mut self.pfp_search, &format!("Search {} borders...", cached.len()));
                });
                cols[1].add_space(6.0);
                let ah = cols[1].available_height().max(200.0);
                list_frame().show(&mut cols[1], |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("min_pfp_list")
                        .auto_shrink([false, false])
                        .max_height((ah - 18.0).max(140.0))
                        .show_rows(ui, 30.0, cached.len(), |ui, range| {
                            for i in range {
                                let Some(&raw_i) = cached.get(i) else { continue };
                                let Some((lbl, pkg)) = borders.get(raw_i) else { continue };
                                let swapped = active_swaps.contains_key(&pkg.to_lowercase())
                                    || bak_path_for(&Path::new(cooked_dir_str).join(pkg)).exists();
                                let tag = if swapped { Some(("Custom", MINT)) } else { None };
                                if minimal_row(ui, sel_b == i, 28.0, None, 0.0, lbl, tag) {
                                    picked_border = Some(i);
                                }
                            }
                        });
                });
            });

            if let Some(i) = picked_border {
                self.pfp_border_idx = i;
                self.pfp_preview_dirty = true;
            }
        });
    }

    // ── 4. Custom Banner Studio ──────────────────────────────────────────────
    fn render_custom_banner_studio(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if !self.banner_initialized {
            self.banner_custom_img = Some(generate_banner_template());
            self.banner_preview_dirty = true;
            self.banner_initialized = true;
        }
        let q_low = self.banner_search.to_lowercase();
        if self.cache_banner_q.as_deref() != Some(&q_low) {
            self.cached_banner_idxs = self
                .player_banners
                .iter()
                .enumerate()
                .filter(|(_, (lbl, pkg))| q_low.is_empty() || lbl.to_lowercase().contains(&q_low) || pkg.to_lowercase().contains(&q_low))
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

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.vertical(|vui| {
                vui.spacing_mut().item_spacing.y = 1.0;
                vui.label(RichText::new("Banner Studio").font(FontId::proportional(21.0)).color(TEXT_PRIMARY).strong());
                vui.label(RichText::new(format!("{} banners · 840×200 (14:3) compositor", self.player_banners.len())).font(FontId::proportional(11.0)).color(TEXT_MUTED));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                let can_apply = !self.is_busy && selected_banner.is_some() && self.banner_custom_img.is_some();
                if white_cta_button(rui, can_apply, "Apply Banner", 118.0) {
                    if let (Some((lbl, pkg)), Some(img)) = (selected_banner.clone(), self.banner_custom_img.clone()) {
                        let _ = save_custom_image_to_library("Banners", &lbl, &img);
                        let bake_flag = self.banner_bake_name;
                        let player_nm = self.banner_player_name.clone();
                        let final_img = if bake_flag {
                            bake_name_onto_banner(&img, &player_nm)
                        } else {
                            img.clone()
                        };
                        self.is_busy = true;
                        let cdir = PathBuf::from(&self.cooked_dir);
                        self.spawn_op_with_repaint(move |tx| match inject_custom_banner(&cdir, &pkg, &final_img) {
                            Ok(msg) => {
                                let rec = ActiveSwapRecord {
                                    target_pkg: pkg.clone(),
                                    donor_desc: "Custom Banner".into(),
                                    target_desc: lbl.clone(),
                                    kind: "BANNER".into(),
                                    slot: "Player Banner".into(),
                                    donor_pkg: String::new(),
                                    donor_asset_path: String::new(),
                                    target_asset_path: String::new(),
                                    paint_hex: None,
                                    paint_name: None,
                                };
                                let mut sd = StudioStateDelta::default();
                                if let Some(f) = save_studio_png_file("banner.png", &img, 840) {
                                    sd.banner = Some(ActiveCustomBannerRef {
                                        banner_pkg: pkg,
                                        banner_desc: lbl,
                                        image_file: f,
                                        player_name: player_nm,
                                        bake_name: bake_flag,
                                    });
                                }
                                let _ = tx.send(WorkerEvent::OperationDone {
                                    success: true,
                                    message: msg,
                                    swapped_pkgs: vec![rec],
                                    cleared_pkgs: vec![],
                                    studio_delta: Some(sd),
                                });
                            }
                            Err(e) => {
                                let _ = tx.send(WorkerEvent::OperationDone {
                                    success: false,
                                    message: e,
                                    swapped_pkgs: vec![],
                                    cleared_pkgs: vec![],
                                    studio_delta: None,
                                });
                            }
                        });
                    }
                }
                let mut restore_ban: Option<(String, String)> = None;
                if let Some((ref lbl, ref pkg)) = selected_banner {
                    if self.is_pkg_swapped(pkg) && rui.add(pill_button("Reset", CORAL)).clicked() {
                        restore_ban = Some((pkg.clone(), lbl.clone()));
                    }
                }
                if let Some((pkg, lbl)) = restore_ban {
                    self.restore_target_pkg(&pkg, &lbl);
                }
            });
        });
        ui.add_space(10.0);

        self.render_studio_tabs(ui);

        surface_frame().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                if ui.add(pill_button("Load Image", TEXT_PRIMARY)).clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Image", &["png", "jpg", "jpeg", "webp"]).pick_file() {
                        if let Ok(im) = image::open(&p) {
                            let fname = p.file_name().unwrap_or_default().to_string_lossy().to_string();
                            self.open_image_cropper(ctx, CropTarget::Banner, Some(p), im.to_rgba8(), fname);
                        }
                    }
                }
                let can_crop_ban = self.banner_custom_img.is_some() || self.banner_source_path.is_some();
                if ui.add_enabled(can_crop_ban, pill_button("Cut / Crop", TEXT_SECONDARY)).clicked() {
                    let reloaded = self
                        .banner_source_path
                        .as_ref()
                        .and_then(|p| image::open(p).ok().map(|im| im.to_rgba8()));
                    if let Some(src) = reloaded.or_else(|| self.banner_custom_img.clone()) {
                        let nm = self
                            .banner_source_path
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| self.banner_custom_path.clone());
                        self.open_image_cropper(ctx, CropTarget::Banner, self.banner_source_path.clone(), src, nm);
                    }
                }
                if ui.add(pill_button("Export 14:3", TEXT_SECONDARY)).clicked() {
                    let tpl = generate_banner_template();
                    if let Some(sp) = rfd::FileDialog::new().set_file_name("Banner_840x200.png").save_file() {
                        let _ = tpl.save(&sp);
                    }
                }
                if ui.add(pill_button("Template", TEXT_MUTED)).clicked() {
                    self.banner_custom_img = Some(generate_banner_template());
                    self.banner_custom_path = "Banner Template".into();
                    self.banner_source_path = None;
                    self.banner_preview_dirty = true;
                }
                let saved_banners = list_saved_custom_images("Banners");
                if !saved_banners.is_empty() {
                    let mut picked_ban_img: Option<(String, PathBuf)> = None;
                    egui::ComboBox::from_id_salt("saved_ban_lib_dd")
                        .selected_text(RichText::new("Saved Banners").color(TEXT_SECONDARY))
                        .width(126.0)
                        .show_ui(ui, |ui| {
                            for (nm, p) in &saved_banners {
                                if ui.selectable_label(false, nm).clicked() {
                                    picked_ban_img = Some((nm.clone(), p.clone()));
                                }
                            }
                        });
                    if let Some((nm, p)) = picked_ban_img {
                        if let Ok(im) = image::open(&p) {
                            self.banner_custom_img = Some(im.to_rgba8());
                            self.banner_custom_path = nm;
                            self.banner_source_path = Some(p);
                            self.banner_preview_dirty = true;
                        }
                    }
                }
                ui.add_space(6.0);
                ui.allocate_ui_with_layout(Vec2::new(165.0, 28.0), Layout::left_to_right(Align::Center), |nui| {
                    if minimal_text_box(nui, &mut self.banner_player_name, "Custom player name...") {
                        self.banner_preview_dirty = true;
                    }
                });
                minimal_switch(ui, &mut self.banner_bake_name, "Bake Name");
            });
            ui.add_space(10.0);

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

            let mut picked_ban = None;
            let sel_ban = self.banner_target_idx;
            let banners = &self.player_banners;
            let cached = &self.cached_banner_idxs;
            let active_swaps = &self.state.active_swaps;
            let cooked_dir_str = self.cooked_dir.as_str();

            ui.columns(2, |cols| {
                let lh = cols[0].available_height().max(260.0);
                cols[0].vertical_centered(|ui| {
                    ui.add_space(((lh - 230.0) * 0.38).max(16.0));
                    well_frame().show(ui, |wui| {
                        if let Some(ref tex) = self.banner_preview_tex {
                            let w = wui.available_width().min(440.0);
                            wui.image((tex.id(), Vec2::new(w, w * 168.0 / 540.0)));
                        }
                    });
                    ui.add_space(10.0);
                    ui.add(egui::Label::new(RichText::new(&self.banner_custom_path).font(FontId::proportional(13.0)).color(TEXT_PRIMARY).strong()).truncate());
                    let t_lbl = selected_banner.as_ref().map(|(l, _)| l.as_str()).unwrap_or("No Slot Selected");
                    ui.label(RichText::new(format!("Target: {}", t_lbl)).font(FontId::proportional(11.0)).color(TEXT_MUTED));
                });

                cols[1].allocate_ui_with_layout(Vec2::new(cols[1].available_width(), 30.0), Layout::left_to_right(Align::Center), |ui| {
                    search_line(ui, &mut self.banner_search, &format!("Search {} banners...", cached.len()));
                });
                cols[1].add_space(6.0);
                let ah = cols[1].available_height().max(200.0);
                list_frame().show(&mut cols[1], |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("min_banner_list")
                        .auto_shrink([false, false])
                        .max_height((ah - 18.0).max(140.0))
                        .show_rows(ui, 30.0, cached.len(), |ui, range| {
                            for i in range {
                                let Some(&raw_i) = cached.get(i) else { continue };
                                let Some((lbl, pkg)) = banners.get(raw_i) else { continue };
                                let swapped = active_swaps.contains_key(&pkg.to_lowercase())
                                    || bak_path_for(&Path::new(cooked_dir_str).join(pkg)).exists();
                                let tag = if swapped { Some(("Custom", MINT)) } else { None };
                                if minimal_row(ui, sel_ban == i, 28.0, None, 0.0, lbl, tag) {
                                    picked_ban = Some(i);
                                }
                            }
                        });
                });
            });

            if let Some(i) = picked_ban {
                self.banner_target_idx = i;
            }
        });
    }

    // ── 5. Custom Match Ball Studio ──────────────────────────────────────────
    fn render_custom_ball_studio(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if !self.ball_initialized {
            let cooked = Path::new(&self.cooked_dir);
            self.ball_custom_img = Some(extract_ball_texture_preview(cooked).unwrap_or_else(|| generate_ball_template(None)));
            self.ball_orb_dirty = true;
            self.ball_initialized = true;
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.vertical(|vui| {
                vui.spacing_mut().item_spacing.y = 1.0;
                vui.label(RichText::new("Match Ball").font(FontId::proportional(21.0)).color(TEXT_PRIMARY).strong());
                vui.label(RichText::new("GameInfo_Soccar_SF.upk · AlphaConsole diffuse, normal & masks").font(FontId::proportional(11.0)).color(TEXT_MUTED));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                let cta_lbl = if self.is_busy { "Applying..." } else { "Apply Ball" };
                if white_cta_button(rui, !self.is_busy && self.ball_custom_img.is_some(), cta_lbl, 108.0) {
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
                    save_custom_ball_to_library(&pack);
                    self.is_busy = true;
                    let cdir = PathBuf::from(&self.cooked_dir);
                    self.spawn_op_with_repaint(move |tx| match inject_alphaconsole_ball(&cdir, &pack) {
                        Ok(msg) => {
                            let rec = ActiveSwapRecord {
                                target_pkg: "GameInfo_Soccar_SF.upk".into(),
                                donor_desc: pack.name.clone(),
                                target_desc: "Match Ball".into(),
                                kind: "BALL".into(),
                                slot: String::new(),
                                donor_pkg: String::new(),
                                donor_asset_path: String::new(),
                                target_asset_path: String::new(),
                                paint_hex: None,
                                paint_name: None,
                            };
                            let ball_ref = save_studio_png_file("ball_diffuse.png", &pack.diffuse_img, 1024).map(|diff_file| ActiveCustomBallRef {
                                name: if pack.name.trim().is_empty() { "Custom Ball".into() } else { pack.name.clone() },
                                diffuse_file: diff_file,
                                normal_file: pack.normal_img.as_ref().and_then(|im| save_studio_png_file("ball_normal.png", im, 1024)),
                                mask_file: pack.mask_img.as_ref().and_then(|im| save_studio_png_file("ball_mask.png", im, 1024)),
                            });
                            let mut sd = StudioStateDelta::default();
                            sd.ball = ball_ref;
                            let _ = tx.send(WorkerEvent::OperationDone {
                                success: true,
                                message: msg,
                                swapped_pkgs: vec![rec],
                                cleared_pkgs: vec![],
                                studio_delta: Some(sd),
                            });
                        }
                        Err(e) => {
                            let _ = tx.send(WorkerEvent::OperationDone {
                                success: false,
                                message: e,
                                swapped_pkgs: vec![],
                                cleared_pkgs: vec![],
                                studio_delta: None,
                            });
                        }
                    });
                }
                if self.is_pkg_swapped("GameInfo_Soccar_SF.upk") && rui.add(pill_button("Reset", CORAL)).clicked() {
                    self.restore_target_pkg("GameInfo_Soccar_SF.upk", "Match Ball");
                }
            });
        });
        ui.add_space(10.0);

        self.render_studio_tabs(ui);

        if self.ball_orb_dirty {
            if let Some(ref b_im) = self.ball_custom_img {
                let orb = render_2d_ball_orb_preview(b_im, 256, None);
                self.ball_orb_tex = Some(ctx.load_texture("ball_2d_orb", rgba_to_egui_image(&orb), TextureOptions::LINEAR));
                let flat = imageops::resize(b_im, 140, 140, FilterType::Triangle);
                self.ball_flat_tex = Some(ctx.load_texture("ball_flat", rgba_to_egui_image(&flat), TextureOptions::LINEAR));
            }
            self.ball_norm_tex = self.ball_normal_img.as_ref().map(|n_im| {
                let nf = imageops::resize(n_im, 140, 140, FilterType::Triangle);
                ctx.load_texture("ball_norm", rgba_to_egui_image(&nf), TextureOptions::LINEAR)
            });
            self.ball_mask_tex = self.ball_mask_img.as_ref().map(|m_im| {
                let mf = imageops::resize(m_im, 140, 140, FilterType::Triangle);
                ctx.load_texture("ball_mask", rgba_to_egui_image(&mf), TextureOptions::LINEAR)
            });
            self.ball_orb_dirty = false;
        }

        surface_frame().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                if ui.add(pill_button("Import File", TEXT_PRIMARY)).clicked() {
                    if let Some(p) = rfd::FileDialog::new()
                        .add_filter("AlphaConsole Ball or Image", &["zip", "json", "png", "jpg", "jpeg", "webp"])
                        .pick_file()
                    {
                        self.import_ball_file(&p);
                    }
                }
                if ui.add(pill_button("Import Folder", TEXT_SECONDARY)).clicked() {
                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        self.import_ball_file(&dir);
                    }
                }
                if ui.add(pill_button("Export UV", TEXT_SECONDARY)).clicked() {
                    let tpl = generate_ball_template(Some(Path::new(&self.cooked_dir)));
                    if let Some(sp) = rfd::FileDialog::new().set_file_name("Ball_UV.png").save_file() {
                        let _ = tpl.save(&sp);
                    }
                }
                if ui.add(pill_button("Vanilla", TEXT_MUTED)).clicked() {
                    self.ac_ball_packs.clear();
                    self.ball_normal_img = None;
                    self.ball_mask_img = None;
                    self.ball_custom_img = Some(extract_ball_texture_preview(Path::new(&self.cooked_dir)).unwrap_or_else(|| generate_ball_template(None)));
                    self.ball_custom_path = "Vanilla Match Ball".into();
                    self.ball_orb_dirty = true;
                }

                let saved_balls = list_saved_custom_balls();
                if !saved_balls.is_empty() {
                    let mut picked_saved_ball = None;
                    egui::ComboBox::from_id_salt("saved_ball_lib_dd")
                        .selected_text(RichText::new("Saved Balls").color(TEXT_SECONDARY))
                        .width(120.0)
                        .show_ui(ui, |ui| {
                            for (nm, dir) in &saved_balls {
                                if ui.selectable_label(false, nm).clicked() {
                                    picked_saved_ball = Some(dir.clone());
                                }
                            }
                        });
                    if let Some(dir) = picked_saved_ball {
                        self.import_ball_file(&dir);
                    }
                }

                if self.ac_ball_packs.len() > 1 {
                    let cur_nm = self.ac_ball_packs[self.ac_ball_idx].name.clone();
                    let mut picked = None;
                    egui::ComboBox::from_id_salt("ac_ball_preset")
                        .selected_text(cur_nm)
                        .width(180.0)
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

                ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                    if self.ball_mask_img.is_some() {
                        status_badge(rui, "Masks", LEMON, true);
                    }
                    if self.ball_normal_img.is_some() {
                        status_badge(rui, "Normal", MINT, true);
                    }
                    status_badge(rui, "Diffuse", TEXT_SECONDARY, false);
                });
            });
            ui.add_space(12.0);

            let stage_h = ui.available_height().max(260.0);
            ui.columns(2, |cols| {
                cols[0].vertical_centered(|ui| {
                    ui.add_space(((stage_h - 300.0) * 0.30).max(12.0));
                    ui.allocate_ui(Vec2::splat(266.0), |fui| {
                        well_frame().show(fui, |wui| {
                            if let Some(ref tex) = self.ball_orb_tex {
                                wui.image((tex.id(), Vec2::splat(250.0)));
                            }
                        });
                    });
                    ui.add_space(10.0);
                    ui.add(egui::Label::new(RichText::new(&self.ball_custom_path).font(FontId::proportional(13.5)).color(TEXT_PRIMARY).strong()).truncate());
                });

                cols[1].vertical(|ui| {
                    ui.add_space(12.0);
                    ui.label(RichText::new("Channels").font(FontId::proportional(11.0)).color(TEXT_MUTED));
                    ui.add_space(8.0);
                    let has_mask = self.ball_mask_tex.is_some();
                    let col_cnt = if has_mask { 3.0 } else { 2.0 };
                    let col_w = ui.available_width().max(220.0);
                    let tile_w = ((col_w - 12.0 * (col_cnt - 1.0)) / col_cnt).clamp(104.0, 176.0);
                    let img_dim = (tile_w - 20.0).clamp(80.0, 136.0);
                    let tile_h = img_dim + 42.0;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        if let Some(ref ft) = self.ball_flat_tex {
                            ui.allocate_ui(Vec2::new(tile_w, tile_h), |tui| {
                                well_frame().show(tui, |vui| {
                                    vui.set_min_size(Vec2::new((tile_w - 16.0).max(76.0), tile_h - 16.0));
                                    vui.vertical_centered(|vui| {
                                        vui.image((ft.id(), Vec2::splat(img_dim)));
                                        vui.add_space(6.0);
                                        vui.label(RichText::new("Diffuse").font(FontId::proportional(11.5)).color(TEXT_PRIMARY));
                                    });
                                });
                            });
                        }
                        if let Some(ref nt) = self.ball_norm_tex {
                            ui.allocate_ui(Vec2::new(tile_w, tile_h), |tui| {
                                well_frame().show(tui, |vui| {
                                    vui.set_min_size(Vec2::new((tile_w - 16.0).max(76.0), tile_h - 16.0));
                                    vui.vertical_centered(|vui| {
                                        vui.image((nt.id(), Vec2::splat(img_dim)));
                                        vui.add_space(6.0);
                                        vui.label(RichText::new("Normal").font(FontId::proportional(11.5)).color(MINT));
                                    });
                                });
                            });
                        } else {
                            ui.allocate_ui(Vec2::new(tile_w, tile_h), |tui| {
                                well_frame().show(tui, |vui| {
                                    vui.set_min_size(Vec2::new((tile_w - 16.0).max(76.0), tile_h - 16.0));
                                    paint_empty_preview(vui, Vec2::new(img_dim, tile_h - 20.0), "Normal Map", "Optional");
                                });
                            });
                        }
                        if let Some(ref mt) = self.ball_mask_tex {
                            ui.allocate_ui(Vec2::new(tile_w, tile_h), |tui| {
                                well_frame().show(tui, |vui| {
                                    vui.set_min_size(Vec2::new((tile_w - 16.0).max(76.0), tile_h - 16.0));
                                    vui.vertical_centered(|vui| {
                                        vui.image((mt.id(), Vec2::splat(img_dim)));
                                        vui.add_space(6.0);
                                        vui.label(RichText::new("Masks").font(FontId::proportional(11.5)).color(LEMON));
                                    });
                                });
                            });
                        }
                    });
                });
            });
        });
    }

    // ── 6. Workshop Maps ─────────────────────────────────────────────────────
    fn render_workshop_maps_studio(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let underpass_active = self.is_pkg_swapped(WORKSHOP_SAFE_SLOT);
        let mut load_sel: Option<WorkshopMapEntry> = None;

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.vertical(|vui| {
                vui.spacing_mut().item_spacing.y = 1.0;
                vui.label(RichText::new("Workshop").font(FontId::proportional(21.0)).color(TEXT_PRIMARY).strong());
                vui.label(RichText::new("Free Play > Underpass slot · Local library & online catalog").font(FontId::proportional(11.0)).color(TEXT_MUTED));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                if let Some(sel) = self.workshop_library.get(self.workshop_selected_idx).cloned() {
                    if white_cta_button(rui, !self.is_busy, "Load in Underpass", 136.0) {
                        load_sel = Some(sel);
                    }
                }
                if underpass_active && rui.add_enabled(!self.is_busy, pill_button("Restore Underpass", CORAL)).clicked() {
                    if restore_workshop_underpass(Path::new(&self.cooked_dir)) {
                        let k = WORKSHOP_SAFE_SLOT.to_lowercase();
                        self.state.active_swaps.remove(&k);
                        self.state.swap_history.retain(|x| x != &k);
                        self.state.active_studio.workshop_map_id = None;
                        self.workshop_missing_deps.clear();
                        self.bump_swaps_revision();
                        self.save_state();
                        self.refresh_health_report();
                        self.status_is_error = false;
                        self.status_text = "Restored Underpass".into();
                    }
                }
            });
        });
        ui.add_space(10.0);

        if let Some(ref dl) = self.workshop_dl_progress {
            surface_frame().show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                let fmt_mb = |b: u64| -> String { format!("{:.1} MB", (b as f64) / 1_048_576.0) };
                ui.horizontal(|ui| {
                    ui.add(egui::Label::new(RichText::new(format!("{} · {}", dl.name, dl.phase)).font(FontId::proportional(12.2)).color(TEXT_PRIMARY).strong()).truncate());
                    ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                        let stats = if let Some(tot) = dl.total.filter(|&t| t > 0) {
                            let pct = ((dl.downloaded as f64 / tot as f64) * 100.0).min(100.0);
                            format!("{} / {} ({:.0}%)", fmt_mb(dl.downloaded), fmt_mb(tot), pct)
                        } else if dl.downloaded > 0 {
                            fmt_mb(dl.downloaded)
                        } else {
                            dl.phase.clone()
                        };
                        rui.label(RichText::new(stats).font(FontId::monospace(11.0)).color(TEXT_SECONDARY));
                    });
                });
                ui.add_space(6.0);
                let frac = match dl.total {
                    Some(tot) if tot > 0 => ((dl.downloaded as f32) / (tot as f32)).clamp(0.0, 1.0),
                    _ => {
                        let t = ctx.input(|i| i.time) as f32;
                        0.18 + 0.72 * ((t * 0.9).sin() * 0.5 + 0.5)
                    }
                };
                ui.add(egui::ProgressBar::new(frac).fill(WHITE_CTA_BG).desired_height(6.0).rounding(Rounding::same(3.0)).animate(dl.total.is_none()));
            });
            ui.add_space(8.0);
        }

        if self.workshop_online_items.is_empty() && !self.workshop_online_loading && !self.status_is_error {
            self.workshop_online_loading = true;
            self.spawn_op_with_repaint(|tx| {
                let _ = tx.send(WorkerEvent::WorkshopCatalogDone(fetch_all_online_workshop_maps(false)));
            });
        }

        let ws_key = (
            self.workshop_online_search.to_lowercase(),
            self.workshop_direct_only,
            self.workshop_source_filter,
            self.workshop_sort_mode,
            self.workshop_online_items.len(),
        );
        if self.cache_ws_key != ws_key {
            self.cached_ws_idxs = filter_sort_workshop_indices(
                &self.workshop_online_items,
                &ws_key.0,
                ws_key.1,
                ws_key.2,
                ws_key.3,
            );
            self.cache_ws_key = ws_key;
        }

        let mut trigger_dl_textures = false;
        surface_frame().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                if ui.add(pill_button("Import Map", MINT)).clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Map", &["udk", "upk", "zip"]).pick_file() {
                        match import_workshop_file_or_zip(&p, None, None) {
                            Ok(entry) => {
                                self.workshop_library = list_library_maps();
                                if let Some(pos) = self.workshop_library.iter().position(|e| e.id == entry.id) {
                                    self.workshop_selected_idx = pos;
                                }
                                self.workshop_local_thumb = (String::new(), None);
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
                let cached_tex_cnt = count_cached_workshop_textures();
                let dl_tex_tip = format!(
                    "1-click download & install the shared Workshop Textures pack (~44.5 MB)\nCached in {}: {} files",
                    get_workshop_textures_dir().display(),
                    cached_tex_cnt
                );
                if ui
                    .add_enabled(!self.is_busy, pill_button("Download Textures", MINT))
                    .on_hover_text(dl_tex_tip)
                    .clicked()
                {
                    trigger_dl_textures = true;
                }
                if ui.add(pill_button("Install Textures", TEXT_PRIMARY)).on_hover_text("Install shared .upk/.tfc textures from a local folder").clicked() {
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
                if ui.add_enabled(!self.workshop_online_loading, pill_button("Refresh", TEXT_SECONDARY)).clicked() {
                    self.workshop_online_loading = true;
                    self.workshop_online_failed_thumbs.clear();
                    self.workshop_online_pending_thumbs.clear();
                    self.spawn_op_with_repaint(|tx| {
                        let _ = tx.send(WorkerEvent::WorkshopCatalogDone(fetch_all_online_workshop_maps(true)));
                    });
                }
                if let Some(sel) = self.workshop_library.get(self.workshop_selected_idx).cloned() {
                    if ui.add(pill_button("Open Folder", TEXT_SECONDARY)).clicked() {
                        let dir = get_workshop_library_dir().join(&sel.id);
                        let _ = std::process::Command::new("explorer").arg(dir).spawn();
                    }
                    if ui.add(pill_button("Delete Selected", CORAL)).clicked() {
                        let _ = delete_library_map(&sel);
                        self.workshop_library = list_library_maps();
                        self.workshop_selected_idx = 0;
                        self.workshop_local_thumb = (String::new(), None);
                        self.workshop_missing_deps.clear();
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                    status_badge(
                        rui,
                        if underpass_active { "Underpass Active" } else { "Underpass Slot" },
                        if underpass_active { MINT } else { TEXT_MUTED },
                        underpass_active,
                    );
                });
            });
            if !self.workshop_missing_deps.is_empty() {
                ui.add_space(4.0);
                ui.horizontal(|hui| {
                    let lw = (hui.available_width() - 150.0).max(120.0);
                    hui.allocate_ui(Vec2::new(lw, 24.0), |tui| {
                        tui.add(
                            egui::Label::new(
                                RichText::new(format!(
                                    "Missing texture packages ({}): {} — click Download & Fix Now to avoid checkerboard floors",
                                    self.workshop_missing_deps.len(),
                                    self.workshop_missing_deps.join(", ")
                                ))
                                .font(FontId::proportional(11.0))
                                .color(LEMON),
                            )
                            .truncate(),
                        );
                    });
                    hui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                        if rui.add_enabled(!self.is_busy, pill_button("Download & Fix Now", MINT)).clicked() {
                            trigger_dl_textures = true;
                        }
                    });
                });
            }
            ui.add_space(8.0);

            let mut pick_lib = None;
            let mut dbl_pick_lib = None;
            let mut pick_online = None;
            let mut start_dl: Option<OnlineWorkshopItem> = None;
            let sel_lib = self.workshop_selected_idx;
            let sel_online = self.workshop_selected_online_idx;
            let lib = &self.workshop_library;
            let ws_idxs = &self.cached_ws_idxs;
            let online = &self.workshop_online_items;
            let ws_thumbs = &self.workshop_online_thumbs;
            let ws_pending = &mut self.workshop_online_pending_thumbs;
            let ws_failed = &self.workshop_online_failed_thumbs;
            let ws_thumb_tx = &self.ws_thumb_tx;
            let is_busy = self.is_busy;
            let active_ws_id = if underpass_active {
                self.state.active_studio.workshop_map_id.as_deref()
            } else {
                None
            };
            let mut lib_name_counts: HashMap<String, usize> = HashMap::new();
            for e in lib {
                *lib_name_counts.entry(e.name.to_lowercase()).or_insert(0) += 1;
            }

            let ws_col_list_h = (ui.available_height() - 114.0).max(120.0);
            ui.columns(2, |cols| {
                cols[0].allocate_ui_with_layout(Vec2::new(cols[0].available_width(), 32.0), Layout::left_to_right(Align::Center), |lui| {
                    lui.set_min_height(32.0);
                    lui.label(RichText::new(format!("Local Library ({})", lib.len())).font(FontId::proportional(12.5)).color(TEXT_PRIMARY).strong());
                    if !lib.is_empty() {
                        lui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                            rui.label(RichText::new("Double-click to load").font(FontId::proportional(10.5)).color(TEXT_MUTED));
                        });
                    }
                });
                cols[0].add_space(6.0);
                list_frame().show(&mut cols[0], |ui| {
                    if lib.is_empty() {
                        ui.set_min_size(Vec2::new(ui.available_width(), ws_col_list_h));
                        paint_empty_preview(ui, Vec2::new(ui.available_width(), ws_col_list_h), "No Local Maps", "Import or drop a .udk / .zip map");
                    } else {
                        egui::ScrollArea::vertical()
                            .id_salt("min_ws_lib")
                            .auto_shrink([false, false])
                            .max_height(ws_col_list_h)
                            .show_rows(ui, 30.0, lib.len(), |ui, range| {
                                for i in range {
                                    let is_active_ws = active_ws_id == Some(lib[i].id.as_str());
                                    let extra_tag = if is_active_ws {
                                        Some(("Active", MINT))
                                    } else if !lib[i].extra_files_rel.is_empty() {
                                        Some(("Tex", TEXT_MUTED))
                                    } else {
                                        None
                                    };
                                    let display_name = if lib_name_counts.get(&lib[i].name.to_lowercase()).copied().unwrap_or(0) > 1 {
                                        let tail = lib[i].id.rsplit('_').next().unwrap_or(&lib[i].id);
                                        format!("{} (#{})", lib[i].name, tail)
                                    } else {
                                        lib[i].name.clone()
                                    };
                                    let (clicked, dbl) = minimal_row_ex(ui, sel_lib == i, 28.0, None, 0.0, &display_name, extra_tag);
                                    if clicked {
                                        pick_lib = Some(i);
                                    }
                                    if dbl {
                                        dbl_pick_lib = Some(i);
                                    }
                                }
                            });
                    }
                });

                cols[1].allocate_ui_with_layout(Vec2::new(cols[1].available_width(), 32.0), Layout::left_to_right(Align::Center), |ui| {
                    ui.set_min_height(32.0);
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let sw = (ui.available_width() - 282.0).max(70.0);
                    ui.allocate_ui(Vec2::new(sw, 28.0), |sui| {
                        search_line(sui, &mut self.workshop_online_search, &format!("Browse {} maps...", ws_idxs.len()));
                    });
                    let src_lbl = match self.workshop_source_filter {
                        1 => "Bakkes",
                        2 => "Lethamyr",
                        3 => "Steam",
                        _ => "All",
                    };
                    egui::ComboBox::from_id_salt("ws_src_dd")
                        .selected_text(RichText::new(src_lbl).font(FontId::proportional(11.0)).color(TEXT_PRIMARY))
                        .width(76.0)
                        .show_ui(ui, |ui| {
                            for (sid, lbl) in [(0usize, "All Sources"), (1, "BakkesPlugins"), (2, "Lethamyr"), (3, "Steam")] {
                                if ui.selectable_label(self.workshop_source_filter == sid, lbl).clicked() {
                                    self.workshop_source_filter = sid;
                                }
                            }
                        });
                    let sort_lbl = match self.workshop_sort_mode {
                        1 => "Top DL",
                        2 => "A-Z",
                        _ => "Featured",
                    };
                    egui::ComboBox::from_id_salt("ws_sort_dd")
                        .selected_text(RichText::new(sort_lbl).font(FontId::proportional(11.0)).color(TEXT_PRIMARY))
                        .width(72.0)
                        .show_ui(ui, |ui| {
                            for (mid, lbl) in [(0usize, "Featured"), (1, "Most Downloaded"), (2, "Name A-Z")] {
                                if ui.selectable_label(self.workshop_sort_mode == mid, lbl).clicked() {
                                    self.workshop_sort_mode = mid;
                                }
                            }
                        });
                    minimal_switch(ui, &mut self.workshop_direct_only, "1-Click");
                });
                cols[1].add_space(6.0);
                list_frame().show(&mut cols[1], |ui| {
                    if self.workshop_online_loading || ws_idxs.is_empty() {
                        ui.set_min_size(Vec2::new(ui.available_width(), ws_col_list_h));
                        paint_empty_preview(
                            ui,
                            Vec2::new(ui.available_width(), ws_col_list_h),
                            if self.workshop_online_loading { "Loading Catalog..." } else { "No Matches" },
                            "",
                        );
                    } else {
                        egui::ScrollArea::vertical()
                            .id_salt("min_ws_online")
                            .auto_shrink([false, false])
                            .max_height(ws_col_list_h)
                            .show_rows(ui, 54.0, ws_idxs.len(), |ui, range| {
                                for r in range {
                                    let Some(&real_i) = ws_idxs.get(r) else { continue };
                                    let Some(item) = online.get(real_i) else { continue };
                                    if let Some(ref tu) = item.thumb_url {
                                        if !ws_thumbs.contains_key(tu)
                                            && !ws_pending.contains(tu)
                                            && !ws_failed.contains(tu)
                                            && ws_pending.len() < MAX_INFLIGHT_ICONS
                                        {
                                            ws_pending.insert(tu.clone());
                                            let _ = ws_thumb_tx.send(tu.clone());
                                        }
                                    }
                                    let is_sel = sel_online == Some(real_i);
                                    let avail_w = ui.available_width().max(120.0);
                                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(avail_w, 50.0), Sense::click());
                                    if resp.hovered() {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                    }
                                    if resp.clicked() {
                                        pick_online = Some(real_i);
                                    }
                                    if ui.is_rect_visible(rect) {
                                        if is_sel {
                                            ui.painter().rect_filled(rect, Rounding::same(7.0), BG_SELECTED);
                                            ui.painter().rect_stroke(rect, Rounding::same(7.0), Stroke::new(1.0_f32, BORDER_ACTIVE));
                                        } else if resp.hovered() {
                                            ui.painter().rect_filled(rect, Rounding::same(7.0), BG_HOVER);
                                        }
                                        let inner = rect.shrink2(Vec2::new(6.0, 5.0));
                                        let mut rui = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(Layout::left_to_right(Align::Center)));
                                        rui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                                        let (tr, _) = rui.allocate_exact_size(Vec2::new(68.0, 40.0), Sense::hover());
                                        rui.painter().rect_filled(tr, Rounding::same(5.0), BG_CARD);
                                        rui.painter().rect_stroke(tr, Rounding::same(5.0), Stroke::new(1.0_f32, BORDER_SUBTLE));
                                        if let Some(tex) = item.thumb_url.as_ref().and_then(|u| ws_thumbs.get(u)) {
                                            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                                            rui.painter().image(tex.id(), tr.shrink(1.0), uv, Color32::WHITE);
                                        }
                                        rui.add_space(8.0);
                                        let tw = (rui.available_width() - 88.0).max(40.0);
                                        let sub = if item.downloads > 0 || item.likes > 0 {
                                            format!(
                                                "{} · {} DL · {} Likes",
                                                item.source,
                                                fmt_compact_count(item.downloads),
                                                fmt_compact_count(item.likes)
                                            )
                                        } else {
                                            item.source.clone()
                                        };
                                        rui.allocate_ui_with_layout(Vec2::new(tw, inner.height()), Layout::top_down(Align::LEFT), |tui| {
                                            tui.add_space(3.0);
                                            tui.add(
                                                egui::Label::new(RichText::new(&item.name).font(FontId::proportional(12.2)).color(TEXT_PRIMARY).strong())
                                                    .truncate()
                                                    .selectable(false),
                                            );
                                            tui.add(
                                                egui::Label::new(RichText::new(sub).font(FontId::proportional(10.5)).color(TEXT_MUTED))
                                                    .truncate()
                                                    .selectable(false),
                                            );
                                        });
                                        rui.with_layout(Layout::right_to_left(Align::Center), |bui| {
                                            if item.direct_download {
                                                if bui.add_enabled(!is_busy, pill_button("Download", MINT)).clicked() {
                                                    start_dl = Some(item.clone());
                                                }
                                            } else if bui.add(pill_button("Steam", TEXT_SECONDARY)).clicked() {
                                                let _ = std::process::Command::new("cmd").args(["/C", "start", "", &item.download_url]).spawn();
                                            }
                                        });
                                    }
                                }
                            });
                    }
                });
            });

            // Compact Selected Map Details Bar
            ui.add_space(6.0);
            if let Some(oidx) = self.workshop_selected_online_idx.and_then(|i| self.workshop_online_items.get(i)) {
                let desc = if oidx.short_desc.trim().is_empty() {
                    format!("{} · Select Download or Steam to add to your local library", oidx.source)
                } else {
                    format!("{}: {}", oidx.name, oidx.short_desc.trim())
                };
                well_frame().show(ui, |dui| {
                    dui.set_min_width(dui.available_width());
                    dui.horizontal(|hui| {
                        let dw = (hui.available_width() - 90.0).max(100.0);
                        hui.allocate_ui(Vec2::new(dw, 20.0), |tui| {
                            tui.add(egui::Label::new(RichText::new(desc).font(FontId::proportional(11.0)).color(TEXT_SECONDARY)).truncate());
                        });
                        hui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                            if rui.add(pill_button("Browser", TEXT_MUTED)).clicked() {
                                let _ = std::process::Command::new("cmd").args(["/C", "start", "", &oidx.download_url]).spawn();
                            }
                        });
                    });
                });
            } else if let Some(sel) = self.workshop_library.get(self.workshop_selected_idx).cloned() {
                if self.workshop_local_thumb.0 != sel.id {
                    let loaded_tex = sel.thumb_file_rel.as_ref().and_then(|rel| {
                        let tp = get_workshop_library_dir().join(&sel.id).join(rel);
                        image::open(&tp).ok().map(|im| {
                            let rgba = im.resize_exact(84, 48, FilterType::Triangle).to_rgba8();
                            ctx.load_texture(&format!("ws_loc_{}", sel.id), rgba_to_egui_image(&rgba), TextureOptions::LINEAR)
                        })
                    });
                    self.workshop_local_thumb = (sel.id.clone(), loaded_tex);
                }
                well_frame().show(ui, |dui| {
                    dui.set_min_width(dui.available_width());
                    dui.horizontal(|hui| {
                        if let Some(ref tex) = self.workshop_local_thumb.1 {
                            let (tr, _) = hui.allocate_exact_size(Vec2::new(42.0, 24.0), Sense::hover());
                            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                            hui.painter().image(tex.id(), tr, uv, Color32::WHITE);
                            hui.add_space(6.0);
                        }
                        let note = if sel.extra_files_rel.is_empty() {
                            format!("{} · Ready to load in Free Play > Underpass", sel.name)
                        } else {
                            format!("{} · {} bundled texture packages", sel.name, sel.extra_files_rel.len())
                        };
                        hui.add(egui::Label::new(RichText::new(note).font(FontId::proportional(11.0)).color(TEXT_SECONDARY)).truncate());
                    });
                });
            }

            if let Some(oi) = pick_online {
                self.workshop_selected_online_idx = Some(oi);
            }
            if let Some(i) = pick_lib.or(dbl_pick_lib) {
                self.workshop_selected_idx = i;
                self.workshop_selected_online_idx = None;
                self.workshop_missing_deps = scan_upk_external_dependencies(
                    &get_workshop_library_dir().join(&self.workshop_library[i].id).join(&self.workshop_library[i].map_file_rel),
                    Path::new(&self.cooked_dir),
                );
            }
            if let Some(di) = dbl_pick_lib {
                if let Some(entry) = self.workshop_library.get(di).cloned() {
                    load_sel = Some(entry);
                }
            }
            if let Some(it_cl) = start_dl {
                if is_workshop_textures_item(&it_cl) {
                    trigger_dl_textures = true;
                } else {
                    self.is_busy = true;
                    let tx_dl = self.tx.clone();
                    let ctx_cl = ctx.clone();
                    let cdir_dl = PathBuf::from(&self.cooked_dir);
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
                                let _ = tx_dl.send(WorkerEvent::WorkshopDownloadProgress(WorkshopDownloadProgress {
                                    name: name_cl.clone(),
                                    downloaded: dl,
                                    total: tot,
                                    phase: phase.to_string(),
                                }));
                                ctx_cl.request_repaint();
                            }
                        })
                        .map(|entry| {
                            let deps = scan_upk_external_dependencies(
                                &get_workshop_library_dir().join(&entry.id).join(&entry.map_file_rel),
                                &cdir_dl,
                            );
                            (entry, deps)
                        });
                        let _ = tx_dl.send(WorkerEvent::WorkshopDownloadDone(res));
                        ctx_cl.request_repaint();
                    });
                }
            }
        });

        if trigger_dl_textures {
            self.trigger_workshop_textures_download(ctx, false);
        }
        if let Some(entry) = load_sel {
            self.trigger_workshop_load(entry);
        }
    }

    // ── 8. System: Presets ───────────────────────────────────────────────────
    fn render_system_tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            if folder_tab_btn(ui, self.active_tab == ActiveTab::Presets, "Presets") {
                self.active_tab = ActiveTab::Presets;
            }
            if folder_tab_btn(ui, self.active_tab == ActiveTab::Health, "Health & Backups") {
                self.refresh_health_report();
                self.active_tab = ActiveTab::Health;
            }
        });
        ui.add_space(6.0);
    }

    fn render_presets_tab(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.vertical(|vui| {
                vui.spacing_mut().item_spacing.y = 1.0;
                vui.label(RichText::new("Presets").font(FontId::proportional(21.0)).color(TEXT_PRIMARY).strong());
                vui.label(
                    RichText::new(format!(
                        "Saved as individual folders in {}",
                        get_presets_dir().display()
                    ))
                    .font(FontId::proportional(11.0))
                    .color(TEXT_MUTED),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                let can_save = !self.new_preset_name.trim().is_empty();
                if white_cta_button(rui, can_save, "Save Preset", 112.0) {
                    let preset = self.build_preset_from_active(&self.new_preset_name);
                    self.state.active_preset_name = Some(preset.name.clone());
                    self.presets.retain(|p| !p.name.eq_ignore_ascii_case(&preset.name));
                    self.presets.push(preset);
                    self.save_presets();
                    self.save_state();
                    self.status_is_error = false;
                    self.status_text = format!("Saved preset folder '{}'", self.new_preset_name.trim());
                }
            });
        });
        ui.add_space(10.0);

        self.render_system_tabs(ui);

        surface_frame().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let mut import_json_path: Option<PathBuf> = None;
            ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 28.0), Layout::left_to_right(Align::Center), |ui| {
                ui.allocate_ui(Vec2::new(220.0, 28.0), |nui| {
                    minimal_text_box(nui, &mut self.new_preset_name, "Preset name...");
                });
                if ui.add(pill_button("Import JSON", TEXT_SECONDARY)).clicked() {
                    import_json_path = rfd::FileDialog::new().add_filter("Preset JSON", &["json"]).pick_file();
                }
                if ui
                    .add(pill_button("Open Presets Folder", TEXT_SECONDARY))
                    .on_hover_text("Open Downloads\\RL Swapper\\Presets in Windows Explorer")
                    .clicked()
                {
                    let _ = std::process::Command::new("explorer").arg(get_presets_dir()).spawn();
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                    minimal_switch(rui, &mut self.preset_clean_apply, "Clean Apply");
                });
            });
            if let Some(p) = import_json_path {
                self.import_preset_file(&p);
            }
            ui.add_space(10.0);

            let mut delete_idx = None;
            let mut apply_pr: Option<LoadoutPreset> = None;
            let ah = ui.available_height().max(200.0);
            list_frame().show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("min_presets_list")
                    .auto_shrink([false, false])
                    .max_height((ah - 18.0).max(140.0))
                    .show(ui, |ui| {
                        if self.presets.is_empty() {
                            paint_empty_preview(ui, Vec2::new(ui.available_width(), 160.0), "No Presets Saved", "Includes cosmetic swaps + active Studio decals, PFP, banner & ball");
                        }
                        for (idx, pr) in self.presets.iter().enumerate() {
                            let sub = format_preset_summary(pr);
                            let is_active_pr = self
                                .state
                                .active_preset_name
                                .as_deref()
                                .map_or(false, |a| a.eq_ignore_ascii_case(&pr.name));
                            egui::Frame::none()
                                .fill(BG_CARD)
                                .stroke(Stroke::new(1.0_f32, if is_active_pr { BORDER_ACTIVE } else { BORDER_SUBTLE }))
                                .rounding(Rounding::same(8.0))
                                .inner_margin(egui::Margin::symmetric(12.0, 9.0))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        let iw = (ui.available_width() - 295.0).max(80.0);
                                        ui.allocate_ui_with_layout(Vec2::new(iw, 32.0), Layout::top_down(Align::LEFT), |iui| {
                                            let title_txt = if is_active_pr {
                                                format!("{} (Active)", pr.name)
                                            } else {
                                                pr.name.clone()
                                            };
                                            iui.add(
                                                egui::Label::new(
                                                    RichText::new(title_txt)
                                                        .font(FontId::proportional(13.5))
                                                        .color(if is_active_pr { MINT } else { TEXT_PRIMARY })
                                                        .strong(),
                                                )
                                                .truncate()
                                                .selectable(false),
                                            );
                                            iui.add(
                                                egui::Label::new(RichText::new(sub).font(FontId::proportional(11.0)).color(TEXT_MUTED))
                                                    .truncate()
                                                    .selectable(false),
                                            );
                                        });
                                        ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                                            if rui.add(pill_button("Delete", CORAL)).clicked() {
                                                delete_idx = Some(idx);
                                            }
                                            if rui.add(pill_button("Export", TEXT_SECONDARY)).clicked() {
                                                if let Some(sp) = rfd::FileDialog::new().set_file_name(&format!("{}.json", pr.name.replace(' ', "_"))).save_file() {
                                                    if let Ok(js) = serde_json::to_string_pretty(pr) {
                                                        let _ = fs::write(sp, js);
                                                    }
                                                }
                                            }
                                            if rui
                                                .add(pill_button("Folder", TEXT_SECONDARY))
                                                .on_hover_text("Open this preset's folder (PNG textures + preset.json)")
                                                .clicked()
                                            {
                                                let pdir = preset_folder_for_name(&pr.name);
                                                let _ = save_preset_to_folder(pr);
                                                let _ = std::process::Command::new("explorer").arg(pdir).spawn();
                                            }
                                            if white_cta_button(rui, !self.is_busy, "Apply", 78.0) {
                                                apply_pr = Some(pr.clone());
                                            }
                                        });
                                    });
                                });
                            ui.add_space(6.0);
                        }
                    });
            });

            if let Some(i) = delete_idx {
                self.presets.remove(i);
                self.save_presets();
            }
            if let Some(pr_cl) = apply_pr {
                let clean = self.preset_clean_apply;
                self.apply_preset(pr_cl, clean);
            }
        });
    }

    // ── 9. System: Health & Active Backups ───────────────────────────────────
    fn render_active_health_tab(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.vertical(|vui| {
                vui.spacing_mut().item_spacing.y = 1.0;
                vui.label(RichText::new("Health & Backups").font(FontId::proportional(21.0)).color(TEXT_PRIMARY).strong());
                vui.label(RichText::new("Manage backups, texture cache & restore vanilla files").font(FontId::proportional(11.0)).color(TEXT_MUTED));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                if white_cta_button(rui, !self.is_busy, "Reset All Vanilla", 130.0) {
                    self.reset_all_swaps();
                }
                if rui.add(pill_button("Repair", TEXT_PRIMARY)).clicked() {
                    let (synced, healed) = repair_cooked_health(Path::new(&self.cooked_dir));
                    self.refresh_health_report();
                    self.status_text = format!("Repaired {} backups · {} borders", synced, healed);
                }
                if rui.add(pill_button("Clear Cache", TEXT_SECONDARY)).clicked() {
                    clear_rl_texture_cache();
                    self.refresh_health_report();
                    self.status_text = "Cleared RL cache".into();
                }
            });
        });
        ui.add_space(10.0);

        self.render_system_tabs(ui);

        surface_frame().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            if let Some(ref rep) = self.health_report {
                ui.horizontal_wrapped(|ui| {
                    meta_col(ui, "Backups", &rep.total_bak_files.to_string(), TEXT_PRIMARY);
                    ui.add_space(20.0);
                    meta_col(ui, "Texture Cache", &rep.cache_file_count.to_string(), TEXT_SECONDARY);
                    ui.add_space(20.0);
                    meta_col(
                        ui,
                        "Underpass",
                        if rep.underpass_swapped { "Workshop" } else { "Vanilla" },
                        if rep.underpass_swapped { MINT } else { TEXT_SECONDARY },
                    );
                    if !rep.damaged_avatar_borders.is_empty() {
                        ui.add_space(20.0);
                        meta_col(ui, "Damaged Borders", &rep.damaged_avatar_borders.len().to_string(), LEMON);
                    }
                });
                ui.add_space(10.0);
            }

            let mut restore_key: Option<(String, String)> = None;
            let ah = ui.available_height().max(200.0);
            list_frame().show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("min_health_list")
                    .auto_shrink([false, false])
                    .max_height((ah - 18.0).max(140.0))
                    .show(ui, |ui| {
                        let disk_baks = self.health_report.as_ref().map(|r| r.bak_filenames.as_slice()).unwrap_or(&[]);
                        if self.state.active_swaps.is_empty() && disk_baks.is_empty() {
                            paint_empty_preview(ui, Vec2::new(ui.available_width(), 160.0), "All Files Vanilla", "No modified packages detected");
                        }
                        for rec in self.state.active_swaps.values() {
                            ui.horizontal(|ui| {
                                ui.add_space(4.0);
                                status_badge(ui, &rec.kind, TEXT_SECONDARY, false);
                                let w = (ui.available_width() - 92.0).max(80.0);
                                ui.allocate_ui_with_layout(Vec2::new(w, 28.0), Layout::left_to_right(Align::Center), |lui| {
                                    lui.add(
                                        egui::Label::new(RichText::new(format!("{} -> {}", rec.donor_desc, rec.target_desc)).color(TEXT_PRIMARY))
                                            .truncate()
                                            .selectable(false),
                                    );
                                });
                                ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                                    if rui.add(pill_button("Restore", CORAL)).clicked() {
                                        restore_key = Some((rec.target_pkg.clone(), rec.target_desc.clone()));
                                    }
                                });
                            });
                        }
                        for pkg in disk_baks {
                            if !self.state.active_swaps.contains_key(&pkg.to_lowercase()) {
                                ui.horizontal(|ui| {
                                    ui.add_space(4.0);
                                    status_badge(ui, "BAK", TEXT_MUTED, false);
                                    let w = (ui.available_width() - 92.0).max(80.0);
                                    ui.allocate_ui_with_layout(Vec2::new(w, 28.0), Layout::left_to_right(Align::Center), |lui| {
                                        lui.add(
                                            egui::Label::new(RichText::new(pkg).color(TEXT_SECONDARY))
                                                .truncate()
                                                .selectable(false),
                                        );
                                    });
                                    ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                                        if rui.add(pill_button("Restore", CORAL)).clicked() {
                                            restore_key = Some((pkg.clone(), pkg.clone()));
                                        }
                                    });
                                });
                            }
                        }
                    });
            });

            if let Some((pkg, lbl)) = restore_key {
                self.restore_target_pkg(&pkg, &lbl);
            }
        });
    }

    // ── Quick Reset Modal ────────────────────────────────────────────────────
    fn render_reset_modal(&mut self, ctx: &egui::Context) {
        let mut scrim_clicked = false;
        egui::Area::new(egui::Id::new("min_reset_scrim"))
            .fixed_pos(egui::pos2(0.0, 0.0))
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                let r = ctx.screen_rect();
                if ui.allocate_rect(r, Sense::click()).clicked() {
                    scrim_clicked = true;
                }
                ui.painter().rect_filled(r, Rounding::ZERO, Color32::from_rgba_unmultiplied(0, 0, 0, 185));
            });

        let mut open = self.show_reset_modal && !scrim_clicked;
        let mut restore_k: Option<(String, String)> = None;
        let mut trigger_all = false;
        egui::Window::new("Active Swaps")
            .order(egui::Order::Foreground)
            .fade_in(false)
            .fade_out(false)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(BG_SURFACE)
                    .stroke(Stroke::new(1.0_f32, BORDER_ACTIVE))
                    .rounding(Rounding::same(10.0))
                    .shadow(egui::Shadow::NONE)
                    .inner_margin(egui::Margin::same(16.0)),
            )
            .fixed_size(Vec2::new(480.0, 320.0))
            .show(ctx, |ui| {
                ui.label(RichText::new("Restore individual packages or reset all to vanilla (Esc closes).").font(FontId::proportional(11.5)).color(TEXT_MUTED));
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);
                if self.state.active_swaps.is_empty() {
                    paint_empty_preview(ui, Vec2::new(ui.available_width(), 190.0), "No Active Swaps", "All packages are vanilla");
                } else {
                    list_frame().show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("min_reset_modal_list")
                            .auto_shrink([false, false])
                            .max_height(176.0)
                            .show(ui, |ui| {
                                for rec in self.state.active_swaps.values() {
                                    ui.horizontal(|ui| {
                                        ui.add_space(2.0);
                                        status_badge(ui, &rec.kind, TEXT_SECONDARY, false);
                                        let w = (ui.available_width() - 94.0).max(80.0);
                                        ui.allocate_ui_with_layout(Vec2::new(w, 28.0), Layout::left_to_right(Align::Center), |lui| {
                                            lui.add(
                                                egui::Label::new(RichText::new(format!("{} -> {}", rec.donor_desc, rec.target_desc)).color(TEXT_PRIMARY))
                                                    .truncate()
                                                    .selectable(false),
                                            );
                                        });
                                        ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                                            if rui.add(pill_button("Restore", CORAL)).clicked() {
                                                restore_k = Some((rec.target_pkg.clone(), rec.target_desc.clone()));
                                            }
                                        });
                                    });
                                }
                            });
                    });
                    ui.add_space(10.0);
                    let w = ui.available_width().max(160.0);
                    if white_cta_button(ui, true, "Reset All to Vanilla", w) {
                        trigger_all = true;
                    }
                }
            });
        self.show_reset_modal = open;
        if let Some((pkg, lbl)) = restore_k {
            self.restore_target_pkg(&pkg, &lbl);
        }
        if trigger_all {
            self.reset_all_swaps();
        }
    }

    fn render_image_crop_modal(&mut self, ctx: &egui::Context) {
        let mut scrim_clicked = false;
        egui::Area::new(egui::Id::new("min_crop_scrim"))
            .fixed_pos(egui::pos2(0.0, 0.0))
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                let r = ctx.screen_rect();
                if ui.allocate_rect(r, Sense::click()).clicked() {
                    scrim_clicked = true;
                }
                ui.painter().rect_filled(r, Rounding::ZERO, Color32::from_rgba_unmultiplied(0, 0, 0, 195));
            });

        let Some(ref mut modal) = self.crop_modal else { return };
        let mut open = !scrim_clicked;
        let mut cancelled = false;
        let mut apply_cut: Option<(CropTarget, RgbaImage, String, Option<PathBuf>)> = None;

        let title = match modal.target {
            CropTarget::Pfp => "Cut Profile Picture (1:1)",
            CropTarget::Banner => "Cut Player Banner (14:3)",
        };
        let aspect = modal.target.aspect();

        egui::Window::new(title)
            .order(egui::Order::Foreground)
            .fade_in(false)
            .fade_out(false)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(BG_SURFACE)
                    .stroke(Stroke::new(1.0_f32, BORDER_ACTIVE))
                    .rounding(Rounding::same(10.0))
                    .shadow(egui::Shadow::NONE)
                    .inner_margin(egui::Margin::same(16.0)),
            )
            .fixed_size(Vec2::new(580.0, 448.0))
            .show(ctx, |ui| {
                let (sw, sh) = modal.source_img.dimensions();
                let (rx, ry, rw, rh) = compute_crop_rect(sw, sh, aspect, modal.zoom, modal.pan_x, modal.pan_y);
                let (out_w, out_h) = modal.target.output_dims(rw);

                ui.horizontal(|hui| {
                    hui.label(
                        RichText::new("Drag frame to reposition · Scroll or slide to zoom")
                            .font(FontId::proportional(11.5))
                            .color(TEXT_MUTED),
                    );
                    hui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                        rui.label(
                            RichText::new(format!("{}×{} -> {}×{}", rw, rh, out_w, out_h))
                                .font(FontId::monospace(10.8))
                                .color(TEXT_SECONDARY),
                        );
                    });
                });
                ui.add_space(8.0);

                // Interactive crop canvas stage
                let stage_sz = Vec2::new(548.0, 300.0);
                let (stage_rect, stage_resp) = ui.allocate_exact_size(stage_sz, Sense::click_and_drag());
                let painter = ui.painter_at(stage_rect);
                painter.rect_filled(stage_rect, Rounding::same(8.0), BG_WELL);
                painter.rect_stroke(stage_rect, Rounding::same(8.0), Stroke::new(1.0_f32, BORDER_SUBTLE));

                let inner = stage_rect.shrink(8.0);
                let scale = (inner.width() / sw.max(1) as f32).min(inner.height() / sh.max(1) as f32);
                let dw = (sw as f32 * scale).max(1.0);
                let dh = (sh as f32 * scale).max(1.0);
                let disp_rect = egui::Rect::from_center_size(inner.center(), Vec2::new(dw, dh));

                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                painter.image(modal.preview_tex.id(), disp_rect, uv, Color32::WHITE);

                let cw_d = (rw as f32 / sw.max(1) as f32) * dw;
                let ch_d = (rh as f32 / sh.max(1) as f32) * dh;
                let cl = disp_rect.left() + (rx as f32 / sw.max(1) as f32) * dw;
                let ct = disp_rect.top() + (ry as f32 / sh.max(1) as f32) * dh;
                let crop_rect = egui::Rect::from_min_size(egui::pos2(cl, ct), Vec2::new(cw_d, ch_d));

                // Interactive drag & wheel zoom
                let slack_x_disp = (dw - cw_d).max(0.0);
                let slack_y_disp = (dh - ch_d).max(0.0);
                if stage_resp.dragged() {
                    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                    let delta = stage_resp.drag_delta();
                    if slack_x_disp > 1.0 {
                        modal.pan_x = (modal.pan_x + 2.0 * delta.x / slack_x_disp).clamp(-1.0, 1.0);
                    }
                    if slack_y_disp > 1.0 {
                        modal.pan_y = (modal.pan_y + 2.0 * delta.y / slack_y_disp).clamp(-1.0, 1.0);
                    }
                } else if stage_resp.hovered() {
                    if slack_x_disp > 1.0 || slack_y_disp > 1.0 {
                        ctx.set_cursor_icon(egui::CursorIcon::Grab);
                    }
                    let scroll = ui.input(|i| i.raw_scroll_delta.y);
                    if scroll.abs() > 0.1 {
                        modal.zoom = (modal.zoom + scroll * 0.0025).clamp(1.0, 6.0);
                    }
                }

                // Dim outside cut area with 4 scrim rects around crop_rect
                let mask_col = Color32::from_rgba_unmultiplied(0, 0, 0, 175);
                if crop_rect.top() > disp_rect.top() {
                    painter.rect_filled(
                        egui::Rect::from_min_max(disp_rect.min, egui::pos2(disp_rect.right(), crop_rect.top())),
                        Rounding::ZERO,
                        mask_col,
                    );
                }
                if crop_rect.bottom() < disp_rect.bottom() {
                    painter.rect_filled(
                        egui::Rect::from_min_max(egui::pos2(disp_rect.left(), crop_rect.bottom()), disp_rect.max),
                        Rounding::ZERO,
                        mask_col,
                    );
                }
                if crop_rect.left() > disp_rect.left() {
                    painter.rect_filled(
                        egui::Rect::from_min_max(egui::pos2(disp_rect.left(), crop_rect.top()), egui::pos2(crop_rect.left(), crop_rect.bottom())),
                        Rounding::ZERO,
                        mask_col,
                    );
                }
                if crop_rect.right() < disp_rect.right() {
                    painter.rect_filled(
                        egui::Rect::from_min_max(egui::pos2(crop_rect.right(), crop_rect.top()), egui::pos2(disp_rect.right(), crop_rect.bottom())),
                        Rounding::ZERO,
                        mask_col,
                    );
                }

                // Rule-of-thirds & target-specific Rocket League safe-zone overlay
                let grid_st = Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 42));
                for f in [1.0 / 3.0, 2.0 / 3.0] {
                    let gx = crop_rect.left() + crop_rect.width() * f;
                    let gy = crop_rect.top() + crop_rect.height() * f;
                    painter.line_segment([egui::pos2(gx, crop_rect.top()), egui::pos2(gx, crop_rect.bottom())], grid_st);
                    painter.line_segment([egui::pos2(crop_rect.left(), gy), egui::pos2(crop_rect.right(), gy)], grid_st);
                }
                match modal.target {
                    CropTarget::Pfp => {
                        let safe_sz = Vec2::splat(crop_rect.width().min(crop_rect.height()) * 0.65);
                        let safe_r = egui::Rect::from_center_size(crop_rect.center(), safe_sz);
                        painter.rect_stroke(safe_r, Rounding::same(6.0), Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(112, 243, 152, 130)));
                    }
                    CropTarget::Banner => {
                        let av_w = crop_rect.width() * (92.0 / 420.0);
                        let av_r = egui::Rect::from_min_max(
                            crop_rect.min,
                            egui::pos2(crop_rect.left() + av_w, crop_rect.bottom()),
                        );
                        painter.rect_filled(av_r, Rounding::ZERO, Color32::from_rgba_unmultiplied(242, 104, 104, 42));
                        painter.line_segment(
                            [egui::pos2(av_r.right(), crop_rect.top()), egui::pos2(av_r.right(), crop_rect.bottom())],
                            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(242, 104, 104, 155)),
                        );
                    }
                }
                painter.rect_stroke(crop_rect, Rounding::ZERO, Stroke::new(1.5_f32, WHITE_CTA_BG));

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Zoom").font(FontId::proportional(11.5)).color(TEXT_MUTED));
                    ui.add(egui::Slider::new(&mut modal.zoom, 1.0..=6.0).show_value(false).custom_formatter(|v, _| format!("{:.1}×", v)));
                    ui.label(RichText::new(format!("{:.1}×", modal.zoom)).font(FontId::monospace(11.0)).color(TEXT_SECONDARY));
                    ui.add_space(6.0);
                    if ui.add(pill_button("Reset", TEXT_SECONDARY)).clicked() {
                        modal.zoom = 1.0;
                        modal.pan_x = 0.0;
                        modal.pan_y = 0.0;
                    }
                    if ui.add(pill_button("Fit Full", TEXT_MUTED)).on_hover_text("Use entire uncropped image (stretches to target ratio)").clicked() {
                        let (fw, fh) = modal.target.output_dims(sw.min(sh));
                        let stretched = imageops::resize(&modal.source_img, fw, fh, FilterType::Lanczos3);
                        apply_cut = Some((modal.target, stretched, modal.filename.clone(), modal.source_path.clone()));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |rui| {
                        let enter_apply = !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::Enter));
                        if white_cta_button(rui, true, "Apply Cut", 104.0) || enter_apply {
                            let cut = crop_and_resize_rgba(
                                &modal.source_img,
                                aspect,
                                modal.zoom,
                                modal.pan_x,
                                modal.pan_y,
                                out_w,
                                out_h,
                            );
                            apply_cut = Some((modal.target, cut, modal.filename.clone(), modal.source_path.clone()));
                        }
                        if rui.add(pill_button("Cancel", CORAL)).clicked() {
                            apply_cut = None;
                            cancelled = true;
                        }
                    });
                });
            });

        open &= !cancelled;
        if let Some((tgt, img, fname, sp)) = apply_cut {
            self.apply_crop_result(tgt, img, fname, sp);
            self.crop_modal = None;
        } else if !open {
            self.crop_modal = None;
        }
    }
}

fn main() -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("RL Swapper")
        .with_inner_size([1120.0, 700.0])
        .with_min_inner_size([860.0, 540.0]);

    let logo_bytes = include_bytes!("../../assets/logo.png");
    if let Some(rounded_rgba) = make_rounded_icon_rgba(logo_bytes, 128) {
        viewport = viewport.with_icon(Arc::new(egui::IconData {
            rgba: rounded_rgba.into_raw(),
            width: 128,
            height: 128,
        }));
    }

    eframe::run_native(
        "RL Swapper",
        eframe::NativeOptions { viewport, ..Default::default() },
        Box::new(|cc| Ok(Box::new(RLSwapperGui::new(cc)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn ordered_slots_and_catalog_coverage() {
        let items = load_catalog_items(None);
        let mut by_slot: HashMap<String, Vec<CatalogItem>> = HashMap::new();
        for it in items {
            by_slot.entry(it.slot.clone()).or_default().push(it);
        }
        assert_eq!(ORDERED_SLOTS.len(), 12);
        for &slot in ORDERED_SLOTS {
            let cnt = by_slot.get(slot).map(|v| v.len()).unwrap_or(0);
            assert!(cnt > 0, "Slot '{}' must have embedded items", slot);
        }
        // Every Body item must resolve cleanly into one of the 6 hitboxes
        let bodies = &by_slot["Body"];
        let mut seen_hb = HashSet::new();
        for b in bodies {
            seen_hb.insert(body_hitbox_for_item(&b.product, &b.asset_package).label());
        }
        assert_eq!(seen_hb.len(), 6, "All 6 hitboxes must appear across Body catalog");
    }

    #[test]
    fn cosmetic_filtering_hitbox_search_and_swapped_only() {
        let dir = std::env::temp_dir().join("rl_gui_test_cosmetic_filter");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let oct = CatalogItem {
            id: 1,
            product: "Octane".into(),
            slot: "Body".into(),
            asset_package: "Body_Octane_SF.upk".into(),
            asset_path: "Body_Octane.Body_Octane".into(),
            quality: "Common".into(),
        };
        let fen = CatalogItem {
            id: 2,
            product: "Fennec".into(),
            slot: "Body".into(),
            asset_package: "body_grain_SF.upk".into(),
            asset_path: "body_grain.body_grain".into(),
            quality: "Import".into(),
        };
        let merc = CatalogItem {
            id: 3,
            product: "Merc".into(),
            slot: "Body".into(),
            asset_package: "Body_Vanquish_SF.upk".into(),
            asset_path: "Body_Vanquish.Body_Vanquish".into(),
            quality: "Common".into(),
        };
        let list = vec![oct.clone(), fen, merc];

        // Hitbox filter Octane -> Octane + Fennec
        let mut active = HashMap::new();
        let hb_oct = filter_cosmetic_indices(&list, "", Some(CarHitbox::Octane), false, &active, &dir);
        assert_eq!(hb_oct, vec![0, 1]);

        // Search filter matches package or name
        let search_grain = filter_cosmetic_indices(&list, "grain", None, false, &active, &dir);
        assert_eq!(search_grain, vec![1]);

        // Swapped-only via active_swaps
        active.insert(
            "body_octane_sf.upk".into(),
            ActiveSwapRecord {
                target_pkg: "Body_Octane_SF.upk".into(),
                donor_desc: "Fennec".into(),
                target_desc: "Octane".into(),
                kind: "ITEM".into(),
                slot: "Body".into(),
                donor_pkg: "body_grain_SF.upk".into(),
                donor_asset_path: String::new(),
                target_asset_path: String::new(),
                paint_hex: None,
                paint_name: None,
            },
        );
        let sw_idxs = filter_cosmetic_indices(&list, "", None, true, &active, &dir);
        assert_eq!(sw_idxs, vec![0]);

        // Swapped-only via .bak disk detection (even without active_swaps)
        active.clear();
        fs::write(dir.join("Body_Vanquish_SF.upk.bak"), b"bak").unwrap();
        let bak_idxs = filter_cosmetic_indices(&list, "", None, true, &active, &dir);
        assert_eq!(bak_idxs, vec![2]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn fifo_eviction_and_paint_specs() {
        let mut map = HashMap::new();
        let mut order = VecDeque::new();
        for i in 0..5 {
            evict_fifo_insert(&mut map, &mut order, 3, format!("k{}", i), i);
        }
        assert_eq!(map.len(), 3);
        assert!(!map.contains_key("k0"));
        assert!(!map.contains_key("k1"));
        assert!(map.contains_key("k2"));
        assert!(map.contains_key("k3"));
        assert!(map.contains_key("k4"));

        let paints = official_paints();
        assert_eq!(paints.len(), 14);
        let c = PaintColorSpec::from_custom_hex("#00ffd5");
        assert_eq!(c.hex_srgb, "#00FFD5");
        assert_eq!(c.rgb8(), [0x00, 0xFF, 0xD5]);
    }

    #[test]
    fn workshop_filtering_sorting_and_counts() {
        let ws_items = vec![
            OnlineWorkshopItem {
                name: "Neon Rings".into(),
                source: "Lethamyr".into(),
                download_url: "https://example.com/1.zip".into(),
                thumb_url: None,
                direct_download: true,
                short_desc: "Hard dribble".into(),
                downloads: 500,
                likes: 20,
            },
            OnlineWorkshopItem {
                name: "Aim Training".into(),
                source: "Steam Workshop".into(),
                download_url: "https://steamcommunity.com/sharedfiles/filedetails/?id=1".into(),
                thumb_url: None,
                direct_download: false,
                short_desc: "Coco aim map".into(),
                downloads: 90000,
                likes: 1200,
            },
            OnlineWorkshopItem {
                name: "Bakkes Dribble".into(),
                source: "BakkesPlugins".into(),
                download_url: "https://example.com/3.zip".into(),
                thumb_url: None,
                direct_download: true,
                short_desc: "Challenge".into(),
                downloads: 4200,
                likes: 85,
            },
        ];
        // Sort by Most Downloaded
        let by_dl = filter_sort_workshop_indices(&ws_items, "", false, 0, 1);
        assert_eq!(by_dl, vec![1, 2, 0]);
        // Sort A-Z + 1-click only
        let az_dir = filter_sort_workshop_indices(&ws_items, "", true, 0, 2);
        assert_eq!(az_dir, vec![2, 0]);
        // Filter source BakkesPlugins
        let bp = filter_sort_workshop_indices(&ws_items, "", false, 1, 0);
        assert_eq!(bp, vec![2]);
        assert_eq!(fmt_compact_count(950), "950");
        assert_eq!(fmt_compact_count(12500), "12.5k");
        assert_eq!(fmt_compact_count(2_300_000), "2.3M");
    }

    #[test]
    fn dropped_file_classifier_and_preset_json_roundtrip() {
        let dir = std::env::temp_dir().join("rl_gui_test_drops");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let udk = dir.join("map.udk");
        fs::write(&udk, b"udk").unwrap();
        assert_eq!(classify_dropped_file(&udk), DroppedFileKind::WorkshopMap);

        let png = dir.join("skin.png");
        fs::write(&png, b"png").unwrap();
        assert_eq!(classify_dropped_file(&png), DroppedFileKind::ImageAsset);

        // Workshop zip containing a .udk entry vs AlphaConsole decal zip
        let ws_zip_p = dir.join("ws.zip");
        {
            let mut z = zip::ZipWriter::new(fs::File::create(&ws_zip_p).unwrap());
            z.start_file("Course/Map_P.udk", zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(b"data").unwrap();
            z.finish().unwrap();
        }
        assert_eq!(classify_dropped_file(&ws_zip_p), DroppedFileKind::WorkshopMap);

        let ac_zip_p = dir.join("ac.zip");
        {
            let mut z = zip::ZipWriter::new(fs::File::create(&ac_zip_p).unwrap());
            z.start_file("MyDecal/MyDecal.json", zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(b"{}").unwrap();
            z.finish().unwrap();
        }
        assert_eq!(classify_dropped_file(&ac_zip_p), DroppedFileKind::DecalOrBallPack);

        let preset = LoadoutPreset {
            name: "Main Octane".into(),
            created_at: "2026-11-17 09:40".into(),
            item_swaps: vec![],
            custom_decal: None,
            custom_pfp: None,
            custom_banner: None,
            custom_ball: None,
            workshop_map_id: Some("rings_map".into()),
        };
        let pr_path = dir.join("preset.json");
        fs::write(&pr_path, serde_json::to_string_pretty(&preset).unwrap()).unwrap();
        assert_eq!(classify_dropped_file(&pr_path), DroppedFileKind::PresetJson);

        let ac_json = dir.join("decal.json");
        fs::write(&ac_json, r#"{"Decal":{"BodyID":23}}"#).unwrap();
        assert_eq!(classify_dropped_file(&ac_json), DroppedFileKind::DecalOrBallPack);

        let txt = dir.join("notes.txt");
        fs::write(&txt, b"hello").unwrap();
        assert_eq!(classify_dropped_file(&txt), DroppedFileKind::Unsupported);

        let _ = fs::remove_dir_all(&dir);
    }

    fn make_test_gui(dir: &Path) -> RLSwapperGui {
        let catalog = load_catalog_items(None);
        let mut items_by_slot: HashMap<String, Vec<CatalogItem>> = HashMap::new();
        for it in catalog {
            if !it.slot.is_empty() {
                items_by_slot.entry(it.slot.clone()).or_default().push(it);
            }
        }
        let (tx, rx) = mpsc::channel();
        let (icon_job_tx, _) = mpsc::channel();
        let (ws_thumb_tx, _) = mpsc::channel();
        RLSwapperGui {
            cooked_dir: dir.to_string_lossy().to_string(),
            state: AppPersistentState::default(),
            active_tab: ActiveTab::Cosmetics,
            active_slot_idx: 1, // Decal (3300+ items)
            items_by_slot,
            avatar_borders: Vec::new(),
            player_banners: Vec::new(),
            paints: official_paints(),
            selected_paint_idx: Some(0), // Titanium White
            custom_hex_color: [0, 229, 255],
            use_custom_hex: false,
            want_search: "z".into(),
            have_search: "w".into(),
            selected_want_idx: 0,
            selected_have_idx: 0,
            body_hitbox_filter: None,
            cosmetic_swapped_only: false,
            cache_slot_key: (String::new(), None, "\x01".into()),
            cached_want_idxs: Vec::new(),
            cache_have_key: (String::new(), None, "\x01".into(), false, 0),
            cached_have_idxs: Vec::new(),
            egui_ctx: None,
            icon_mgr: Arc::new(IconCacheManager::new()),
            icon_job_tx,
            tx,
            rx,
            gpu_icons: HashMap::new(),
            gpu_icon_order: VecDeque::new(),
            pending_icons: HashSet::new(),
            is_busy: false,
            status_text: String::new(),
            status_is_error: false,
            decal_car_label: "Octane",
            decal_filter_car_only: true,
            decal_target_idx: 0,
            decal_target_search: String::new(),
            cache_decal_key: None,
            cached_decal_idxs: Vec::new(),
            ac_decal_packs: Vec::new(),
            ac_decal_idx: 0,
            decal_custom_img: None,
            decal_mask_img: None,
            decal_custom_path: String::new(),
            decal_primary_rgb: [30, 64, 175],
            decal_accent_rgb: [56, 189, 248],
            decal_preview_tex: None,
            decal_preview_dirty: true,
            decal_initialized: false,
            pfp_border_idx: 0,
            pfp_search: String::new(),
            cache_pfp_q: None,
            cached_pfp_idxs: Vec::new(),
            pfp_custom_img: None,
            pfp_composited_img: None,
            pfp_custom_path: String::new(),
            pfp_source_path: None,
            pfp_preview_tex: None,
            pfp_preview_dirty: true,
            banner_target_idx: 0,
            banner_search: String::new(),
            cache_banner_q: None,
            cached_banner_idxs: Vec::new(),
            banner_custom_img: None,
            banner_custom_path: String::new(),
            banner_source_path: None,
            banner_player_name: String::new(),
            banner_bake_name: false,
            banner_preview_tex: None,
            banner_preview_dirty: true,
            banner_initialized: false,
            ac_ball_packs: Vec::new(),
            ac_ball_idx: 0,
            ball_custom_img: None,
            ball_normal_img: None,
            ball_mask_img: None,
            ball_custom_path: String::new(),
            ball_orb_tex: None,
            ball_flat_tex: None,
            ball_norm_tex: None,
            ball_mask_tex: None,
            ball_orb_dirty: true,
            ball_initialized: false,
            crop_modal: None,
            workshop_library: Vec::new(),
            workshop_selected_idx: 0,
            workshop_selected_online_idx: None,
            workshop_local_thumb: (String::new(), None),
            workshop_missing_deps: Vec::new(),
            workshop_online_items: Vec::new(),
            workshop_online_loading: false,
            workshop_online_search: String::new(),
            workshop_direct_only: false,
            workshop_source_filter: 0,
            workshop_sort_mode: 0,
            cache_ws_key: (String::new(), false, 0, 0, usize::MAX),
            cached_ws_idxs: Vec::new(),
            workshop_online_thumbs: HashMap::new(),
            ws_thumb_order: VecDeque::new(),
            workshop_online_pending_thumbs: HashSet::new(),
            workshop_online_failed_thumbs: HashSet::new(),
            ws_thumb_tx,
            workshop_dl_progress: None,
            presets: Vec::new(),
            new_preset_name: String::new(),
            preset_clean_apply: true,
            health_report: None,
            swaps_revision: 1,
            show_reset_modal: false,
            rl_running: false,
            last_proc_check: Instant::now(),
            app_logo_tex: None,
            snap_state: None,
        }
    }

    #[test]
    fn slot_switch_with_active_search_never_panics_and_post_swap_resets_paint_and_search() {
        let dir = std::env::temp_dir().join("rl_gui_test_slot_switch_and_reset");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let mut app = make_test_gui(&dir);

        // 1. Reproduce exact crash setup: search in huge slot (Decal, len > 3000) -> high raw indices
        app.sync_cosmetic_caches();
        assert!(app.cached_want_idxs.iter().any(|&idx| idx > 200));

        // 2. Switch immediately to smallest slot (Paint Finish, index 8, ~153 items)
        let pf_idx = ORDERED_SLOTS.iter().position(|&s| s == "Paint Finish").unwrap();
        app.switch_cosmetic_slot(pf_idx);
        assert!(app.want_search.is_empty());
        assert!(app.have_search.is_empty());
        let pf_len = app.items_by_slot["Paint Finish"].len();
        assert!(app.cached_want_idxs.iter().all(|&i| i < pf_len));
        assert!(app.cached_have_idxs.iter().all(|&i| i < pf_len));

        // 3. Test post-swap reset: switches Paint back to Default & resets search bars cleanly
        app.switch_cosmetic_slot(0); // Body
        app.want_search = "Fennec".into();
        app.have_search = "Octane".into();
        app.selected_paint_idx = Some(3);
        app.use_custom_hex = true;
        app.reset_cosmetic_after_swap(&[("body_grain_SF.upk".into(), "Body_Octane_SF.upk".into())]);
        assert_eq!(app.selected_paint_idx, None);
        assert!(!app.use_custom_hex);
        assert!(app.want_search.is_empty());
        assert!(app.have_search.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn clicking_exact_item_text_selects_row_via_egui() {
        let ctx = egui::Context::default();
        let run_frame = |events: Vec<egui::Event>, time: f64| -> (egui::Rect, (bool, bool)) {
            let mut row_rect = egui::Rect::NOTHING;
            let mut res = (false, false);
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(600.0, 200.0))),
                events,
                time: Some(time),
                ..Default::default()
            };
            let _ = ctx.run(raw, |c| {
                egui::CentralPanel::default().show(c, |ui| {
                    let top_before = ui.cursor().min;
                    let avail = ui.available_width();
                    res = minimal_row_ex(ui, false, 44.0, None, 32.0, "Titanium White Octane", Some(("MINT", MINT)));
                    row_rect = egui::Rect::from_min_size(top_before, egui::vec2(avail, 44.0));
                });
            });
            (row_rect, res)
        };

        let (row_rect, res0) = run_frame(vec![], 0.0);
        assert_eq!(res0, (false, false));
        let text_pt = egui::pos2(row_rect.left() + 62.0, row_rect.center().y);

        let _ = run_frame(
            vec![
                egui::Event::PointerMoved(text_pt),
                egui::Event::PointerButton {
                    pos: text_pt,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            0.05,
        );
        let (_, res2) = run_frame(
            vec![
                egui::Event::PointerMoved(text_pt),
                egui::Event::PointerButton {
                    pos: text_pt,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            0.10,
        );
        assert!(res2.0, "clicking exact item text inside minimal_row_ex must select the row");
    }

    #[test]
    fn presets_save_and_restore_custom_studio_decal_pfp_banner_ball() {
        let _lock = TEST_HUB_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join("rl_gui_test_studio_preset");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let make_img = |w: u32, h: u32, rgba: [u8; 4]| -> RgbaImage {
            RgbaImage::from_pixel(w, h, image::Rgba(rgba))
        };

        let tmp_files = [
            "test_preset_decal_skin.png",
            "test_preset_decal_base.png",
            "test_preset_decal_mask.png",
            "test_preset_pfp.png",
            "test_preset_banner.png",
            "test_preset_ball_diff.png",
            "test_preset_ball_norm.png",
            "test_preset_ball_mask.png",
        ];

        let mut app = make_test_gui(&dir);
        app.state.active_studio = ActiveStudioState {
            decal: Some(ActiveCustomDecalRef {
                name: "RLCS Custom".into(),
                target_pkg: "Skin_Octane_Flames_SF.upk".into(),
                target_desc: "Flames".into(),
                target_asset_path: "Skin_Octane_Flames.Body_Octane_Flames".into(),
                preferred_donor_pkg: None,
                skin_file: save_studio_png_file(tmp_files[0], &make_img(16, 16, [255, 20, 40, 255]), 1024),
                diffuse_base_file: save_studio_png_file(tmp_files[1], &make_img(16, 16, [20, 20, 30, 255]), 1024),
                mask_file: save_studio_png_file(tmp_files[2], &make_img(16, 16, [0, 255, 0, 255]), 1024),
            }),
            pfp: Some(ActiveCustomPfpRef {
                border_pkg: "AvatarBorder_Season1_SF.upk".into(),
                border_desc: "Season 1 Border".into(),
                image_file: save_studio_png_file(tmp_files[3], &make_img(24, 24, [10, 200, 255, 255]), 512).unwrap(),
            }),
            banner: Some(ActiveCustomBannerRef {
                banner_pkg: "PlayerBanner_RLCS_SF.upk".into(),
                banner_desc: "RLCS Banner".into(),
                image_file: save_studio_png_file(tmp_files[4], &make_img(42, 10, [250, 180, 0, 255]), 840).unwrap(),
                player_name: "ProPlayer".into(),
                bake_name: true,
            }),
            ball: Some(ActiveCustomBallRef {
                name: "Gold Orb".into(),
                diffuse_file: save_studio_png_file(tmp_files[5], &make_img(32, 32, [255, 215, 0, 255]), 1024).unwrap(),
                normal_file: save_studio_png_file(tmp_files[6], &make_img(16, 16, [128, 128, 255, 255]), 1024),
                mask_file: save_studio_png_file(tmp_files[7], &make_img(16, 16, [255, 255, 255, 255]), 1024),
            }),
            workshop_map_id: Some("leth_rings".into()),
        };

        let built = app.build_preset_from_active("Full Custom Loadout");
        let summary = format_preset_summary(&built);
        assert!(summary.contains("Decal: Flames"));
        assert!(summary.contains("PFP: Season 1 Border"));
        assert!(summary.contains("Banner: RLCS Banner"));
        assert!(summary.contains("Custom Ball"));
        assert!(summary.contains("Workshop"));

        let json = serde_json::to_string(&built).unwrap();
        let loaded: LoadoutPreset = serde_json::from_str(&json).unwrap();

        let dec = loaded.custom_decal.as_ref().unwrap();
        assert_eq!(dec.name, "RLCS Custom");
        assert_eq!(decode_b64_png_to_rgba(dec.skin_png_b64.as_ref().unwrap()).unwrap().dimensions(), (16, 16));
        assert_eq!(decode_b64_png_to_rgba(dec.diffuse_base_png_b64.as_ref().unwrap()).unwrap().dimensions(), (16, 16));
        assert_eq!(decode_b64_png_to_rgba(dec.mask_png_b64.as_ref().unwrap()).unwrap().dimensions(), (16, 16));

        let pfp = loaded.custom_pfp.as_ref().unwrap();
        assert_eq!(pfp.border_pkg, "AvatarBorder_Season1_SF.upk");
        assert_eq!(decode_b64_png_to_rgba(&pfp.image_png_b64).unwrap().dimensions(), (24, 24));

        let ban = loaded.custom_banner.as_ref().unwrap();
        assert_eq!(ban.player_name, "ProPlayer");
        assert!(ban.bake_name);
        assert_eq!(decode_b64_png_to_rgba(&ban.image_png_b64).unwrap().dimensions(), (42, 10));

        let ball = loaded.custom_ball.as_ref().unwrap();
        assert_eq!(ball.name, "Gold Orb");
        assert_eq!(decode_b64_png_to_rgba(&ball.diffuse_png_b64).unwrap().dimensions(), (32, 32));
        assert_eq!(decode_b64_png_to_rgba(ball.normal_png_b64.as_ref().unwrap()).unwrap().dimensions(), (16, 16));
        assert_eq!(decode_b64_png_to_rgba(ball.mask_png_b64.as_ref().unwrap()).unwrap().dimensions(), (16, 16));

        assert_eq!(loaded.workshop_map_id.as_deref(), Some("leth_rings"));

        for f in tmp_files {
            let _ = fs::remove_file(active_studio_dir().join(f));
        }
        let _ = fs::remove_dir_all(&dir);
    }

    static TEST_HUB_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn sync_studio_from_active_reloads_and_resets_studio_widgets() {
        let _lock = TEST_HUB_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join("rl_gui_test_sync_studio");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let test_files = ["test_sync_decal.png", "test_sync_pfp.png", "test_sync_banner.png", "test_sync_ball.png"];
        let px = RgbaImage::from_pixel(32, 32, image::Rgba([50, 140, 240, 255]));
        let mut app = make_test_gui(&dir);
        app.state.active_studio = ActiveStudioState {
            decal: Some(ActiveCustomDecalRef {
                name: "Synced Decal".into(),
                target_pkg: "Skin_Octane_Stripes_SF.upk".into(),
                target_desc: "Octane: Stripes".into(),
                target_asset_path: String::new(),
                preferred_donor_pkg: None,
                skin_file: save_studio_png_file(test_files[0], &px, 512),
                diffuse_base_file: None,
                mask_file: None,
            }),
            pfp: Some(ActiveCustomPfpRef {
                border_pkg: "AvatarBorder_Default_SF.upk".into(),
                border_desc: "Default".into(),
                image_file: save_studio_png_file(test_files[1], &px, 512).unwrap(),
            }),
            banner: Some(ActiveCustomBannerRef {
                banner_pkg: "PlayerBanner_RLCS_SF.upk".into(),
                banner_desc: "RLCS".into(),
                image_file: save_studio_png_file(test_files[2], &RgbaImage::from_pixel(84, 20, image::Rgba([20, 20, 20, 255])), 840).unwrap(),
                player_name: "c1ya".into(),
                bake_name: true,
            }),
            ball: Some(ActiveCustomBallRef {
                name: "Synced Ball".into(),
                diffuse_file: save_studio_png_file(test_files[3], &px, 512).unwrap(),
                normal_file: None,
                mask_file: None,
            }),
            workshop_map_id: None,
        };

        app.sync_studio_from_active(false);
        assert!(app.decal_initialized && app.decal_custom_img.is_some());
        assert_eq!(app.decal_custom_path, "Synced Decal (Active)");
        assert!(app.pfp_custom_img.is_some());
        assert_eq!(app.pfp_custom_path, "Active: Default");
        assert!(app.banner_initialized && app.banner_bake_name);
        assert_eq!(app.banner_player_name, "c1ya");
        assert!(app.ball_initialized && app.ball_custom_img.is_some());
        assert_eq!(app.ball_custom_path, "Synced Ball (Active)");

        app.state.active_studio = ActiveStudioState::default();
        app.sync_studio_from_active(true);
        assert!(!app.decal_initialized);
        assert!(app.pfp_custom_img.is_none());
        assert!(!app.banner_initialized);
        assert!(!app.ball_initialized);

        for f in test_files {
            let _ = fs::remove_file(active_studio_dir().join(f));
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn crop_target_and_apply_crop_result_produce_exact_aspect_assets() {
        let _lock = TEST_HUB_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join("rl_gui_test_crop_apply");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let src = RgbaImage::from_pixel(1920, 1080, image::Rgba([100, 180, 250, 255]));
        let (_, _, rw_p, _) = compute_crop_rect(src.width(), src.height(), CropTarget::Pfp.aspect(), 1.5, 0.25, -0.25);
        let (pw, ph) = CropTarget::Pfp.output_dims(rw_p);
        let pfp_cut = crop_and_resize_rgba(&src, CropTarget::Pfp.aspect(), 1.5, 0.25, -0.25, pw, ph);
        assert_eq!(pfp_cut.dimensions().0, pfp_cut.dimensions().1);
        assert!((128..=512).contains(&pfp_cut.dimensions().0));

        let (_, _, rw_b, _) = compute_crop_rect(src.width(), src.height(), CropTarget::Banner.aspect(), 1.0, 0.0, 0.0);
        let (bw, bh) = CropTarget::Banner.output_dims(rw_b);
        assert_eq!((bw, bh), (840, 200));
        let ban_cut = crop_and_resize_rgba(&src, CropTarget::Banner.aspect(), 1.0, 0.0, 0.0, bw, bh);
        assert_eq!(ban_cut.dimensions(), (840, 200));

        let mut app = make_test_gui(&dir);
        app.apply_crop_result(CropTarget::Pfp, pfp_cut, "test_crop_avatar.png".into(), None);
        assert!(app.pfp_custom_img.is_some());
        assert!(app.pfp_preview_dirty);
        let _ = fs::remove_file(get_custom_items_dir("PFP").join("test_crop_avatar.png"));

        app.apply_crop_result(CropTarget::Banner, ban_cut, "test_crop_banner.png".into(), None);
        assert!(app.banner_custom_img.is_some());
        assert!(app.banner_preview_dirty);
        let _ = fs::remove_file(get_custom_items_dir("Banners").join("test_crop_banner.png"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn hub_dir_preset_folders_custom_library_and_favorites_work() {
        let _lock = TEST_HUB_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let hub = std::env::temp_dir().join("rl_gui_test_hub_v8");
        let _ = fs::remove_dir_all(&hub);
        std::env::set_var("RL_SWAPPER_HUB_OVERRIDE", &hub);

        assert_eq!(get_swapper_hub_dir(), hub);
        assert!(get_presets_dir().is_dir());
        assert!(get_custom_items_dir("Decals").is_dir());
        assert!(get_custom_items_dir("PFP").is_dir());
        assert!(get_custom_items_dir("Banners").is_dir());
        assert!(get_custom_items_dir("Balls").is_dir());
        assert!(get_workshop_library_dir().is_dir());
        assert!(get_workshop_textures_dir().is_dir());
        assert_eq!(count_cached_workshop_textures(), 0);

        // 1. Preset folder round-trip with live PNG editing
        let red_img = RgbaImage::from_pixel(16, 16, image::Rgba([255, 0, 0, 255]));
        let blue_img = RgbaImage::from_pixel(32, 32, image::Rgba([0, 120, 255, 255]));
        let pr = LoadoutPreset {
            name: "Tournament Setup".into(),
            created_at: "2026-11-18 01:30".into(),
            item_swaps: vec![],
            custom_decal: None,
            custom_pfp: Some(PresetCustomPfp {
                border_pkg: "AvatarBorder_Season1_SF.upk".into(),
                border_desc: "Season 1".into(),
                image_png_b64: encode_rgba_to_b64_png(&red_img).unwrap(),
            }),
            custom_banner: None,
            custom_ball: None,
            workshop_map_id: None,
        };
        let folder = save_preset_to_folder(&pr).expect("preset folder created");
        assert!(folder.join("preset.json").is_file());
        assert!(folder.join("pfp.png").is_file());

        // Overwrite pfp.png directly in the preset folder and verify load_preset_from_folder picks up the new dimensions
        blue_img.save(folder.join("pfp.png")).unwrap();
        let reloaded = load_preset_from_folder(&folder).expect("preset loaded from folder");
        let loaded_pfp = decode_b64_png_to_rgba(&reloaded.custom_pfp.unwrap().image_png_b64).unwrap();
        assert_eq!(loaded_pfp.dimensions(), (32, 32));

        // 2. Custom Items Library persistence
        save_custom_image_to_library("PFP", "My_Avatar.png", &blue_img).unwrap();
        assert!(!list_saved_custom_images("PFP").is_empty());

        // 3. Favorites sorting
        let items = vec![
            CatalogItem {
                id: 1,
                product: "Aftershock".into(),
                slot: "Body".into(),
                asset_package: "Body_Aftershock_SF.upk".into(),
                asset_path: String::new(),
                quality: String::new(),
            },
            CatalogItem {
                id: 2,
                product: "Octane".into(),
                slot: "Body".into(),
                asset_package: "Body_Octane_SF.upk".into(),
                asset_path: String::new(),
                quality: String::new(),
            },
        ];
        let mut idxs = vec![0usize, 1usize];
        let mut favs = HashSet::new();
        favs.insert("body_octane_sf.upk".to_string());
        sort_cosmetic_indices_with_favorites(&mut idxs, &items, &favs);
        assert_eq!(idxs, vec![1, 0], "Favorited Octane must sort before Aftershock");

        std::env::remove_var("RL_SWAPPER_HUB_OVERRIDE");
        let _ = fs::remove_dir_all(&hub);
    }
}
