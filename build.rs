use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

// The window icon ships as a precompiled resource archive; the VERSIONINFO
// resource is generated from Cargo.toml so the executable always reports the
// package version. rc.exe comes from the Windows SDK and is optional: a build
// without it still succeeds, just without embedded version metadata.
fn main() {
    println!("cargo:rerun-if-changed=assets/transpop.res");
    println!("cargo:rerun-if-env-changed=CARGO_PKG_VERSION");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows")
        || env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    println!(
        "cargo:rustc-link-arg-bin=transpop={}",
        manifest.join("assets").join("transpop.res").display()
    );
    match version_resource(&manifest) {
        Ok(resource) => println!("cargo:rustc-link-arg-bin=transpop={}", resource.display()),
        Err(reason) => println!("cargo:warning=version resource skipped: {reason}"),
    }
}

fn version_resource(manifest: &Path) -> Result<PathBuf, String> {
    let version = env::var("CARGO_PKG_VERSION").map_err(|e| e.to_string())?;
    let mut numeric = [0u64; 4];
    for (slot, part) in numeric.iter_mut().zip(version.split('.')) {
        *slot = part.parse().unwrap_or(0);
    }
    let file_version = numeric.map(|value| value.to_string()).join(", ");
    let rc_text = format!(
        "1 VERSIONINFO\n\
         FILEVERSION {file_version}\n\
         PRODUCTVERSION {file_version}\n\
         FILEOS 0x40004\n\
         FILETYPE 0x1\n\
         BEGIN\n\
           BLOCK \"StringFileInfo\"\n\
           BEGIN\n\
             BLOCK \"040904b0\"\n\
             BEGIN\n\
               VALUE \"CompanyName\", \"Gurio\"\n\
               VALUE \"FileDescription\", \"TransPop: IME candidate translator\"\n\
               VALUE \"FileVersion\", \"{version}\"\n\
               VALUE \"InternalName\", \"transpop\"\n\
               VALUE \"LegalCopyright\", \"Copyright (c) 2026 Gurio. MIT License\"\n\
               VALUE \"OriginalFilename\", \"transpop.exe\"\n\
               VALUE \"ProductName\", \"TransPop\"\n\
               VALUE \"ProductVersion\", \"{version}\"\n\
             END\n\
           END\n\
           BLOCK \"VarFileInfo\"\n\
           BEGIN\n\
             VALUE \"Translation\", 0x409, 1200\n\
           END\n\
         END\n"
    );
    let out_dir = PathBuf::from(env::var("OUT_DIR").map_err(|e| e.to_string())?);
    let rc_file = out_dir.join("transpop-version.rc");
    fs::write(&rc_file, rc_text).map_err(|e| format!("write rc file: {e}"))?;
    let rc_exe = find_rc(manifest)
        .ok_or_else(|| "rc.exe not found; install the Windows SDK".to_string())?;
    let resource = out_dir.join("transpop-version.res");
    let output = Command::new(&rc_exe)
        .arg("/nologo")
        .arg("/fo")
        .arg(&resource)
        .arg(&rc_file)
        .output()
        .map_err(|e| format!("run {}: {e}", rc_exe.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("rc.exe failed: {}", stderr.trim()));
    }
    Ok(resource)
}

fn find_rc(manifest: &Path) -> Option<PathBuf> {
    if let Ok(explicit) = env::var("RC") {
        if Path::new(&explicit).is_file() {
            return Some(PathBuf::from(explicit));
        }
    }
    if let Some(paths) = env::var_os("PATH") {
        for directory in env::split_paths(&paths) {
            let candidate = directory.join("rc.exe");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    // The SDK lives next to the project on this machine; a checked-in copy of
    // rc.exe is not an option because the SDK license does not allow it.
    let _ = manifest;
    let arch = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok("x86") => "x86",
        _ => "x64",
    };
    let program_files = env::var_os("ProgramFiles(x86)")
        .or_else(|| env::var_os("ProgramFiles"))?;
    let bin = Path::new(&program_files)
        .join("Windows Kits")
        .join("10")
        .join("bin");
    let mut versions: Vec<PathBuf> = fs::read_dir(&bin).ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .collect();
    versions.sort();
    for version in versions.iter().rev() {
        let candidate = version.join(arch).join("rc.exe");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}
