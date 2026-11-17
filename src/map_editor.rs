//! Official 96-Arena Rocket League Map Swapper (Standard, Custom Default Size, Labs & Special)
//! and Parallel "Override All Default-Size Maps" Re-Key Engine.

use crate::icon_system::extract_texture_from_upk;
use crate::upk_engine::{
    aes_ctr_crypt, aes_ecb_decrypt, aes_ecb_encrypt, bak_path_for, clear_rl_texture_cache,
    decrypt_upk_header, ensure_pristine_before_swap, get_appdata_dir, parse_chunk_table,
    parse_upk_prefix, restore_single_package,
};
use image::{imageops::FilterType, DynamicImage, Rgba, RgbaImage};
use rayon::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct StandardMapSpec {
    pub upk_name: &'static str,
    pub display_name: &'static str,
    pub arena_family: &'static str,
    pub variant_label: &'static str,
    pub map_image_upk: &'static str,
    pub is_default_size: bool,
    pub category: &'static str,
}

pub const STANDARD_SOCCAR_MAPS: &[StandardMapSpec] = &[
    // ── Standard 57 Competitive Arenas ────────────────────────────────────────
    StandardMapSpec { upk_name: "Park_P.upk",                               display_name: "Beckwith Park",                       arena_family: "Beckwith Park",     variant_label: "Day",             map_image_upk: "Park_P_MapImage_SF.upk",                           is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "Park_Night_P.upk",                         display_name: "Beckwith Park (Midnight)",            arena_family: "Beckwith Park",     variant_label: "Midnight",        map_image_upk: "Park_Night_P_MapImage_SF.upk",                     is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "Park_Rainy_P.upk",                         display_name: "Beckwith Park (Stormy)",              arena_family: "Beckwith Park",     variant_label: "Stormy",          map_image_upk: "Park_Rainy_P_MapImage_SF.upk",                     is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "Park_Snowy_P.upk",                         display_name: "Beckwith Park (Snowy)",               arena_family: "Beckwith Park",     variant_label: "Snowy",           map_image_upk: "park_snowy_p_MapImage_SF.upk",                     is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "Stadium_P.upk",                            display_name: "DFH Stadium",                         arena_family: "DFH Stadium",       variant_label: "Day",             map_image_upk: "stadium_p_MapImage_SF.upk",                        is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "Stadium_Day_P.upk",                        display_name: "DFH Stadium (Day)",                   arena_family: "DFH Stadium",       variant_label: "Sunny",           map_image_upk: "stadium_day_p_MapImage_SF.upk",                    is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "Stadium_Foggy_P.upk",                      display_name: "DFH Stadium (Stormy)",                arena_family: "DFH Stadium",       variant_label: "Stormy",          map_image_upk: "Stadium_Foggy_P_MapImage_SF.upk",                  is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "Stadium_Winter_P.upk",                     display_name: "DFH Stadium (Snowy)",                 arena_family: "DFH Stadium",       variant_label: "Snowy",           map_image_upk: "stadium_winter_p_MapImage_SF.upk",                 is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "Stadium_Race_Day_p.upk",                   display_name: "DFH Stadium (Circuit)",               arena_family: "DFH Stadium",       variant_label: "Circuit",         map_image_upk: "stadium_race_day_p_MapImage_SF.upk",               is_default_size: true,  category: "Custom Default Size" },
    StandardMapSpec { upk_name: "stadium_10a_p.upk",                        display_name: "DFH Stadium (10th Anniversary)",      arena_family: "DFH Stadium",       variant_label: "10th Anniv",      map_image_upk: "stadium_10a_p_MapImage_SF.upk",                    is_default_size: true,  category: "Custom Default Size" },
    StandardMapSpec { upk_name: "EuroStadium_P.upk",                        display_name: "Mannfield",                           arena_family: "Mannfield",         variant_label: "Day",             map_image_upk: "EuroStadium_P_MapImage_SF.upk",                    is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "EuroStadium_Night_P.upk",                  display_name: "Mannfield (Night)",                   arena_family: "Mannfield",         variant_label: "Night",           map_image_upk: "EuroStadium_Night_P_MapImage_SF.upk",              is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "EuroStadium_Rainy_P.upk",                  display_name: "Mannfield (Stormy)",                  arena_family: "Mannfield",         variant_label: "Stormy",          map_image_upk: "EuroStadium_Rainy_P_MapImage_SF.upk",              is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "EuroStadium_SnowNight_P.upk",              display_name: "Mannfield (Snowy)",                   arena_family: "Mannfield",         variant_label: "Snowy",           map_image_upk: "eurostadium_snownight_p_MapImage_SF.upk",          is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "EuroStadium_Dusk_P.upk",                   display_name: "Mannfield (Dusk)",                    arena_family: "Mannfield",         variant_label: "Dusk",            map_image_upk: "eurostadium_dusk_p_MapImage_SF.upk",               is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "TrainStation_P.upk",                       display_name: "Urban Central",                       arena_family: "Urban Central",     variant_label: "Day",             map_image_upk: "TrainStation_P_MapImage_SF.upk",                   is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "TrainStation_Night_P.upk",                 display_name: "Urban Central (Night)",               arena_family: "Urban Central",     variant_label: "Night",           map_image_upk: "TrainStation_Night_P_MapImage_SF.upk",             is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "TrainStation_Dawn_P.upk",                  display_name: "Urban Central (Dawn)",                arena_family: "Urban Central",     variant_label: "Dawn",            map_image_upk: "TrainStation_Dawn_P_MapImage_SF.upk",              is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "Haunted_TrainStation_P.upk",               display_name: "Urban Central (Haunted)",             arena_family: "Urban Central",     variant_label: "Haunted",         map_image_upk: "haunted_trainstation_p_MapImage_SF.upk",           is_default_size: true,  category: "Custom Default Size" },
    StandardMapSpec { upk_name: "UtopiaStadium_P.upk",                      display_name: "Utopia Coliseum",                     arena_family: "Utopia Coliseum",   variant_label: "Day",             map_image_upk: "UtopiaStadium_P_MapImage_SF.upk",                  is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "UtopiaStadium_Dusk_P.upk",                 display_name: "Utopia Coliseum (Dusk)",              arena_family: "Utopia Coliseum",   variant_label: "Dusk",            map_image_upk: "UtopiaStadium_Dusk_P_MapImage_SF.upk",             is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "UtopiaStadium_Snow_P.upk",                 display_name: "Utopia Coliseum (Snowy)",             arena_family: "Utopia Coliseum",   variant_label: "Snowy",           map_image_upk: "UtopiaStadium_Snow_P_MapImage_SF.upk",             is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "UtopiaStadium_Lux_P.upk",                  display_name: "Utopia Coliseum (Gilded)",            arena_family: "Utopia Coliseum",   variant_label: "Gilded",          map_image_upk: "utopiastadium_lux_p_MapImage_SF.upk",              is_default_size: true,  category: "Custom Default Size" },
    StandardMapSpec { upk_name: "Wasteland_S_P.upk",                        display_name: "Wasteland",                           arena_family: "Wasteland",         variant_label: "Day",             map_image_upk: "wasteland_s_p_MapImage_SF.upk",                    is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "Wasteland_Night_S_P.upk",                  display_name: "Wasteland (Night)",                   arena_family: "Wasteland",         variant_label: "Night",           map_image_upk: "wasteland_night_s_p_MapImage_SF.upk",              is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "Wasteland_grs_P.upk",                      display_name: "Wasteland (Pitched)",                 arena_family: "Wasteland",         variant_label: "Pitched",         map_image_upk: "wasteland_grs_p_MapImage_SF.upk",                  is_default_size: true,  category: "Custom Default Size" },
    StandardMapSpec { upk_name: "Wasteland_Art_S_P.upk",                    display_name: "Wasteland (Standard Art)",            arena_family: "Wasteland",         variant_label: "Art",             map_image_upk: "wasteland_s_p_MapImage_SF.upk",                    is_default_size: true,  category: "Custom Default Size" },
    StandardMapSpec { upk_name: "Wasteland_Art_Night_S_P.upk",              display_name: "Wasteland (Night Art)",               arena_family: "Wasteland",         variant_label: "Night Art",       map_image_upk: "wasteland_night_s_p_MapImage_SF.upk",              is_default_size: true,  category: "Custom Default Size" },
    StandardMapSpec { upk_name: "NeoTokyo_Standard_P.upk",                  display_name: "Neo Tokyo",                           arena_family: "Neo Tokyo",         variant_label: "Night",           map_image_upk: "neotokyo_standard_p_MapImage_SF.upk",              is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "NeoTokyo_Toon_p.upk",                      display_name: "Neo Tokyo (Comic)",                   arena_family: "Neo Tokyo",         variant_label: "Comic",           map_image_upk: "neotokyo_toon_p_MapImage_SF.upk",                  is_default_size: true,  category: "Custom Default Size" },
    StandardMapSpec { upk_name: "NeoTokyo_Hax_P.upk",                       display_name: "Neo Tokyo (Hacked)",                  arena_family: "Neo Tokyo",         variant_label: "Hacked",          map_image_upk: "neotokyo_hax_p_MapImage_SF.upk",                   is_default_size: true,  category: "Custom Default Size" },
    StandardMapSpec { upk_name: "Neotokyo_Arcade_P.upk",                    display_name: "Neo Tokyo (Arcade)",                  arena_family: "Neo Tokyo",         variant_label: "Arcade",          map_image_upk: "neotokyo_arcade_p_MapImage_SF.upk",                is_default_size: true,  category: "Custom Default Size" },
    StandardMapSpec { upk_name: "Underwater_P.upk",                         display_name: "AquaDome",                            arena_family: "AquaDome",          variant_label: "Standard",        map_image_upk: "Underwater_P_MapImage_SF.upk",                     is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "Underwater_GRS_P.upk",                     display_name: "AquaDome (Salty Shallows)",           arena_family: "AquaDome",          variant_label: "Salty Shallows",  map_image_upk: "underwater_grs_p_MapImage_SF.upk",                 is_default_size: true,  category: "Custom Default Size" },
    StandardMapSpec { upk_name: "arc_standard_p.upk",                       display_name: "Starbase ARC",                        arena_family: "Starbase ARC",      variant_label: "Standard",        map_image_upk: "arc_standard_p_MapImage_SF.upk",                   is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "ARC_Darc_P.upk",                           display_name: "Starbase ARC (Aftermath)",            arena_family: "Starbase ARC",      variant_label: "Aftermath",       map_image_upk: "arc_darc_p_MapImage_SF.upk",                       is_default_size: true,  category: "Custom Default Size" },
    StandardMapSpec { upk_name: "Farm_P.upk",                               display_name: "Farmstead",                           arena_family: "Farmstead",         variant_label: "Dusk",            map_image_upk: "farm_p_MapImage_SF.upk",                           is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "Farm_Night_P.upk",                         display_name: "Farmstead (Night)",                   arena_family: "Farmstead",         variant_label: "Night",           map_image_upk: "farm_night_p_MapImage_SF.upk",                     is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "Farm_HW_P.upk",                            display_name: "Farmstead (Spooky)",                  arena_family: "Farmstead",         variant_label: "Spooky",          map_image_upk: "farm_hw_p_MapImage_SF.upk",                        is_default_size: true,  category: "Custom Default Size" },
    StandardMapSpec { upk_name: "Farm_GRS_P.upk",                           display_name: "Farmstead (Pitched)",                 arena_family: "Farmstead",         variant_label: "Pitched",         map_image_upk: "farm_grs_p_MapImage_SF.upk",                       is_default_size: true,  category: "Custom Default Size" },
    StandardMapSpec { upk_name: "cs_p.upk",                                 display_name: "Champions Field",                     arena_family: "Champions Field",   variant_label: "Night",           map_image_upk: "cs_p_MapImage_SF.upk",                             is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "cs_day_p.upk",                             display_name: "Champions Field (Day)",               arena_family: "Champions Field",   variant_label: "Day",             map_image_upk: "cs_day_p_MapImage_SF.upk",                         is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "cs_hw_p.upk",                              display_name: "Rivals Arena",                        arena_family: "Champions Field",   variant_label: "Rivals",          map_image_upk: "cs_hw_p_MapImage_SF.upk",                          is_default_size: true,  category: "Custom Default Size" },
    StandardMapSpec { upk_name: "Swoosh_P.upk",                             display_name: "Champions Field (Nike FC)",           arena_family: "Champions Field",   variant_label: "Nike FC",         map_image_upk: "swoosh_p_MapImage_SF.upk",                         is_default_size: true,  category: "Custom Default Size" },
    StandardMapSpec { upk_name: "BB_P.upk",                                 display_name: "Champions Field (NFL)",               arena_family: "Champions Field",   variant_label: "NFL",             map_image_upk: "bb_p_MapImage_SF.upk",                             is_default_size: true,  category: "Custom Default Size" },
    StandardMapSpec { upk_name: "chn_stadium_p.upk",                        display_name: "Forbidden Temple",                    arena_family: "Forbidden Temple",  variant_label: "Standard",        map_image_upk: "chn_stadium_p_MapImage_SF.upk",                    is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "chn_stadium_day_p.upk",                    display_name: "Forbidden Temple (Dawn)",             arena_family: "Forbidden Temple",  variant_label: "Dawn",            map_image_upk: "chn_stadium_day_p_MapImage_SF.upk",                is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "fni_stadium_p.upk",                        display_name: "Forbidden Temple (Fire & Ice)",       arena_family: "Forbidden Temple",  variant_label: "Fire & Ice",      map_image_upk: "fni_stadium_p_MapImage_SF.upk",                    is_default_size: true,  category: "Custom Default Size" },
    StandardMapSpec { upk_name: "beach_P.upk",                              display_name: "Salty Shores",                        arena_family: "Salty Shores",      variant_label: "Day",             map_image_upk: "beach_p_MapImage_SF.upk",                          is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "beach_night_p.upk",                        display_name: "Salty Shores (Night)",                arena_family: "Salty Shores",      variant_label: "Night",           map_image_upk: "beach_night_p_MapImage_SF.upk",                    is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "beach_night_grs_p.upk",                    display_name: "Salty Shores (Salty Fest)",           arena_family: "Salty Shores",      variant_label: "Salty Fest",      map_image_upk: "Beach_Night_GRS_P_MapImage_SF.upk",                is_default_size: true,  category: "Custom Default Size" },
    StandardMapSpec { upk_name: "music_p.upk",                              display_name: "Neon Fields",                         arena_family: "Neon Fields",       variant_label: "Standard",        map_image_upk: "music_p_MapImage_SF.upk",                          is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "outlaw_p.upk",                             display_name: "Deadeye Canyon",                      arena_family: "Deadeye Canyon",    variant_label: "Standard",        map_image_upk: "outlaw_p_MapImage_SF.upk",                         is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "Outlaw_Oasis_P.upk",                       display_name: "Deadeye Canyon (Oasis)",              arena_family: "Deadeye Canyon",    variant_label: "Oasis",           map_image_upk: "outlaw_oasis_p_MapImage_SF.upk",                   is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "Street_P.upk",                             display_name: "Estadio Vida",                        arena_family: "Estadio Vida",      variant_label: "Dusk",            map_image_upk: "street_p_MapImage_SF.upk",                         is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "Woods_P.upk",                              display_name: "Drift Woods",                         arena_family: "Drift Woods",       variant_label: "Dawn",            map_image_upk: "woods_p_MapImage_SF.upk",                          is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "Woods_Night_P.upk",                        display_name: "Drift Woods (Night)",                 arena_family: "Drift Woods",       variant_label: "Night",           map_image_upk: "woods_night_p_MapImage_SF.upk",                    is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "Woods_Forest_P.upk",                       display_name: "Drift Woods (Forest)",                arena_family: "Drift Woods",       variant_label: "Forest",          map_image_upk: "woods_p_MapImage_SF.upk",                          is_default_size: true,  category: "Custom Default Size" },
    StandardMapSpec { upk_name: "FF_Dusk_P.upk",                            display_name: "Sovereign Heights",                   arena_family: "Sovereign Heights", variant_label: "Dusk",            map_image_upk: "ff_dusk_p_MapImage_SF.upk",                        is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "Paname_Dusk_P.upk",                        display_name: "Parc de Paris",                       arena_family: "Parc de Paris",     variant_label: "Dusk",            map_image_upk: "Paname_Dusk_p_MapImage_SF.upk",                    is_default_size: true,  category: "Standard" },
    StandardMapSpec { upk_name: "Mall_Day_P.upk",                           display_name: "Boostfield Mall",                     arena_family: "Boostfield Mall",   variant_label: "Day",             map_image_upk: "mall_day_p_MapImage_SF.upk",                       is_default_size: true,  category: "Standard" },
    // ── Custom Default-Size LTM / Event / 4v4 Arenas ──────────────────────────
    StandardMapSpec { upk_name: "UF_Day_P.upk",                             display_name: "Futura Garden (Day)",                 arena_family: "Futura Garden",     variant_label: "Day",             map_image_upk: "UF_Day_P_MapImage_SF.upk",                         is_default_size: true,  category: "Custom Default Size" },
    StandardMapSpec { upk_name: "UF_Night_P.upk",                           display_name: "Futura Garden (Night)",               arena_family: "Futura Garden",     variant_label: "Night",           map_image_upk: "uf_night_p_MapImage_SF.upk",                       is_default_size: true,  category: "Custom Default Size" },
    StandardMapSpec { upk_name: "Park_Bman_P.upk",                          display_name: "Beckwith Park (Gotham Night)",        arena_family: "Beckwith Park",     variant_label: "Gotham",          map_image_upk: "park_bman_p_MapImage_SF.upk",                      is_default_size: true,  category: "Custom Default Size" },
    StandardMapSpec { upk_name: "Labs_4v4_Arena15_Blackout_P.upk",          display_name: "Chaos Run Blackout (4v4)",            arena_family: "4v4 Arena",         variant_label: "Blackout",        map_image_upk: "labs_4v4_arena15_blackout_p_MapImage_SF.upk",      is_default_size: true,  category: "Custom Default Size" },
    StandardMapSpec { upk_name: "Labs_4v4_Arena15_EuroStadium_Night_P.upk", display_name: "Mannfield Night (4v4)",               arena_family: "4v4 Arena",         variant_label: "Night",           map_image_upk: "labs_4v4_arena15_eurostadium_night_p_MapImage_SF.upk", is_default_size: true, category: "Custom Default Size" },
    StandardMapSpec { upk_name: "Labs_4v4_Arena15_Retro_P.upk",             display_name: "Retro Utopia (4v4)",                  arena_family: "4v4 Arena",         variant_label: "Retro",           map_image_upk: "labs_4v4_arena15_retro_p_MapImage_SF.upk",         is_default_size: true,  category: "Custom Default Size" },
    // ── Official Rocket Labs, Classic Non-Standard, Hoops, Dropshot & Knockout
    StandardMapSpec { upk_name: "NeoTokyo_P.upk",                           display_name: "Tokyo Underpass",                     arena_family: "Neo Tokyo",         variant_label: "Underpass",       map_image_upk: "neotokyo_p_MapImage_SF.upk",                       is_default_size: false, category: "Labs & Special" },
    StandardMapSpec { upk_name: "Wasteland_P.upk",                          display_name: "Badlands",                            arena_family: "Wasteland",         variant_label: "Classic",         map_image_upk: "wasteland_p_MapImage_SF.upk",                      is_default_size: false, category: "Labs & Special" },
    StandardMapSpec { upk_name: "Wasteland_Night_P.upk",                    display_name: "Badlands (Night)",                    arena_family: "Wasteland",         variant_label: "Classic Night",   map_image_upk: "wasteland_night_p_MapImage_SF.upk",                is_default_size: false, category: "Labs & Special" },
    StandardMapSpec { upk_name: "ARC_P.upk",                                display_name: "Starbase ARC (Octagon)",              arena_family: "Starbase ARC",      variant_label: "Octagon",         map_image_upk: "arc_p_MapImage_SF.upk",                            is_default_size: false, category: "Labs & Special" },
    StandardMapSpec { upk_name: "ThrowbackStadium_P.upk",                   display_name: "Throwback Stadium",                   arena_family: "Throwback",         variant_label: "Standard",        map_image_upk: "throwbackstadium_p_MapImage_SF.upk",               is_default_size: false, category: "Labs & Special" },
    StandardMapSpec { upk_name: "ThrowbackHockey_P.upk",                    display_name: "Throwback Stadium (Snowy)",           arena_family: "Throwback",         variant_label: "Snowy",           map_image_upk: "throwbackhockey_p_MapImage_SF.upk",                is_default_size: false, category: "Labs & Special" },
    StandardMapSpec { upk_name: "Labs_Underpass_P.upk",                     display_name: "Underpass (Labs)",                    arena_family: "Rocket Labs",       variant_label: "Labs",            map_image_upk: "labs_underpass_p_MapImage_SF.upk",                 is_default_size: false, category: "Labs & Special" },
    StandardMapSpec { upk_name: "Labs_CirclePillars_P.upk",                 display_name: "Pillars",                             arena_family: "Rocket Labs",       variant_label: "Pillars",         map_image_upk: "labs_circlepillars_p_MapImage_SF.upk",             is_default_size: false, category: "Labs & Special" },
    StandardMapSpec { upk_name: "Labs_PillarGlass_P.upk",                   display_name: "Pillars (Glass)",                     arena_family: "Rocket Labs",       variant_label: "Glass",           map_image_upk: "labs_pillarglass_p_MapImage_SF.upk",               is_default_size: false, category: "Labs & Special" },
    StandardMapSpec { upk_name: "Labs_PillarHeat_P.upk",                    display_name: "Pillars (Heatseeker)",                arena_family: "Rocket Labs",       variant_label: "Heat",            map_image_upk: "labs_pillarheat_p_MapImage_SF.upk",                is_default_size: false, category: "Labs & Special" },
    StandardMapSpec { upk_name: "Labs_PillarWings_P.upk",                   display_name: "Pillars (Wings)",                     arena_family: "Rocket Labs",       variant_label: "Wings",           map_image_upk: "labs_pillarwings_p_MapImage_SF.upk",               is_default_size: false, category: "Labs & Special" },
    StandardMapSpec { upk_name: "Labs_Cosmic_V4_P.upk",                     display_name: "Cosmic",                              arena_family: "Rocket Labs",       variant_label: "Labs",            map_image_upk: "labs_cosmic_v4_p_MapImage_SF.upk",                 is_default_size: false, category: "Labs & Special" },
    StandardMapSpec { upk_name: "Labs_DoubleGoal_V2_P.upk",                 display_name: "Double Goal",                         arena_family: "Rocket Labs",       variant_label: "Labs",            map_image_upk: "labs_doublegoal_v2_p_MapImage_SF.upk",             is_default_size: false, category: "Labs & Special" },
    StandardMapSpec { upk_name: "Labs_Utopia_P.upk",                        display_name: "Utopia Retro",                        arena_family: "Rocket Labs",       variant_label: "Labs",            map_image_upk: "labs_utopia_p_MapImage_SF.upk",                    is_default_size: false, category: "Labs & Special" },
    StandardMapSpec { upk_name: "Labs_Octagon_02_P.upk",                    display_name: "Octagon",                             arena_family: "Rocket Labs",       variant_label: "Labs",            map_image_upk: "labs_octagon_02_p_MapImage_SF.upk",                is_default_size: false, category: "Labs & Special" },
    StandardMapSpec { upk_name: "Labs_Octagon_B2B_02_P.upk",                display_name: "Octagon (Back-to-Back)",              arena_family: "Rocket Labs",       variant_label: "B2B",             map_image_upk: "labs_octagon_b2b_02_p_MapImage_SF.upk",            is_default_size: false, category: "Labs & Special" },
    StandardMapSpec { upk_name: "Labs_Octagon_Vent_P.upk",                  display_name: "Octagon (Vent)",                      arena_family: "Rocket Labs",       variant_label: "Vent",            map_image_upk: "labs_octagon_vent_p_MapImage_SF.upk",              is_default_size: false, category: "Labs & Special" },
    StandardMapSpec { upk_name: "Labs_Corridor_P.upk",                      display_name: "Corridor",                            arena_family: "Rocket Labs",       variant_label: "Labs",            map_image_upk: "labs_corridor_p_MapImage_SF.upk",                  is_default_size: false, category: "Labs & Special" },
    StandardMapSpec { upk_name: "Labs_Basin_P.upk",                         display_name: "Basin",                               arena_family: "Rocket Labs",       variant_label: "Labs",            map_image_upk: "labs_basin_p_MapImage_SF.upk",                     is_default_size: false, category: "Labs & Special" },
    StandardMapSpec { upk_name: "Labs_Galleon_P.upk",                       display_name: "Galleon",                             arena_family: "Rocket Labs",       variant_label: "Labs",            map_image_upk: "labs_galleon_p_MapImage_SF.upk",                   is_default_size: false, category: "Labs & Special" },
    StandardMapSpec { upk_name: "Labs_Galleon_Mast_P.upk",                  display_name: "Galleon Mast",                        arena_family: "Rocket Labs",       variant_label: "Labs",            map_image_upk: "labs_galleon_mast_p_MapImage_SF.upk",              is_default_size: false, category: "Labs & Special" },
    StandardMapSpec { upk_name: "Labs_Holyfield_P.upk",                     display_name: "Loophole",                            arena_family: "Rocket Labs",       variant_label: "Labs",            map_image_upk: "labs_holyfield_p_MapImage_SF.upk",                 is_default_size: false, category: "Labs & Special" },
    StandardMapSpec { upk_name: "Labs_Holyfield_Space_P.upk",               display_name: "Loophole (Space)",                    arena_family: "Rocket Labs",       variant_label: "Space",           map_image_upk: "Labs_Holyfield_Space_P_MapImage_SF.upk",           is_default_size: false, category: "Labs & Special" },
    StandardMapSpec { upk_name: "HoopsStadium_P.upk",                       display_name: "Dunk House (Hoops)",                  arena_family: "Hoops",             variant_label: "Hoops",           map_image_upk: "hoopsstadium_p_MapImage_SF.upk",                   is_default_size: false, category: "Labs & Special" },
    StandardMapSpec { upk_name: "HoopsStreet_P.upk",                        display_name: "The Block (Hoops)",                   arena_family: "Hoops",             variant_label: "Hoops",           map_image_upk: "hoopsstreet_p_MapImage_SF.upk",                    is_default_size: false, category: "Labs & Special" },
    StandardMapSpec { upk_name: "ShatterShot_P.upk",                        display_name: "Core 707 (Dropshot)",                 arena_family: "Dropshot",          variant_label: "Dropshot",        map_image_upk: "shattershot_p_MapImage_SF.upk",                    is_default_size: false, category: "Labs & Special" },
    StandardMapSpec { upk_name: "KO_Calavera_P.upk",                        display_name: "Calavera (Knockout)",                 arena_family: "Knockout",          variant_label: "Knockout",        map_image_upk: "ko_calavera_p_MapImage_SF.upk",                    is_default_size: false, category: "Labs & Special" },
    StandardMapSpec { upk_name: "KO_Carbon_P.upk",                          display_name: "Carbon (Knockout)",                   arena_family: "Knockout",          variant_label: "Knockout",        map_image_upk: "ko_carbon_p_MapImage_SF.upk",                      is_default_size: false, category: "Labs & Special" },
    StandardMapSpec { upk_name: "KO_Quadron_P.upk",                         display_name: "Quadron (Knockout)",                  arena_family: "Knockout",          variant_label: "Knockout",        map_image_upk: "ko_quadron_p_MapImage_SF.upk",                     is_default_size: false, category: "Labs & Special" },
];

