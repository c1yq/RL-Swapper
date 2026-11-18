use image::{Rgba, RgbaImage};
use rl_swapper::custom_studio::{
    compose_pfp_with_border_preview, decode_texture2d_export, inject_alphaconsole_ball,
    inject_alphaconsole_decal, inject_custom_banner, inject_custom_decal, inject_custom_pfp,
    list_avatar_border_packages, list_avatar_border_packages_with_catalog,
    list_player_banner_packages, parse_alphaconsole_ball_source, parse_alphaconsole_decal_source,
};
use rl_swapper::icon_system::{
    composite_icon_with_paint, extract_ball_texture_preview, scrape_item_icon_online,
    IconCacheManager,
};
use rl_swapper::map_editor::{
    is_standard_soccar_map_name, reset_all_standard_maps, swap_standard_map, STANDARD_SOCCAR_MAPS,
};
use rl_swapper::templates::{
    bake_name_onto_banner, generate_ball_template, generate_banner_template,
    generate_decal_template, generate_decal_template_for_car, render_2d_ball_orb_preview,
    render_2d_banner_scoreboard_preview, render_2d_car_decal_with_mask_preview,
};
use rl_swapper::upk_engine::{
    body_hitbox_for_item, find_catalog_item_by_id, find_export_properties, inspect_cooked_health,
    is_avatar_border_modified, load_catalog_items, official_paints, parse_property_stream,
    parse_upk_prefix, read_f32_le, read_fstring_at, read_i32_le, repair_cooked_health,
    restore_single_package, swap_item_packages, unpack_upk_from_disk, write_u32_le, CarHitbox,
};
use rl_swapper::workshop::{
    delete_library_map, download_online_workshop_map_with_progress,
    extract_bakkesplugins_download_url, extract_gdrive_confirm_url, fetch_all_online_workshop_maps,
    fetch_lethamyr_catalog_page, import_workshop_file_or_zip, load_workshop_map_into_underpass,
    parse_bakkesplugins_maps_html, restore_workshop_underpass, scan_upk_external_dependencies,
    OnlineWorkshopItem,
};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

fn detect_test_cooked_dir() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("RL_COOKED_DIR").map(PathBuf::from) {
        if p.is_dir() {
            return Some(p);
        }
    }
    for cand in [
        r"C:\Program Files\Epic Games\rocketleague\TAGame\CookedPCConsole",
        r"C:\Program Files (x86)\Steam\steamapps\common\rocketleague\TAGame\CookedPCConsole",
        r"D:\Epic Games\rocketleague\TAGame\CookedPCConsole",
        r"D:\SteamLibrary\steamapps\common\rocketleague\TAGame\CookedPCConsole",
    ] {
        let p = Path::new(cand);
        if p.is_dir() {
            return Some(p.to_path_buf());
        }
    }
    None
}

