use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=assets/transpop.res");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let resource = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("assets").join("transpop.res");
        // MSVC link.exe embeds the Windows .res file in transpop.exe.
        // No rc.exe, DLL, runtime asset directory or extra build dependency.
        println!("cargo:rustc-link-arg-bin=transpop={}", resource.display());
    }
}
