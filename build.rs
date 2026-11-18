use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=assets/logo.png");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let ico_path = manifest_dir.join("assets").join("icon.ico");
    if !ico_path.exists() {
        println!("cargo:warning=Missing icon asset at {}; skipping Windows icon embedding", ico_path.display());
        return;
    }

    let version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "4.1.0".into());
    let mut nums: Vec<u16> = version
        .split('.')
        .filter_map(|p| p.parse::<u16>().ok())
        .collect();
    while nums.len() < 4 {
        nums.push(0);
    }
    let ver_commas = format!("{},{},{},{}", nums[0], nums[1], nums[2], nums[3]);
    let ico_rc_path = ico_path.to_string_lossy().replace('\\', "/");

    let rc_contents = format!(
        r#"1 ICON "{ico}"

1 VERSIONINFO
FILEVERSION {ver_commas}
PRODUCTVERSION {ver_commas}
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x1L
FILESUBTYPE 0x0L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904b0"
        BEGIN
            VALUE "CompanyName", "c1ya\0"
            VALUE "FileDescription", "RL Swapper\0"
            VALUE "FileVersion", "{ver_str}\0"
            VALUE "InternalName", "RL_Swapper\0"
            VALUE "LegalCopyright", "MIT License (c) c1ya\0"
            VALUE "OriginalFilename", "RL_Swapper.exe\0"
            VALUE "ProductName", "RL Swapper\0"
            VALUE "ProductVersion", "{ver_str}\0"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x0409, 1200
    END
END
"#,
        ico = ico_rc_path,
        ver_commas = ver_commas,
        ver_str = version,
    );

    let rc_path = out_dir.join("app_resource.rc");
    fs::write(&rc_path, rc_contents).expect("write app_resource.rc");

    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_else(|_| "x86_64".into());

    if target_env == "gnu" {
        let Some(windres) = find_windres(&target_arch) else {
            println!("cargo:warning=windres not found; RL_Swapper.exe will not contain an embedded Windows icon");
            return;
        };
        let obj_path = out_dir.join("app_resource.o");
        let mut cmd = Command::new(&windres);
        if let Some(bin_dir) = windres.parent() {
            let cur_path = env::var_os("PATH").unwrap_or_default();
            let mut paths = vec![bin_dir.to_path_buf()];
            paths.extend(env::split_paths(&cur_path));
            if let Ok(joined) = env::join_paths(paths) {
                cmd.env("PATH", joined);
            }
        }
        let status = cmd
            .arg("--input")
            .arg(&rc_path)
            .arg("--output-format=coff")
            .arg("--output")
            .arg(&obj_path)
            .status()
            .unwrap_or_else(|e| panic!("failed to run {}: {}", windres.display(), e));
        if !status.success() {
            panic!("windres failed with status {}", status);
        }
        println!("cargo:rustc-link-arg-bin=RL_Swapper={}", obj_path.display());
    } else if target_env == "msvc" {
        let Some(rc_exe) = find_msvc_rc(&target_arch) else {
            println!("cargo:warning=rc.exe not found; RL_Swapper.exe will not contain an embedded Windows icon");
            return;
        };
        let res_path = out_dir.join("app_resource.res");
        let status = Command::new(&rc_exe)
            .arg("/nologo")
            .arg("/fo")
            .arg(&res_path)
            .arg(&rc_path)
            .status()
            .unwrap_or_else(|e| panic!("failed to run {}: {}", rc_exe.display(), e));
        if !status.success() {
            panic!("rc.exe failed with status {}", status);
        }
        println!("cargo:rustc-link-arg-bin=RL_Swapper={}", res_path.display());
    }
}

fn find_in_path(candidates: &[&str]) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    for dir in env::split_paths(&path_var) {
        for name in candidates {
            let full = dir.join(name);
            if full.is_file() {
                return Some(full);
            }
        }
    }
    None
}

fn find_windres(arch: &str) -> Option<PathBuf> {
    if let Some(p) = env::var_os("WINDRES").map(PathBuf::from).filter(|p| p.is_file()) {
        return Some(p);
    }
    let prefixed = if arch == "x86" {
        "i686-w64-mingw32-windres.exe"
    } else {
        "x86_64-w64-mingw32-windres.exe"
    };
    if let Some(p) = find_in_path(&[prefixed, "windres.exe", "windres"]) {
        return Some(p);
    }
    for dir in [r"C:\msys64\mingw64\bin", r"C:\mingw64\bin", r"C:\w64devkit\bin"] {
        for name in [prefixed, "windres.exe"] {
            let cand = Path::new(dir).join(name);
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    None
}

fn find_msvc_rc(arch: &str) -> Option<PathBuf> {
    if let Some(p) = env::var_os("RC").map(PathBuf::from).filter(|p| p.is_file()) {
        return Some(p);
    }
    if let Some(p) = find_in_path(&["rc.exe"]) {
        return Some(p);
    }
    let sdk_arch = if arch == "x86" { "x86" } else { "x64" };
    for root in [
        r"C:\Program Files (x86)\Windows Kits\10\bin",
        r"C:\Program Files\Windows Kits\10\bin",
    ] {
        let root_path = Path::new(root);
        if let Ok(entries) = fs::read_dir(root_path) {
            let mut dirs: Vec<PathBuf> = entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.is_dir())
                .collect();
            dirs.sort();
            dirs.reverse();
            for ver_dir in dirs {
                let cand = ver_dir.join(sdk_arch).join("rc.exe");
                if cand.is_file() {
                    return Some(cand);
                }
            }
        }
    }
    None
}