pub fn is_standard_soccar_map_name(filename: &str) -> bool {
    let low = filename.to_lowercase();
    let clean = low.strip_suffix(".bak").unwrap_or(&low);
    STANDARD_SOCCAR_MAPS
        .iter()
        .any(|m| m.upk_name.to_lowercase() == clean)
}

pub fn is_default_size_map_name(filename: &str) -> bool {
    let low = filename.to_lowercase();
    let clean = low.strip_suffix(".bak").unwrap_or(&low);
    STANDARD_SOCCAR_MAPS
        .iter()
        .any(|m| m.is_default_size && m.upk_name.to_lowercase() == clean)
}

pub fn get_map_preview_cache_dir() -> PathBuf {
    let p = get_appdata_dir().join("map_preview_cache");
    let _ = fs::create_dir_all(&p);
    p
}

pub fn extract_map_preview_image(cooked_dir: &Path, spec: &StandardMapSpec, width: u32, height: u32) -> RgbaImage {
    let cache_dir = get_map_preview_cache_dir();
    let stem = spec.upk_name.to_lowercase().replace(".upk", "");
    let cache_file = cache_dir.join(format!("{}_{}x{}.png", stem, width, height));
    if cache_file.exists() {
        if let Ok(im) = image::open(&cache_file) {
            return im.to_rgba8();
        }
    }

    let img_path = cooked_dir.join(spec.map_image_upk);
    if img_path.exists() {
        if let Some(raw_im) = extract_texture_from_upk(&img_path, Some(cooked_dir)) {
            let resized = DynamicImage::ImageRgba8(raw_im)
                .resize_exact(width, height, FilterType::Lanczos3)
                .to_rgba8();
            let _ = resized.save(&cache_file);
            return resized;
        }
    }

    let mut fallback = RgbaImage::from_pixel(width, height, Rgba([16, 42, 28, 255]));
    for y in 0..height {
        for x in 0..width {
            let t = y as f32 / height as f32;
            let c = if t < 0.45 {
                Rgba([14, 24, 42, 255])
            } else {
                Rgba([18, 68, 36, 255])
            };
            fallback.put_pixel(x, y, c);
        }
    }
    fallback
}