#[test]
fn test_v4_complete_pipeline() {
    let Some(real_cooked_buf) = detect_test_cooked_dir() else {
        eprintln!("Skipping UPK integration test: Rocket League CookedPCConsole not found (set RL_COOKED_DIR to enable).");
        return;
    };
    let real_cooked = real_cooked_buf.as_path();

    // 1. Test On-Demand Car-Specific 1:1 UV Templates (Extracted from Real Body UPKs)
    let oct_tpl = generate_decal_template("Octane");
    let oct_real = generate_decal_template_for_car("Octane", Some(real_cooked));
    let fen_real = generate_decal_template_for_car("Fennec", Some(real_cooked));
    let dom_real = generate_decal_template_for_car("Dominus", Some(real_cooked));
    let ban_tpl = generate_banner_template();
    let bal_tpl = generate_ball_template(Some(real_cooked));
    assert_eq!(oct_tpl.dimensions(), (1024, 1024));
    assert_eq!(oct_real.dimensions(), (1024, 1024));
    assert_eq!(fen_real.dimensions(), (1024, 1024));
    assert_eq!(dom_real.dimensions(), (1024, 1024));
    assert_ne!(oct_real.as_raw(), fen_real.as_raw(), "Octane and Fennec UV templates must differ");
    assert_eq!(rl_swapper::upk_engine::detect_car_uv_label("Octane: Stripes Skin_Octane_Stripes_SF.upk"), Some("Octane"));
    assert_ne!(rl_swapper::upk_engine::detect_car_uv_label("Octane ZSR: Funny Book skin_octane2_sf.upk"), Some("Octane"));
    assert_eq!(rl_swapper::upk_engine::detect_car_uv_label("Fennec: Stripes skin_grain_stripes_SF.upk"), Some("Fennec"));
    assert_eq!(rl_swapper::upk_engine::detect_car_uv_label("Dominus: Stripes Skin_MuscleCar_Stripes_SF.upk"), Some("Dominus"));
    assert_eq!(ban_tpl.dimensions(), (840, 200));
    assert_eq!(bal_tpl.dimensions(), (512, 512));

    // Verify live ball extraction from GameInfo_Soccar_SF.upk + Textures2.tfc
    let live_ball = extract_ball_texture_preview(real_cooked).expect("Must decode Ball_Default00_D");
    println!("Live RL Ball Texture decoded: {:?}", live_ball.dimensions());

    // 2. Verify Hitbox Mapping & 57 Standard Soccar Arenas Whitelist
    let catalog = load_catalog_items(Some(real_cooked));
    assert!(catalog.len() > 1200, "Catalog must load all items");
    assert_eq!(body_hitbox_for_item("Fennec", "body_grain_SF.upk"), CarHitbox::Octane);
    assert_eq!(body_hitbox_for_item("Dominus", "Body_MuscleCar_SF.upk"), CarHitbox::Dominus);
    assert_eq!(body_hitbox_for_item("Merc", "Body_Vanquish_SF.upk"), CarHitbox::Merc);
    assert_eq!(body_hitbox_for_item("Breakout", "Body_Force_SF.upk"), CarHitbox::Breakout);
    assert_eq!(body_hitbox_for_item("Batmobile (2016)", "Body_Darkcar_SF.upk"), CarHitbox::Plank);
    assert_eq!(body_hitbox_for_item("Endo", "body_endo_SF.upk"), CarHitbox::Hybrid);

    assert!(STANDARD_SOCCAR_MAPS.len() >= 90, "Expected 90+ playable arenas, got {}", STANDARD_SOCCAR_MAPS.len());
    assert!(STANDARD_SOCCAR_MAPS.iter().filter(|m| m.is_default_size).count() >= 60);
    assert!(!rl_swapper::map_editor::is_default_size_map_name("HoopsStadium_P.upk"));
    assert!(!rl_swapper::map_editor::is_default_size_map_name("ShatterShot_P.upk"));
    assert!(rl_swapper::map_editor::is_default_size_map_name("Park_P.upk"));
    assert!(rl_swapper::map_editor::is_default_size_map_name("UF_Day_P.upk"));
    assert!(!is_standard_soccar_map_name("RandomNonMap_P.upk"));

    // 3. Verify 2D Preview Renderers
    let custom_decal_test = RgbaImage::from_fn(512, 512, |x, y| {
        if (x / 32 + y / 32) % 2 == 0 {
            Rgba([0, 220, 255, 255])
        } else {
            Rgba([255, 40, 120, 180])
        }
    });
    assert_eq!(
        render_2d_car_decal_with_mask_preview("Octane", Some(real_cooked), Some(&custom_decal_test), Some(&custom_decal_test), [37, 99, 235], [245, 158, 11], 320).dimensions(),
        (320, 320)
    );
    assert_eq!(
        render_2d_banner_scoreboard_preview(Some(&custom_decal_test), Some(&custom_decal_test), "c1ya", 440, 150).dimensions(),
        (440, 150)
    );
    assert_eq!(bake_name_onto_banner(&ban_tpl, "c1ya").dimensions(), (420, 100));
    assert_eq!(render_2d_ball_orb_preview(&live_ball, 220, Some([0, 229, 255])).dimensions(), (220, 220));

    // 4. Icon Extraction + Wiki Scraping
    let icon_mgr = IconCacheManager::new();
    let tw = official_paints().into_iter().find(|p| p.name == "Titanium White").unwrap();
    let octane_item = catalog.iter().find(|i| i.asset_package.eq_ignore_ascii_case("Body_Octane_SF.upk")).unwrap();
    let (base_icon, _) = icon_mgr.get_icon_sync(real_cooked, octane_item, true);
    assert_eq!(composite_icon_with_paint(&base_icon, Some(&tw), 68).dimensions(), (68, 68));
    assert!(scrape_item_icon_online("Fennec", "Body").is_some());

    // 5. Create isolated temp sandbox for full UPK mutation & integrity tests
    let sandbox = std::env::temp_dir().join("rl_swapper_v41_verify");
    let _ = fs::remove_dir_all(&sandbox);
    fs::create_dir_all(&sandbox).unwrap();

    let banners = list_player_banner_packages(real_cooked);
    let test_banner_pkg = banners[0].1.clone();

    for f in [
        "Body_Octane_SF.upk",
        "Body_Backfire_SF.upk",
        "body_grain_SF.upk",
        "Body_MuscleCar_SF.upk",
        "Body_Zippy_SF.upk",
        "WHEEL_Atlantis_SF.upk",
        "WHEEL_Bling_SF.upk",
        "Boost_Bubble_SF.upk",
        "Explosion_Default_SF.upk",
        "explosion_chndragon_SF.upk",
        "SS_ScifiStreak_SF.upk",
        "Hat_Beanie_SF.upk",
        "Antenna_8Ball_SF.upk",
        "skin_octane_alpine_esports_SF.upk",
        "skin_octane_badbeats_SF.upk",
        "skin_octane_leaves_SF.upk",
        "Skin_Octane_Stripes_SF.upk",
        "skin_octane_anthony_SF.upk",
        "skin_grain_aloha_SF.upk",
        "skin_grain_bugbite_SF.upk",
        "Skin_MuscleCar_Stripes_SF.upk",
        "skin_musclecar_bubblebobbler_SF.upk",
        "skin_10x_SF.upk",
        "AvatarBorder_Default_SF.upk",
        "AvatarBorder_5thAnniversary_SF.upk",
        "AvatarBorder_2019ChineseValentinesDay_SF.upk",
        "AvatarBorder_Frosty_SF.upk",
        "avatarborder_s14rewardgrandchampion_SF.upk",
        "paca6f52bcd3aad77dd19_SF.upk",
        test_banner_pkg.as_str(),
        "GameInfo_Soccar_SF.upk",
        "Park_P.upk",
        "Stadium_P.upk",
        "EuroStadium_Night_P.upk",
        "Labs_CirclePillars_P.upk",
        "Labs_Underpass_P.upk",
    ] {
        let raw_src = real_cooked.join(f);
        let bak_src = rl_swapper::upk_engine::bak_path_for(&raw_src);
        let src = if bak_src.is_file() { bak_src } else { raw_src };
        if src.exists() {
            fs::copy(&src, sandbox.join(f)).unwrap();
        }
    }

    // 5a. Surgical Item Swap WITH ExportTable Rewiring Verification + Multi-Category + Car Body Trim Paint Test
    let target_item = catalog.iter().find(|i| i.asset_package.eq_ignore_ascii_case("Body_Backfire_SF.upk")).unwrap();
    let swap_msg = swap_item_packages(&sandbox, octane_item, target_item, Some(&tw)).expect("Item swap failed");
    println!("Item Swap verified: {}", swap_msg);
    let (rewired_upk, _) = unpack_upk_from_disk(&sandbox.join("Body_Backfire_SF.upk")).expect("Must unpack rewired UPK cleanly");
    let has_target_export = rewired_upk
        .exports
        .iter()
        .any(|e| rewired_upk.full_export_path(e.index).eq_ignore_ascii_case(&target_item.asset_path));
    assert!(has_target_export, "Rewired UPK must contain target AssetPath '{}'", target_item.asset_path);
    // Verify Octane swapped onto Backfire STILL carries SkinParameters.VectorParameters TrimColor = TW!
    {
        let body_pa = rewired_upk.exports.iter().find(|e| rewired_upk.export_class_name(e) == "ProductAsset_Body_TA").expect("Rewired body must have ProductAsset_Body_TA");
        let props = find_export_properties(&rewired_upk, body_pa, &rewired_upk.file_bytes);
        let sp = props.iter().find(|p| p.name == "SkinParameters").expect("Octane SkinParameters must exist");
        let sp_abs = body_pa.serial_offset + sp.value_offset;
        let (_, subs, _) = parse_property_stream(&rewired_upk, &rewired_upk.file_bytes[sp_abs..sp_abs + sp.size], 0);
        let vp = subs.iter().find(|s| s.name == "VectorParameters").expect("SkinParameters.VectorParameters must exist");
        let v_abs = sp_abs + vp.value_offset;
        let cnt = read_i32_le(&rewired_upk.file_bytes, v_abs).clamp(0, 32);
        let mut rel = 4usize;
        let mut found_trim = false;
        for _ in 0..cnt {
            let (nr, ep, ok) = parse_property_stream(&rewired_upk, &rewired_upk.file_bytes[v_abs..v_abs + vp.size], rel);
            if !ok && ep.is_empty() { break; }
            if let (Some(n), Some(v)) = (ep.iter().find(|x| x.name == "Name"), ep.iter().find(|x| x.name == "Value" && x.size == 16)) {
                if rewired_upk.resolve_name(read_i32_le(&rewired_upk.file_bytes, v_abs + n.value_offset)) == "TrimColor" {
                    let r = read_f32_le(&rewired_upk.file_bytes, v_abs + v.value_offset);
                    assert!((r - tw.linear_rgb.0).abs() < 0.03, "Swapped Body TrimColor red {r} != TW");
                    found_trim = true;
                }
            }
            rel = nr;
        }
        assert!(found_trim, "Swapped Car Body MUST carry SkinParameters.VectorParameters['TrimColor']!");
    }
    assert!(restore_single_package(&sandbox.join("Body_Backfire_SF.upk")));

    // Verify In-Place Paint Injection works across Car Bodies (Octane, Fennec, Dominus, Zippy), Wheels, Boost, Goal Explosion (incl. Paper Dragon), Trail, Topper, Antenna!
    let crimson = official_paints().into_iter().find(|p| p.name == "Crimson").unwrap();
    let cobalt = official_paints().into_iter().find(|p| p.name == "Cobalt").unwrap();
    for pkg in [
        "Body_Octane_SF.upk",
        "body_grain_SF.upk",
        "Body_MuscleCar_SF.upk",
        "Body_Zippy_SF.upk",
        "WHEEL_Atlantis_SF.upk",
        "WHEEL_Bling_SF.upk",
        "Boost_Bubble_SF.upk",
        "Explosion_Default_SF.upk",
        "explosion_chndragon_SF.upk",
        "SS_ScifiStreak_SF.upk",
        "Hat_Beanie_SF.upk",
        "Antenna_8Ball_SF.upk",
    ] {
        let paint_choice = if pkg == "explosion_chndragon_SF.upk" || pkg == "Body_MuscleCar_SF.upk" {
            &cobalt
        } else if pkg == "Body_Octane_SF.upk" {
            &tw
        } else {
            &crimson
        };
        let it = catalog
            .iter()
            .find(|i| i.asset_package.eq_ignore_ascii_case(pkg))
            .unwrap_or_else(|| panic!("Catalog must contain {}", pkg));
        let pmsg = swap_item_packages(&sandbox, it, it, Some(paint_choice)).expect("Paint swap failed");
        println!("Multi-slot Paint verified [{}]: {}", pkg, pmsg);
        assert!(
            !pmsg.contains("0 shader edits"),
            "Paint on {} reported 0 edits: {}",
            pkg,
            pmsg
        );
        let (painted_upk, _) = unpack_upk_from_disk(&sandbox.join(pkg))
            .unwrap_or_else(|e| panic!("Painted {} must unpack cleanly: {}", pkg, e));

        if pkg.starts_with("Body_") || pkg.starts_with("body_") {
            let pa = painted_upk
                .exports
                .iter()
                .find(|e| painted_upk.export_class_name(e) == "ProductAsset_Body_TA")
                .expect("Car Body must have ProductAsset_Body_TA");
            let props = find_export_properties(&painted_upk, pa, &painted_upk.file_bytes);
            let sp = props.iter().find(|p| p.name == "SkinParameters").expect("Car Body must have SkinParameters");
            let sp_abs = pa.serial_offset + sp.value_offset;
            let (_, subs, _) = parse_property_stream(&painted_upk, &painted_upk.file_bytes[sp_abs..sp_abs + sp.size], 0);
            let vp = subs.iter().find(|s| s.name == "VectorParameters").expect("SkinParameters must have VectorParameters");
            let v_abs = sp_abs + vp.value_offset;
            let cnt = read_i32_le(&painted_upk.file_bytes, v_abs).clamp(0, 32);
            let mut rel = 4usize;
            let mut found_trim = false;
            for _ in 0..cnt {
                let (nr, ep, ok) = parse_property_stream(&painted_upk, &painted_upk.file_bytes[v_abs..v_abs + vp.size], rel);
                if !ok && ep.is_empty() { break; }
                if let (Some(n), Some(v)) = (ep.iter().find(|x| x.name == "Name"), ep.iter().find(|x| x.name == "Value" && x.size == 16)) {
                    if painted_upk.resolve_name(read_i32_le(&painted_upk.file_bytes, v_abs + n.value_offset)) == "TrimColor" {
                        let r = read_f32_le(&painted_upk.file_bytes, v_abs + v.value_offset);
                        let g = read_f32_le(&painted_upk.file_bytes, v_abs + v.value_offset + 4);
                        let b = read_f32_le(&painted_upk.file_bytes, v_abs + v.value_offset + 8);
                        let (er, eg, eb) = paint_choice.linear_rgb;
                        assert!(
                            (r - er).abs() < 0.03 && (g - eg).abs() < 0.03 && (b - eb).abs() < 0.03,
                            "{} TrimColor ({r},{g},{b}) != expected ({er},{eg},{eb})",
                            pkg
                        );
                        found_trim = true;
                    }
                }
                rel = nr;
            }
            assert!(found_trim, "{} MUST have TrimColor injected in SkinParameters.VectorParameters!", pkg);

            if pkg == "Body_MuscleCar_SF.upk" {
                let trim_mic = painted_upk
                    .exports
                    .iter()
                    .find(|e| painted_upk.export_class_name(e) == "MaterialInstanceConstant" && painted_upk.resolve_name(e.object_name) == "MIC_Vehicle_Trim_MuscleCar")
                    .expect("Dominus must have MIC_Vehicle_Trim_MuscleCar");
                let mprops = find_export_properties(&painted_upk, trim_mic, &painted_upk.file_bytes);
                let vp = mprops.iter().find(|p| p.name == "VectorParameterValues").expect("Dominus MIC_Vehicle_Trim_MuscleCar MUST gain VectorParameterValues[TrimColor]!");
                let arr = trim_mic.serial_offset + vp.value_offset;
                let (_, ep, _) = parse_property_stream(&painted_upk, &painted_upk.file_bytes[arr..arr + vp.size], 4);
                let pname = ep.iter().find(|x| x.name == "ParameterName").map(|pn| painted_upk.resolve_name(read_i32_le(&painted_upk.file_bytes, arr + pn.value_offset))).unwrap_or("");
                assert_eq!(pname, "TrimColor", "Dominus Trim MIC parameter must be TrimColor");
            }
        }

        if pkg == "explosion_chndragon_SF.upk" {
                let pa = painted_upk
                    .exports
                    .iter()
                    .find(|e| painted_upk.export_class_name(e).starts_with("ProductAsset_GoalExplosion"))
                    .expect("Paper Dragon must have ProductAsset_GoalExplosion_TA");
                let props = find_export_properties(&painted_upk, pa, &painted_upk.file_bytes);
                let fx_val = props.iter().find(|p| p.name == "GoalExplosionFX")
                    .map(|p| read_i32_le(&painted_upk.file_bytes, pa.serial_offset + p.value_offset))
                    .expect("Paper Dragon must have GoalExplosionFX");
                let pfx_val = props.iter().find(|p| p.name == "PaintedGoalExplosionFX")
                    .map(|p| read_i32_le(&painted_upk.file_bytes, pa.serial_offset + p.value_offset))
                    .expect("Paper Dragon must have PaintedGoalExplosionFX");
                assert_eq!(fx_val, pfx_val, "Paper Dragon GoalExplosionFX MUST point to PaintedGoalExplosionFX!");
                assert!(fx_val > 0);
                let dragon_mic = painted_upk
                    .exports
                    .iter()
                    .find(|e| painted_upk.export_class_name(e) == "MaterialInstanceConstant" && painted_upk.resolve_name(e.object_name) == "Explosion_PaperDragon_MIC")
                    .expect("Paper Dragon must have Explosion_PaperDragon_MIC");
                let mprops = find_export_properties(&painted_upk, dragon_mic, &painted_upk.file_bytes);
                let vp = mprops.iter().find(|p| p.name == "VectorParameterValues").expect("VectorParameterValues must exist on Explosion_PaperDragon_MIC");
                let arr = dragon_mic.serial_offset + vp.value_offset;
                let cnt = read_i32_le(&painted_upk.file_bytes, arr).clamp(0, 16);
                let mut rel = 4usize;
                let mut verified_custom_color = false;
                for _ in 0..cnt {
                    let (nr, ep, ok) = parse_property_stream(&painted_upk, &painted_upk.file_bytes[arr..arr + vp.size], rel);
                    if !ok && ep.is_empty() { break; }
                    if let (Some(pn), Some(pv)) = (
                        ep.iter().find(|x| x.name == "ParameterName"),
                        ep.iter().find(|x| x.name == "ParameterValue" && x.size == 16),
                    ) {
                        if painted_upk.resolve_name(read_i32_le(&painted_upk.file_bytes, arr + pn.value_offset)) == "CustomColor" {
                            let r = read_f32_le(&painted_upk.file_bytes, arr + pv.value_offset);
                            let g = read_f32_le(&painted_upk.file_bytes, arr + pv.value_offset + 4);
                            let b = read_f32_le(&painted_upk.file_bytes, arr + pv.value_offset + 8);
                            let (cr, cg, cb) = cobalt.linear_rgb;
                            assert!(
                                (r - cr).abs() < 0.03 && (g - cg).abs() < 0.03 && (b - cb).abs() < 0.03,
                                "Paper Dragon CustomColor ({r},{g},{b}) must match Cobalt ({cr},{cg},{cb})!"
                            );
                            verified_custom_color = true;
                        }
                    }
                    rel = nr;
                }
                assert!(verified_custom_color, "Must verify Paper Dragon CustomColor updated to Cobalt!");
            }

            assert!(restore_single_package(&sandbox.join(pkg)));
        }

    // 5b. Custom Decal + Real AlphaConsole .ZIP Decal (Diffuse + 1_Diffuse_Skin + 2_Diffuse_Skin_Mask)
    println!("{}", inject_custom_decal(&sandbox, "skin_octane_alpine_esports_SF.upk", &custom_decal_test).unwrap());
    assert!(restore_single_package(&sandbox.join("skin_octane_alpine_esports_SF.upk")));

    // Test AlphaConsole Decal .ZIP (exact schema: BodyID=23, SkinID=8945, Body={Diffuse, 1_Diffuse_Skin, 2_Diffuse_Skin_Mask})
    let real_ac_decal_zip = std::env::var_os("RL_TEST_DECAL_ZIP").map(PathBuf::from);
    let synth_ac_decal_zip = sandbox.join("TestAlphaConsoleDecal.zip");
    {
        let mut skin_im = RgbaImage::from_pixel(512, 512, Rgba([40, 140, 255, 255]));
        for y in 0..64u32 {
            for x in 0..64u32 {
                skin_im.put_pixel(x, y, Rgba([0, 0, 0, 0]));
            }
        }
        let mut skin_png = Vec::new();
        skin_im.write_to(&mut std::io::Cursor::new(&mut skin_png), image::ImageFormat::Png).unwrap();
        let mask_im = RgbaImage::from_pixel(512, 512, Rgba([0, 200, 140, 255]));
        let mut mask_png = Vec::new();
        mask_im.write_to(&mut std::io::Cursor::new(&mut mask_png), image::ImageFormat::Png).unwrap();
        let f = File::create(&synth_ac_decal_zip).unwrap();
        let mut zw = ZipWriter::new(f);
        zw.start_file("TestDecal.json", SimpleFileOptions::default()).unwrap();
        zw.write_all(br#"{"TestDecal":{"BodyID":23,"SkinID":8945,"Body":{"Diffuse":"Decal.png","1_Diffuse_Skin":"Decal.png","2_Diffuse_Skin_Mask":"Mask.png"}}}"#).unwrap();
        zw.start_file("Decal.png", SimpleFileOptions::default()).unwrap();
        zw.write_all(&skin_png).unwrap();
        zw.start_file("Mask.png", SimpleFileOptions::default()).unwrap();
        zw.write_all(&mask_png).unwrap();
        zw.finish().unwrap();
    }
    let ac_decal_src = real_ac_decal_zip
        .as_deref()
        .filter(|p| p.is_file())
        .unwrap_or(synth_ac_decal_zip.as_path());
    let ac_decal_packs = parse_alphaconsole_decal_source(ac_decal_src).expect("Must parse AlphaConsole Decal .zip");
    assert!(!ac_decal_packs.is_empty());
    let ac_pack = &ac_decal_packs[0];
    assert_eq!(ac_pack.body_id, Some(23));
    assert_eq!(ac_pack.skin_id, Some(8945));
    assert!(ac_pack.skin_img.is_some() && ac_pack.mask_img.is_some() && ac_pack.diffuse_base_img.is_some());
    let skin_id_item = find_catalog_item_by_id(&catalog, 8945);
    assert!(skin_id_item.is_some(), "Catalog must resolve AlphaConsole SkinID 8945");

    // Inject onto BOTH native diffuse SkinID 8945 AND common starter decal Skin_Octane_Stripes_SF.upk!
    let assert_inline_tex_valid = |upk: &rl_swapper::upk_engine::UnpackedUpk, exp_idx: usize, expect_dxt5: bool, expect_alpha_range: bool| {
        let exp = &upk.exports[exp_idx];
        let ser = &upk.file_bytes[exp.serial_offset..exp.serial_offset + exp.serial_size];
        let props = find_export_properties(upk, exp, &upk.file_bytes);
        assert!(props.iter().any(|p| p.name == "NeverStream" && ser[p.value_offset] == 1),
            "{} must have NeverStream=1", upk.resolve_name(exp.object_name));
        assert!(!props.iter().any(|p| p.name == "TextureFileCacheName" || p.name == "FirstResourceMemMip"),
            "{} must strip TFC streaming props", upk.resolve_name(exp.object_name));
        if expect_dxt5 {
            let fmt_p = props.iter().find(|p| p.name == "Format").expect("Format prop missing");
            let fmt_nm = upk.resolve_name(read_i32_le(ser, fmt_p.value_offset));
            assert_eq!(fmt_nm, "PF_DXT5", "{} must preserve alpha via PF_DXT5", upk.resolve_name(exp.object_name));
        }
        let img = decode_texture2d_export(upk, exp_idx, None).expect("Must decode inline without .tfc");
        if expect_alpha_range {
            let (mut min_a, mut max_a) = (255u8, 0u8);
            for p in img.pixels() { min_a = min_a.min(p[3]); max_a = max_a.max(p[3]); }
            assert!(min_a <= 30 && max_a >= 225, "{} DXT5 alpha range expected [0..255], got [{}..{}]",
                upk.resolve_name(exp.object_name), min_a, max_a);
        }
    };

    for target_decal in ["skin_octane_badbeats_SF.upk", "Skin_Octane_Stripes_SF.upk", "skin_octane_leaves_SF.upk"] {
        if !sandbox.join(target_decal).exists() { continue; }
        let orig_bytes = std::fs::read(sandbox.join(target_decal)).unwrap();
        let orig_pfx = parse_upk_prefix(&orig_bytes).unwrap();
        let orig_meta8 = read_i32_le(&orig_bytes, orig_pfx.compression_meta_off + 8);
        let msg = inject_alphaconsole_decal(&sandbox, target_decal, ac_pack, Some("skin_octane_badbeats_SF.upk")).unwrap();
        println!("AlphaConsole Decal verified [{}]: {}", target_decal, msg);
        let mod_bytes = std::fs::read(sandbox.join(target_decal)).unwrap();
        let mod_pfx = parse_upk_prefix(&mod_bytes).unwrap();
        assert_eq!(read_i32_le(&mod_bytes, mod_pfx.compression_meta_off + 8), orig_meta8,
            "{} must preserve Psyonix compression_meta+8", target_decal);
        let (dupk, _) = unpack_upk_from_disk(&sandbox.join(target_decal)).expect("Injected AlphaConsole decal must unpack cleanly");
        assert!(dupk.find_name_index("2_Diffuse_Skin_Mask").is_some(), "{} must keep 2_Diffuse_Skin_Mask name", target_decal);
        // Must NOT keep forced locked colors/finishes when unlocked 2-mask donor (leaves) exists!
        if sandbox.join("skin_octane_leaves_SF.upk").exists() {
            let pa = dupk.exports.iter().find(|e| dupk.export_class_name(e) == "ProductAsset_Skin_TA").unwrap();
            let pa_props = find_export_properties(&dupk, pa, &dupk.file_bytes);
            assert!(!pa_props.iter().any(|p| p.name == "ForcedTeamColors" || p.name == "bTeamFinishDisabled"),
                "{} must upgrade to unlocked 2-mask shader instead of locked badbeats!", target_decal);
        }
        let mut saw_skin = false;
        let mut saw_mask = false;
        let mut saw_diff = false;
        for (i, exp) in dupk.exports.iter().enumerate() {
            if dupk.export_class_name(exp) != "MaterialInstanceConstant" { continue; }
            for pr in find_export_properties(&dupk, exp, &dupk.file_bytes) {
                if pr.name == "TextureParameterValues" && pr.tag_type == "ArrayProperty" {
                    let arr = exp.serial_offset + pr.value_offset;
                    let cnt = read_i32_le(&dupk.file_bytes, arr).clamp(0, 32);
                    let mut rel = 4usize;
                    for _ in 0..cnt {
                        let (nr, ep, ok) = parse_property_stream(&dupk, &dupk.file_bytes[arr..arr + pr.size], rel);
                        if !ok && ep.is_empty() { break; }
                        let pn = ep.iter().find(|x| x.name == "ParameterName").map(|x| dupk.resolve_name(read_i32_le(&dupk.file_bytes, arr + x.value_offset))).unwrap_or("");
                        let pv = ep.iter().find(|x| x.name == "ParameterValue").map(|x| read_i32_le(&dupk.file_bytes, arr + x.value_offset)).unwrap_or(0);
                        let guid_nz = ep.iter().find(|x| x.name == "ExpressionGUID" && x.size == 16)
                            .map(|g| dupk.file_bytes[arr + g.value_offset..arr + g.value_offset + 16].iter().any(|&b| b != 0))
                            .unwrap_or(false);
                        if pv > 0 {
                            let tidx = (pv - 1) as usize;
                            if pn == "1_Diffuse_Skin" {
                                assert!(guid_nz, "{} 1_Diffuse_Skin must keep non-zero ExpressionGUID!", target_decal);
                                assert_inline_tex_valid(&dupk, tidx, true, ac_pack.skin_img.as_ref().unwrap().pixels().any(|p| p[3] < 240));
                                saw_skin = true;
                            }
                            if pn == "2_Diffuse_Skin_Mask" {
                                assert!(dupk.exports[tidx].outer_index > 0, "Mask export must preserve subpackage OuterIndex");
                                if sandbox.join("skin_octane_leaves_SF.upk").exists() {
                                    assert!(guid_nz, "{} 2_Diffuse_Skin_Mask must have native compiled non-zero ExpressionGUID!", target_decal);
                                    assert!(!dupk.resolve_name(dupk.exports[tidx].object_name).eq_ignore_ascii_case("RLSwapper_AC_Mask"),
                                        "{} must replace native 2_Diffuse_Skin_Mask directly without fake added export!", target_decal);
                                }
                                assert_inline_tex_valid(&dupk, tidx, true, false);
                                saw_mask = true;
                            }
                            if pn == "Diffuse" {
                                assert_inline_tex_valid(&dupk, tidx, true, false);
                                saw_diff = true;
                            }
                        }
                        rel = nr;
                    }
                }
            }
            let _ = i;
        }
        assert!(saw_skin && saw_mask && saw_diff, "{} must bind inline 1_Diffuse_Skin, 2_Diffuse_Skin_Mask & Diffuse!", target_decal);
        assert!(restore_single_package(&sandbox.join(target_decal)));
    }

    // Standalone transparent PNG onto Skin_Octane_Stripes_SF.upk MUST upgrade onto 1_Diffuse_Skin AND synthesize neutral 2_Diffuse_Skin_Mask!
    if sandbox.join("Skin_Octane_Stripes_SF.upk").exists() {
        let mut grad_png = RgbaImage::new(512, 512);
        for (x, y, px) in grad_png.enumerate_pixels_mut() {
            *px = Rgba([220, 40, 120, ((x + y) % 256) as u8]);
        }
        let png_msg = inject_custom_decal(&sandbox, "Skin_Octane_Stripes_SF.upk", &grad_png).unwrap();
        println!("Standalone PNG onto Stripes verified: {}", png_msg);
        let (pupk, _) = unpack_upk_from_disk(&sandbox.join("Skin_Octane_Stripes_SF.upk")).unwrap();
        assert!(pupk.find_name_index("1_Diffuse_Skin").is_some(), "Stripes PNG must auto-upgrade to 1_Diffuse_Skin shader");
        let mut skin_tex_idx = None;
        let mut mask_tex_idx = None;
        for exp in pupk.exports.iter().filter(|e| {
            pupk.export_class_name(e) == "MaterialInstanceConstant"
                && !pupk.resolve_name(e.object_name).to_lowercase().contains("chassis")
        }) {
            let props = find_export_properties(&pupk, exp, &pupk.file_bytes);
            if let Some(tp) = props.iter().find(|p| p.name == "TextureParameterValues") {
                let arr = exp.serial_offset + tp.value_offset;
                let cnt = read_i32_le(&pupk.file_bytes, arr).clamp(0, 32);
                let mut rel = 4usize;
                for _ in 0..cnt {
                    let (nr, ep, ok) = parse_property_stream(&pupk, &pupk.file_bytes[arr..arr + tp.size], rel);
                    if !ok && ep.is_empty() { break; }
                    let pn = ep.iter().find(|x| x.name == "ParameterName").map(|x| pupk.resolve_name(read_i32_le(&pupk.file_bytes, arr + x.value_offset))).unwrap_or("");
                    let pv = ep.iter().find(|x| x.name == "ParameterValue").map(|x| read_i32_le(&pupk.file_bytes, arr + x.value_offset)).unwrap_or(0);
                    if pv > 0 {
                        if pn == "1_Diffuse_Skin" { skin_tex_idx = Some((pv - 1) as usize); }
                        if pn == "2_Diffuse_Skin_Mask" { mask_tex_idx = Some((pv - 1) as usize); }
                    }
                    rel = nr;
                }
            }
        }
        let s_idx = skin_tex_idx.expect("Stripes PNG must bind 1_Diffuse_Skin parameter");
        assert_inline_tex_valid(&pupk, s_idx, true, true);
        if let Some(m_idx) = mask_tex_idx {
            assert_inline_tex_valid(&pupk, m_idx, true, true);
        }
        assert!(restore_single_package(&sandbox.join("Skin_Octane_Stripes_SF.upk")));
    }

    if sandbox.join("Skin_MuscleCar_Stripes_SF.upk").exists() {
        let dom_msg = inject_custom_decal(&sandbox, "Skin_MuscleCar_Stripes_SF.upk", &custom_decal_test).unwrap();
        println!("Dominus Stripes custom PNG verified: {}", dom_msg);
        let (dupk, _) = unpack_upk_from_disk(&sandbox.join("Skin_MuscleCar_Stripes_SF.upk")).unwrap();
        assert!(dupk.find_name_index("2_Diffuse_Skin_Mask").is_some());
        assert!(restore_single_package(&sandbox.join("Skin_MuscleCar_Stripes_SF.upk")));
    }

    if sandbox.join("skin_grain_aloha_SF.upk").exists() {
        let f_msg = inject_custom_decal(&sandbox, "skin_grain_aloha_SF.upk", &custom_decal_test).unwrap();
        println!("Fennec Aloha inline conversion: {}", f_msg);
        let (f_upk, _) = unpack_upk_from_disk(&sandbox.join("skin_grain_aloha_SF.upk")).unwrap();
        let any_inline = f_upk.exports.iter().enumerate().any(|(i, e)| {
            f_upk.export_class_name(e) == "Texture2D"
                && decode_texture2d_export(&f_upk, i, None).map_or(false, |im| im.width() >= 512)
        });
        assert!(any_inline, "Must decode inline converted Fennec decal");
        assert!(restore_single_package(&sandbox.join("skin_grain_aloha_SF.upk")));
    }

    // 5b-extra: Verify Avatar Border Catalog Normalization, Name Parity & 177/177 Preview/DXT5 Injection
    let ab_catalog_items: Vec<_> = catalog.iter().filter(|c| c.slot == "Avatar Border").collect();
    assert!(
        ab_catalog_items.iter().any(|c| c.product == "Azumaya" && c.asset_package == "AvatarBorder_2019ChineseValentinesDay_SF.upk"),
        "PlayerAvatarBorder items (Azumaya, etc.) must normalize into 'Avatar Border' slot!"
    );
    assert!(
        ab_catalog_items.iter().any(|c| c.product == "Sunset Effigy LTE 01"),
        "Sunset Effigy LTE 01 must be in 'Avatar Border' slot!"
    );
    let custom_borders = list_avatar_border_packages_with_catalog(real_cooked, &catalog);
    assert!(
        custom_borders.len() >= 177,
        "Expected all 177 Avatar Borders (incl. obfuscated Sunset Effigy), got {}",
        custom_borders.len()
    );
    assert!(custom_borders.iter().any(|(lbl, _)| lbl == "Twinkle Box"), "Custom Avatar must use catalog name 'Twinkle Box', not 'Wispy'!");
    assert!(custom_borders.iter().any(|(lbl, _)| lbl == "XP Level 25"), "Custom Avatar must use catalog name 'XP Level 25', not 'XPTier1'!");
    assert!(custom_borders.iter().any(|(lbl, _)| lbl == "Sunset Effigy LTE 01"));
    assert_eq!(list_avatar_border_packages(real_cooked).len(), custom_borders.len());

    // Confirm every single Avatar Border on disk renders a valid 120x120 preview cutout (previously 60/176 returned None!)
    let mut missing_previews = Vec::new();
    for (lbl, pkg) in &custom_borders {
        if compose_pfp_with_border_preview(real_cooked, pkg, None)
            .map_or(true, |im| im.dimensions() != (120, 120))
        {
            missing_previews.push(format!("{} ({})", lbl, pkg));
        }
    }
    assert!(
        missing_previews.is_empty(),
        "All {} Avatar Borders must preview cleanly! Failed: {:?}",
        custom_borders.len(),
        missing_previews
    );
    println!("All {} Avatar Borders verified for catalog name parity & live preview!", custom_borders.len());

    for border_pkg in [
        "AvatarBorder_Default_SF.upk",
        "AvatarBorder_5thAnniversary_SF.upk",
        "AvatarBorder_2019ChineseValentinesDay_SF.upk",
        "AvatarBorder_Frosty_SF.upk",
        "avatarborder_s14rewardgrandchampion_SF.upk",
        "paca6f52bcd3aad77dd19_SF.upk",
    ] {
        if !sandbox.join(border_pkg).exists() {
            continue;
        }
        let pfp_comp = compose_pfp_with_border_preview(&sandbox, border_pkg, Some(&custom_decal_test))
            .expect("Must composite PFP inside AvatarBorder cutout");
        assert_eq!(pfp_comp.dimensions(), (120, 120));
        assert!(pfp_comp.get_pixel(0, 0)[3] <= 8, "{} outer corner (0,0) must not leak opaque background (got {})!", border_pkg, pfp_comp.get_pixel(0, 0)[3]);
        assert!(pfp_comp.get_pixel(119, 119)[3] <= 8, "{} outer corner (119,119) must not leak opaque background (got {})!", border_pkg, pfp_comp.get_pixel(119, 119)[3]);
        assert!(pfp_comp.get_pixel(5, 60)[3] <= 8, "{} outside left border (5,60) must stay transparent (got {})!", border_pkg, pfp_comp.get_pixel(5, 60)[3]);
        assert!(pfp_comp.get_pixel(60, 60)[3] > 200, "{} center PFP pixel must be opaque!", border_pkg);
    }
    println!("{}", inject_custom_pfp(&sandbox, "AvatarBorder_Default_SF.upk", &custom_decal_test).unwrap());
    assert!(is_avatar_border_modified(&sandbox.join("AvatarBorder_Default_SF.upk")));
    assert!(restore_single_package(&sandbox.join("AvatarBorder_Default_SF.upk")));
    assert!(!is_avatar_border_modified(&sandbox.join("AvatarBorder_Default_SF.upk")));

    // Also test DXT5 / 128x128 / obfuscated border PFP injection + restore!
    for dxt_border in [
        "AvatarBorder_2019ChineseValentinesDay_SF.upk",
        "AvatarBorder_Frosty_SF.upk",
        "avatarborder_s14rewardgrandchampion_SF.upk",
        "paca6f52bcd3aad77dd19_SF.upk",
    ] {
        if !sandbox.join(dxt_border).exists() {
            continue;
        }
        println!("{}", inject_custom_pfp(&sandbox, dxt_border, &custom_decal_test).unwrap());
        let (bupk, _) = unpack_upk_from_disk(&sandbox.join(dxt_border)).unwrap();
        let any_filled = bupk.exports.iter().enumerate().any(|(idx, e)| {
            bupk.export_class_name(e) == "Texture2D"
                && !bupk.full_export_path(idx + 1).to_lowercase().contains("thumb_")
                && decode_texture2d_export(&bupk, idx, None)
                    .map_or(false, |im| im.get_pixel(im.width() / 2, im.height() / 2)[3] > 200)
        });
        assert!(any_filled, "{} must contain injected PFP center pixels after inject_custom_pfp!", dxt_border);
        assert!(restore_single_package(&sandbox.join(dxt_border)));
    }

    println!("{}", inject_custom_banner(&sandbox, &test_banner_pkg, &custom_decal_test).unwrap());
    assert!(restore_single_package(&sandbox.join(&test_banner_pkg)));

    // 5c. AlphaConsole Match Ball Injection (Diffuse + Normal into GameInfo_Soccar_SF.upk)
    let real_ac_ball_zip = std::env::var_os("RL_TEST_BALL_ZIP").map(PathBuf::from);
    let synth_ac_ball_zip = sandbox.join("TestAlphaConsoleBall.zip");
    {
        let mut diff_png = Vec::new();
        custom_decal_test.write_to(&mut std::io::Cursor::new(&mut diff_png), image::ImageFormat::Png).unwrap();
        let norm_im = RgbaImage::from_pixel(512, 512, Rgba([128, 128, 255, 255]));
        let mut norm_png = Vec::new();
        norm_im.write_to(&mut std::io::Cursor::new(&mut norm_png), image::ImageFormat::Png).unwrap();
        let f = File::create(&synth_ac_ball_zip).unwrap();
        let mut zw = ZipWriter::new(f);
        zw.start_file("TestBall.json", SimpleFileOptions::default()).unwrap();
        zw.write_all(br#"{"TestBall":{"Group":"","Params":{"Diffuse":"Ball.png","Normal":"BallN.png"}}}"#).unwrap();
        zw.start_file("Ball.png", SimpleFileOptions::default()).unwrap();
        zw.write_all(&diff_png).unwrap();
        zw.start_file("BallN.png", SimpleFileOptions::default()).unwrap();
        zw.write_all(&norm_png).unwrap();
        zw.finish().unwrap();
    }
    let ac_ball_src = real_ac_ball_zip
        .as_deref()
        .filter(|p| p.is_file())
        .unwrap_or(synth_ac_ball_zip.as_path());
    let ac_ball_packs = parse_alphaconsole_ball_source(ac_ball_src).expect("Must parse AlphaConsole Ball .zip");
    assert!(!ac_ball_packs.is_empty() && ac_ball_packs[0].normal_img.is_some());
    let orig_gi_raw = std::fs::read(sandbox.join("GameInfo_Soccar_SF.upk")).unwrap();
    let orig_gi_pfx = parse_upk_prefix(&orig_gi_raw).unwrap();
    let orig_gi_meta8 = read_i32_le(&orig_gi_raw, orig_gi_pfx.compression_meta_off + 8);
    let ball_msg = inject_alphaconsole_ball(&sandbox, &ac_ball_packs[0]).expect("AlphaConsole ball injection failed");
    println!("AlphaConsole Ball verified: {}", ball_msg);
    let mod_gi_raw = std::fs::read(sandbox.join("GameInfo_Soccar_SF.upk")).unwrap();
    let mod_gi_pfx = parse_upk_prefix(&mod_gi_raw).unwrap();
    assert_eq!(read_i32_le(&mod_gi_raw, mod_gi_pfx.compression_meta_off + 8), orig_gi_meta8,
        "GameInfo_Soccar_SF.upk must preserve compression_meta+8 == 8");
    let custom_ball_readback = extract_ball_texture_preview(&sandbox).expect("Must decode custom inline ball diffuse");
    assert!(custom_ball_readback.width() >= 512);
    let (re_ball_upk, _) = unpack_upk_from_disk(&sandbox.join("GameInfo_Soccar_SF.upk")).unwrap();
    for tex_path in ["Ball_Default_Textures.Textures.Ball_Default00_D", "Ball_Default_Textures.Textures.Ball_Default00_N"] {
        let tidx = re_ball_upk
            .exports
            .iter()
            .position(|e| re_ball_upk.full_export_path(e.index).eq_ignore_ascii_case(tex_path))
            .unwrap();
        assert_inline_tex_valid(&re_ball_upk, tidx, false, false);
    }
    assert!(restore_single_package(&sandbox.join("GameInfo_Soccar_SF.upk")));

    // 5d. Pure Normal Map Swapping (Single + Override All Default-Size Maps without touching Labs)
    let single_map_msg = swap_standard_map(&sandbox, "Stadium_P.upk", "Park_P.upk", false)
        .expect("Single arena swap Stadium_P -> Park_P failed");
    println!("Single Map Swap verified: {}", single_map_msg);
    assert!(unpack_upk_from_disk(&sandbox.join("Park_P.upk")).is_ok());

    let map_msg = swap_standard_map(&sandbox, "Stadium_P.upk", "Park_P.upk", true)
        .expect("Override All Default-Size Maps failed");
    println!("Override All Default-Size Maps verified: {}", map_msg);
    assert!(STANDARD_SOCCAR_MAPS.len() >= 90, "Expected 90+ playable arenas, got {}", STANDARD_SOCCAR_MAPS.len());
    assert!(STANDARD_SOCCAR_MAPS.iter().any(|m| m.is_default_size && m.category == "Custom Default Size" && m.upk_name == "UF_Day_P.upk"));
    assert!(STANDARD_SOCCAR_MAPS.iter().any(|m| !m.is_default_size && m.category == "Labs & Special" && m.upk_name == "Labs_CirclePillars_P.upk"));
    assert!(!sandbox.join("Labs_CirclePillars_P.upk.bak").exists(), "Override All Default-Size Maps must NOT overwrite Labs/Special arenas!");
    assert!(unpack_upk_from_disk(&sandbox.join("Stadium_P.upk")).is_ok());
    assert!(unpack_upk_from_disk(&sandbox.join("Park_P.upk")).is_ok());
    assert!(unpack_upk_from_disk(&sandbox.join("EuroStadium_Night_P.upk")).is_ok());
    assert!(reset_all_standard_maps(&sandbox) >= 3);

    // 5e. Epic Workshop Map Support + Google Drive Confirm Form + BakkesPlugins Detail Resolution
    let sample_html = r#"<form id="download-form" action="https://drive.usercontent.google.com/download" method="get"><input type="submit" value="Download anyway"/><input type="hidden" name="id" value="1MjqsUtNXVb3lPA-7A8EXIWCkWWkCe4fw"><input type="hidden" name="export" value="download"><input type="hidden" name="confirm" value="t"><input type="hidden" name="uuid" value="81a9a23b"></form>"#;
    let resolved_gd = extract_gdrive_confirm_url(sample_html).expect("Must parse Google Drive confirm form");
    assert!(resolved_gd.starts_with("https://drive.usercontent.google.com/download?"));
    assert!(resolved_gd.contains("confirm=t") && resolved_gd.contains("uuid=81a9a23b"));

    let bp_html = r#"<a href="/maps/138"><img src="https://cdn.bakkesplugins.com/uploads/preview.jpg"/><h2>Ice Rings</h2><p class="x">20 levels of aerial rings</p></a><a href="/maps?page=23">23</a>"#;
    let (bp_pages, bp_items) = parse_bakkesplugins_maps_html(bp_html);
    assert_eq!(bp_pages, 23);
    assert_eq!(bp_items.len(), 1);
    assert_eq!(bp_items[0].name, "Ice Rings");
    assert_eq!(
        extract_bakkesplugins_download_url(r#"<a href="https://cdn.bakkesplugins.com/uploads/abc-IceRings.zip">DL</a>"#).as_deref(),
        Some("https://cdn.bakkesplugins.com/uploads/abc-IceRings.zip")
    );

    // Simulate uncooked .udk (flags 0x20020001 -> PKG_COOKED bit 0x8 MUST remain 0!)
    let mut uncooked_bytes = fs::read(sandbox.join("Labs_Underpass_P.upk")).unwrap();
    if let Some((_, fp)) = read_fstring_at(&uncooked_bytes, 12) {
        write_u32_le(&mut uncooked_bytes, fp, 0x2002_0001);
    }
    let zip_path = sandbox.join("Test_Rings_Workshop.zip");
    {
        let f = File::create(&zip_path).unwrap();
        let mut zw = ZipWriter::new(f);
        zw.start_file("NestedFolder/Test_Rings_P.udk", SimpleFileOptions::default()).unwrap();
        zw.write_all(&uncooked_bytes).unwrap();
        zw.finish().unwrap();
    }
    let ws_entry = import_workshop_file_or_zip(&zip_path, Some("Verify Test Rings"), None).unwrap();
    let _deps = scan_upk_external_dependencies(&sandbox.join("Labs_Underpass_P.upk"), &sandbox);
    let (ws_msg, _) = load_workshop_map_into_underpass(&sandbox, &ws_entry).unwrap();
    let loaded_pfx = parse_upk_prefix(&fs::read(sandbox.join("Labs_Underpass_P.upk")).unwrap()).unwrap();
    assert_eq!(loaded_pfx.package_flags & 0x8, 0, "MUST NOT force PKG_COOKED onto uncooked .udk maps!");
    println!("Workshop verified: {}", ws_msg);
    let health = inspect_cooked_health(&sandbox);
    assert!(health.underpass_swapped);
    let _ = repair_cooked_health(&sandbox);
    assert!(restore_workshop_underpass(&sandbox));
    let _ = delete_library_map(&ws_entry);

    // 6. Verify Merged Online Workshop Catalog (Lethamyr + BakkesPlugins + Steam Workshop = 700+ maps!)
    let online_pg = fetch_lethamyr_catalog_page(1).expect("Lethamyr public map API must return page 1");
    assert!(!online_pg.items.is_empty());
    let all_online = fetch_all_online_workshop_maps(true).expect("Merged Lethamyr + BakkesPlugins + Steam catalog must succeed");
    let direct_cnt = all_online.iter().filter(|m| m.direct_download).count();
    let steam_cnt = all_online.iter().filter(|m| m.source.contains("Steam")).count();
    println!(
        "Verified Merged Online Workshop Catalog: {} unique maps ({} 1-click direct, {} Steam)!",
        all_online.len(),
        direct_cnt,
        steam_cnt
    );
    assert!(all_online.len() >= 700, "Expected 700+ online workshop maps, got {}", all_online.len());
    assert!(direct_cnt >= 350, "Expected 350+ 1-click direct download maps, got {}", direct_cnt);
    assert!(steam_cnt >= 300, "Expected 300+ Steam Workshop maps, got {}", steam_cnt);

    // 7. Verify Workshop Streaming Download Progress Callback over Local HTTP ZIP Stream
    {
        use std::io::Read as _;
        use std::net::TcpListener;
        let payload_zip = fs::read(&zip_path).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let pz_len = payload_zip.len();
        let srv = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut req_buf = [0u8; 1024];
                let _ = stream.read(&mut req_buf);
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/zip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    pz_len
                );
                let _ = stream.write_all(hdr.as_bytes());
                let _ = stream.write_all(&payload_zip);
                let _ = stream.flush();
            }
        });
        let fake_item = OnlineWorkshopItem {
            name: "Verify Progress Map".into(),
            short_desc: "Test progress callback".into(),
            download_url: format!("http://127.0.0.1:{}/VerifyProgress.zip", port),
            thumb_url: None,
            downloads: 0,
            likes: 0,
            source: "Test HTTP".into(),
            direct_download: true,
        };
        let mut saw_phases = Vec::new();
        let mut max_dl = 0u64;
        let mut saw_total = None;
        let dl_entry = download_online_workshop_map_with_progress(&fake_item, |dl, tot, phase| {
            max_dl = max_dl.max(dl);
            if tot.is_some() {
                saw_total = tot;
            }
            if saw_phases.last().map_or(true, |p: &String| p != phase) {
                saw_phases.push(phase.to_string());
            }
        })
        .expect("Streaming download progress test must succeed");
        let _ = srv.join();
        assert_eq!(saw_total, Some(pz_len as u64));
        assert_eq!(max_dl, pz_len as u64);
        assert!(saw_phases.iter().any(|p| p.contains("Downloading")));
        assert!(saw_phases.iter().any(|p| p.contains("Extracting")));
        let _ = delete_library_map(&dl_entry);
    }

    let _ = fs::remove_dir_all(&sandbox);
}