/// Re-encrypts header AND AES-256-CTR compressed chunks of `built_upk_bytes` with `target_key`.
fn rekey_encrypted_upk_buffer(
    built_upk_bytes: &[u8],
    built_key: &[u8; 32],
    target_key: &[u8; 32],
) -> Result<Vec<u8>, String> {
    if built_key == target_key {
        return Ok(built_upk_bytes.to_vec());
    }
    let pfx = parse_upk_prefix(built_upk_bytes)?;
    let enc_size = pfx
        .total_header_size
        .saturating_sub(pfx.garbage_size)
        .saturating_sub(pfx.name_offset);
    let enc_aligned = (enc_size + 15) & !15;
    if pfx.name_offset + enc_aligned > built_upk_bytes.len() {
        return Err("Invalid UPK header bounds for re-keying".into());
    }
    let enc_slice = &built_upk_bytes[pfx.name_offset..pfx.name_offset + enc_aligned];
    let mut out = built_upk_bytes.to_vec();

    if let Some(nonce) = pfx.ctr_nonce {
        let plain = aes_ctr_crypt(built_key, &nonce, enc_slice);
        let chunks = parse_chunk_table(&plain, &pfx, true);
        let re_enc = aes_ctr_crypt(target_key, &nonce, &plain);
        out[pfx.name_offset..pfx.name_offset + enc_aligned].copy_from_slice(&re_enc);
        for ch in chunks {
            if let Some(cn) = ch.chunk_nonce {
                let end = ch.compressed_offset + ch.compressed_size;
                if end <= out.len() {
                    let dec = aes_ctr_crypt(built_key, &cn, &out[ch.compressed_offset..end]);
                    let enc = aes_ctr_crypt(target_key, &cn, &dec);
                    out[ch.compressed_offset..end].copy_from_slice(&enc);
                }
            }
        }
    } else {
        let plain = aes_ecb_decrypt(built_key, enc_slice);
        let re_enc = aes_ecb_encrypt(target_key, &plain[..enc_aligned]);
        out[pfx.name_offset..pfx.name_offset + enc_aligned].copy_from_slice(&re_enc[..enc_aligned]);
    }
    Ok(out)
}

/// Pure normal arena swap: copies pristine `donor_upk_name` (re-keyed per target AES key)
/// onto `target_upk_name` OR across all default-size arenas in parallel when `override_all_default_size` is true.
pub fn swap_standard_map(
    cooked_dir: &Path,
    donor_upk_name: &str,
    target_upk_name: &str,
    override_all_default_size: bool,
) -> Result<String, String> {
    if !is_standard_soccar_map_name(donor_upk_name) {
        return Err(format!("Unsupported donor map: {}", donor_upk_name));
    }
    if !override_all_default_size && !is_standard_soccar_map_name(target_upk_name) {
        return Err(format!("Unsupported target map: {}", target_upk_name));
    }

    let donor_disk = cooked_dir.join(donor_upk_name);
    let donor_bak = bak_path_for(&donor_disk);
    let donor_src = if donor_bak.is_file() { donor_bak } else { donor_disk };
    if !donor_src.is_file() {
        return Err(format!("Donor map not found: {}", donor_src.display()));
    }

    let donor_raw = fs::read(&donor_src).map_err(|e| e.to_string())?;
    let donor_pfx = parse_upk_prefix(&donor_raw)?;
    let (_, donor_key, _) = decrypt_upk_header(&donor_src, &donor_raw, &donor_pfx)?;

    let targets: Vec<&str> = if override_all_default_size {
        STANDARD_SOCCAR_MAPS
            .iter()
            .filter(|m| m.is_default_size)
            .map(|m| m.upk_name)
            .collect()
    } else {
        vec![target_upk_name]
    };

    let applied: usize = targets
        .par_iter()
        .map(|&t_name| {
            let t_path = cooked_dir.join(t_name);
            if !t_path.exists() {
                return 0;
            }
            if t_name.eq_ignore_ascii_case(donor_upk_name) {
                let _ = restore_single_package(&t_path);
                return 1;
            }
            if ensure_pristine_before_swap(&t_path).is_err() {
                return 0;
            }
            let t_bak = bak_path_for(&t_path);
            let key_src = if t_bak.is_file() { &t_bak } else { &t_path };
            let Ok(t_raw) = fs::read(key_src) else { return 0 };
            let Ok(t_pfx) = parse_upk_prefix(&t_raw) else { return 0 };
            let Ok((_, t_key, _)) = decrypt_upk_header(key_src, &t_raw, &t_pfx) else {
                return 0;
            };
            if let Ok(rekeyed) = rekey_encrypted_upk_buffer(&donor_raw, &donor_key, &t_key) {
                if fs::write(&t_path, &rekeyed).is_ok() {
                    return 1;
                }
            }
            0
        })
        .sum();

    clear_rl_texture_cache();
    if override_all_default_size {
        Ok(format!("Swapped {} across {} default-size arenas", donor_upk_name, applied))
    } else {
        Ok(format!("Swapped {} -> {}", donor_upk_name, target_upk_name))
    }
}

/// Restores all official arenas to their original vanilla files in parallel.
pub fn reset_all_standard_maps(cooked_dir: &Path) -> usize {
    let count: usize = STANDARD_SOCCAR_MAPS
        .par_iter()
        .map(|spec| {
            let p = cooked_dir.join(spec.upk_name);
            if restore_single_package(&p) { 1 } else { 0 }
        })
        .sum();
    clear_rl_texture_cache();
    count
}