#[test]
fn test_alphaconsole_real_world_parity() {
    use rl_swapper::custom_studio::parse_alphaconsole_decal_source;
    use rl_swapper::upk_engine::extract_package_guid;
    use std::collections::HashSet;

    let Some(real_cooked_buf) = detect_test_cooked_dir() else {
        eprintln!("Skipping AlphaConsole parity test: Rocket League CookedPCConsole not found (set RL_COOKED_DIR to enable).");
        return;
    };
    let real_cooked = real_cooked_buf.as_path();
    let sandbox = std::env::temp_dir().join("rl_swapper_v4_ac_parity");
    let _ = fs::remove_dir_all(&sandbox);
    fs::create_dir_all(&sandbox).unwrap();

    for pkg in [
        "Skin_Octane_Flames_SF.upk",
        "Skin_Octane_Stripes_SF.upk",
        "skin_octane_dignitas_home_SF.upk",
        "skin_octane_leaves_SF.upk",
        "skin_octane_badbeats_SF.upk",
        "GameInfo_Soccar_SF.upk",
    ] {
        let vp = rl_swapper::upk_engine::vault_backup_path(&real_cooked.join(pkg));
        let src = if vp.is_file() { vp } else { real_cooked.join(pkg) };
        if src.is_file() {
            fs::copy(&src, sandbox.join(pkg)).unwrap();
        }
    }

    let assert_ta_valid = |label: &str, raw: &[u8], upk: &rl_swapper::upk_engine::UnpackedUpk, expected_tex_exps1: &[usize]| {
        let pfx = parse_upk_prefix(raw).unwrap();
        assert!(pfx.texture_allocations_off + 4 <= pfx.compression_meta_off, "{} bad TA bounds", label);
        let cnt = read_i32_le(raw, pfx.texture_allocations_off).max(0) as usize;
        let mut tpos = pfx.texture_allocations_off + 4;
        let mut seen = HashSet::new();
        let mut present = HashSet::new();
        for i in 0..cnt {
            let sx = read_i32_le(raw, tpos);
            let sy = read_i32_le(raw, tpos + 4);
            let mips = read_i32_le(raw, tpos + 8);
            let fmt = read_i32_le(raw, tpos + 12);
            let elen = read_i32_le(raw, tpos + 20).max(0) as usize;
            let mut exps = Vec::new();
            for k in 0..elen {
                let id = read_i32_le(raw, tpos + 24 + k * 4);
                assert!(id >= 0 && (id as usize) < upk.exports.len(), "{} TA[{i}] oob exp id {id}", label);
                assert!(seen.insert(id), "{} duplicate TA exp id {id} in TA[{i}]", label);
                present.insert(id as usize);
                exps.push(id);
            }
            if exps.iter().any(|&id| expected_tex_exps1.contains(&(id as usize + 1))) {
                assert!(sx >= 512 && sy >= 512 && mips >= 1 && (fmt == 5 || fmt == 7),
                    "{} updated tex TA[{i}] invalid {}x{} mips={} fmt={}", label, sx, sy, mips, fmt);
            }
            tpos += 24 + elen * 4;
        }
        assert_eq!(tpos, pfx.compression_meta_off, "{} TA length mismatch", label);
        for &e1 in expected_tex_exps1 {
            assert!(present.contains(&(e1 - 1)), "{} modified tex Exp[{}] missing from TA!", label, e1);
        }
    };

    // Helper to synthesize portable AlphaConsole ZIPs matching exact real-world schemas when optional env paths aren't set
    let synth_tex_png = |rgba: [u8; 4]| -> Vec<u8> {
        let im = RgbaImage::from_pixel(1024, 1024, Rgba(rgba));
        let mut buf = Vec::new();
        im.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png).unwrap();
        buf
    };

    // 1. 2-Mask Esports Donor (SkinID: 8273 -> dignitas_home) in-place FOLLOWED BY Flames (must use PRISTINE dignitas_home!)
    let synth_itachi_zip = sandbox.join("SynthTwoMask8273.zip");
    {
        let f = File::create(&synth_itachi_zip).unwrap();
        let mut zw = ZipWriter::new(f);
        zw.start_file("Decal8273.json", SimpleFileOptions::default()).unwrap();
        zw.write_all(br#"{"Decal8273":{"BodyID":23,"SkinID":8273,"Body":{"1_Diffuse_Skin":"skin.png","2_Diffuse_Skin_Mask":"mask.png"}}}"#).unwrap();
        zw.start_file("skin.png", SimpleFileOptions::default()).unwrap();
        zw.write_all(&synth_tex_png([40, 120, 240, 255])).unwrap();
        zw.start_file("mask.png", SimpleFileOptions::default()).unwrap();
        zw.write_all(&synth_tex_png([255, 0, 0, 255])).unwrap();
        zw.finish().unwrap();
    }
    let itachi_zip_opt = std::env::var_os("RL_TEST_ITACHI_ZIP").map(PathBuf::from).filter(|p| p.is_file());
    let itachi_zip = itachi_zip_opt.as_deref().unwrap_or(synth_itachi_zip.as_path());
    if itachi_zip.is_file() && sandbox.join("skin_octane_dignitas_home_SF.upk").is_file() {
        let packs = parse_alphaconsole_decal_source(itachi_zip).unwrap();
        let pk = &packs[0];
        let orig_dig_cnt = unpack_upk_from_disk(&sandbox.join("skin_octane_dignitas_home_SF.upk")).unwrap().0.exports.len();
        let orig_flames_raw = fs::read(sandbox.join("Skin_Octane_Flames_SF.upk")).unwrap();
        let orig_flames_guid = extract_package_guid(&orig_flames_raw);

        // Apply to dignitas_home first -> MUST stay in-place without rewiring over leaves!
        let m1 = inject_alphaconsole_decal(&sandbox, "skin_octane_dignitas_home_SF.upk", pk, Some("skin_octane_dignitas_home_SF.upk")).unwrap();
        println!("TwoMask8273 -> dignitas_home: {}", m1);
        let (du, draw) = unpack_upk_from_disk(&sandbox.join("skin_octane_dignitas_home_SF.upk")).unwrap();
        assert_eq!(du.exports.len(), orig_dig_cnt, "dignitas_home (8273) must stay native in-place");
        assert_ta_valid("TwoMask->dignitas", &draw, &du, &[136, 137]);

        // Now apply -> Flames while modded dignitas_home sits in sandbox! Must pull PRISTINE dignitas_home,
        // keep clean Skin_Octane_Flames root package, preserve Flames PackageGuid, zero duplicate Package offsets!
        let m2 = inject_alphaconsole_decal(&sandbox, "Skin_Octane_Flames_SF.upk", pk, Some("skin_octane_dignitas_home_SF.upk")).unwrap();
        println!("TwoMask8273 -> Flames (with modded dignitas present): {}", m2);
        let (fu, fraw) = unpack_upk_from_disk(&sandbox.join("Skin_Octane_Flames_SF.upk")).unwrap();
        assert_eq!(extract_package_guid(&fraw), orig_flames_guid, "Flames must preserve target PackageGuid");
        let root_pkgs: Vec<String> = fu.exports.iter()
            .filter(|e| fu.export_class_name(e) == "Package" && e.outer_index == 0)
            .map(|e| fu.resolve_name(e.object_name).to_lowercase())
            .collect();
        assert!(root_pkgs.contains(&"skin_octane_flames".to_string()), "Flames root Package must be Skin_Octane_Flames: {:?}", root_pkgs);
        assert!(!root_pkgs.contains(&"skin_octane_leaves".to_string()), "Must NOT leak skin_octane_leaves root Package!");
        assert!(!root_pkgs.contains(&"skin_octane_dignitas_home".to_string()), "Must NOT leak donor dignitas_home root Package!");
        let pa = fu.exports.iter().find(|e| fu.export_class_name(e) == "ProductAsset_Skin_TA").unwrap();
        assert_eq!(fu.full_export_path(pa.index).to_lowercase(), "skin_octane_flames.skin_octane_flames");
        assert!(decode_texture2d_export(&fu, 135, Some(&sandbox)).is_some());
        assert!(decode_texture2d_export(&fu, 136, Some(&sandbox)).is_some());
        assert_ta_valid("TwoMask->Flames", &fraw, &fu, &[136, 137]);
        assert!(restore_single_package(&sandbox.join("skin_octane_dignitas_home_SF.upk")));
        assert!(restore_single_package(&sandbox.join("Skin_Octane_Flames_SF.upk")));
    }

    // 2. Legacy Diffuse+Skin (SkinID: 0) -> Flames natively in-place
    let synth_legacy_zip = sandbox.join("SynthLegacyDiffuseSkin.zip");
    {
        let f = File::create(&synth_legacy_zip).unwrap();
        let mut zw = ZipWriter::new(f);
        zw.start_file("LegacyOctane.json", SimpleFileOptions::default()).unwrap();
        zw.write_all(br#"{"LegacyOctane":{"BodyID":23,"SkinID":0,"Body":{"Diffuse":"diff.png","Skin":"skin.png"}}}"#).unwrap();
        zw.start_file("diff.png", SimpleFileOptions::default()).unwrap();
        zw.write_all(&synth_tex_png([24, 24, 30, 255])).unwrap();
        zw.start_file("skin.png", SimpleFileOptions::default()).unwrap();
        zw.write_all(&synth_tex_png([220, 40, 80, 255])).unwrap();
        zw.finish().unwrap();
    }
    let violent_zip_opt = std::env::var_os("RL_TEST_VIOLENT_ZIP").map(PathBuf::from).filter(|p| p.is_file());
    let violent_zip = violent_zip_opt.as_deref().unwrap_or(synth_legacy_zip.as_path());
    if violent_zip.is_file() && sandbox.join("Skin_Octane_Flames_SF.upk").is_file() {
        let packs = parse_alphaconsole_decal_source(violent_zip).unwrap();
        let vm = inject_alphaconsole_decal(&sandbox, "Skin_Octane_Flames_SF.upk", &packs[0], None).unwrap();
        println!("Legacy Diffuse+Skin -> Flames: {}", vm);
        let (vu, vraw) = unpack_upk_from_disk(&sandbox.join("Skin_Octane_Flames_SF.upk")).unwrap();
        assert!(vu.exports.iter().any(|e| vu.export_class_name(e) == "MaterialInstanceConstant"
            && vu.full_export_path(e.index).eq_ignore_ascii_case("Skin_Octane_Flames.MIC_Body_Octane_Flames")),
            "Legacy Diffuse+Skin on Flames must stay native MIC_Body_Octane_Flames!");
        let new_diff_idx = vu.exports.iter().position(|e| vu.export_class_name(e) == "Texture2D"
            && vu.resolve_name(e.object_name) == "RLSwapper_AC_Diffuse")
            .expect("Must append RLSwapper_AC_Diffuse Texture2D for Diffuse");
        assert_eq!(decode_texture2d_export(&vu, new_diff_idx, Some(&sandbox)).unwrap().dimensions(), (1024, 1024));
        assert_ta_valid("Legacy->Flames", &vraw, &vu, &[104, new_diff_idx + 1]);
        assert!(restore_single_package(&sandbox.join("Skin_Octane_Flames_SF.upk")));
    }

    // 3. Match Ball with Alpha=113 Normal Map -> GameInfo_Soccar_SF.upk (Ball_Default00_N MUST stay PF_DXT1 TC_Normalmap + valid TA!)
    let synth_ball_a113_zip = sandbox.join("SynthBallNormalAlpha113.zip");
    {
        let f = File::create(&synth_ball_a113_zip).unwrap();
        let mut zw = ZipWriter::new(f);
        zw.start_file("BallA113.json", SimpleFileOptions::default()).unwrap();
        zw.write_all(br#"{"BallA113":{"Group":"","Params":{"Diffuse":"ball_d.png","Normal":"ball_n.png"}}}"#).unwrap();
        zw.start_file("ball_d.png", SimpleFileOptions::default()).unwrap();
        zw.write_all(&synth_tex_png([140, 40, 220, 255])).unwrap();
        zw.start_file("ball_n.png", SimpleFileOptions::default()).unwrap();
        zw.write_all(&synth_tex_png([128, 128, 255, 113])).unwrap();
        zw.finish().unwrap();
    }
    let psyonix_ball_opt = std::env::var_os("RL_TEST_PSYONIX_BALL_ZIP").map(PathBuf::from).filter(|p| p.is_file());
    let psyonix_ball_zip = psyonix_ball_opt.as_deref().unwrap_or(synth_ball_a113_zip.as_path());
    if psyonix_ball_zip.is_file() && sandbox.join("GameInfo_Soccar_SF.upk").is_file() {
        let bpacks = parse_alphaconsole_ball_source(psyonix_ball_zip).unwrap();
        let bm = inject_alphaconsole_ball(&sandbox, &bpacks[0]).unwrap();
        println!("Ball PF_DXT1 Normal verified: {}", bm);
        let (bu, braw) = unpack_upk_from_disk(&sandbox.join("GameInfo_Soccar_SF.upk")).unwrap();
        let n_exp = &bu.exports[2208]; // Exp[2209] Ball_Default00_N
        let n_props = find_export_properties(&bu, n_exp, &bu.file_bytes);
        let n_slice = &bu.file_bytes[n_exp.serial_offset..n_exp.serial_offset + n_exp.serial_size];
        let n_fmt = n_props.iter().find(|p| p.name == "Format")
            .map(|p| bu.resolve_name(read_i32_le(n_slice, p.value_offset))).unwrap_or("");
        assert_eq!(n_fmt, "PF_DXT1", "TC_Normalmap Ball_Default00_N MUST remain PF_DXT1 (not DXT5n)!");
        assert_eq!(decode_texture2d_export(&bu, 2207, Some(&sandbox)).unwrap().dimensions(), (1024, 1024));
        assert_eq!(decode_texture2d_export(&bu, 2208, Some(&sandbox)).unwrap().dimensions(), (1024, 1024));
        assert_ta_valid("PsyonixBall", &braw, &bu, &[2208, 2209]);
        assert!(restore_single_package(&sandbox.join("GameInfo_Soccar_SF.upk")));
    }

    let _ = fs::remove_dir_all(&sandbox);
}
