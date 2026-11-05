use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=native/nds.c");
    println!("cargo:rerun-if-env-changed=BLOCKSDS");
    println!("cargo:rerun-if-env-changed=CC");
    if env::var_os("CARGO_FEATURE_NDS").is_none() {
        return;
    }
    assert_eq!(
        env::var("CARGO_CFG_TARGET_VENDOR").as_deref(),
        Ok("nintendo")
    );
    let sdk = PathBuf::from(env::var_os("BLOCKSDS").expect("Set BLOCKSDS to the SDK core"));
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let object = out.join("nds.o");
    let specs = sdk.join("sys/crts/ds_arm9.specs");
    let mut cc = Command::new(env::var("CC").unwrap_or_else(|_| "arm-none-eabi-gcc".into()));
    cc.args(["-c", "native/nds.c", "-o"])
        .arg(&object)
        .args([
            "-std=gnu11",
            "-O2",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-Wconversion",
            "-ffunction-sections",
            "-fdata-sections",
            "-mthumb",
            "-mcpu=arm946e-s+nofp",
        ])
        .arg(format!("-specs={}", specs.display()));
    for lib in ["libnds", "maxmod"] {
        cc.arg("-isystem")
            .arg(sdk.join("libs").join(lib).join("include"));
        println!(
            "cargo:rustc-link-search=native={}",
            sdk.join("libs").join(lib).join("lib").display()
        );
    }
    assert!(
        cc.status().expect("Run the ARM C compiler").success(),
        "DSi adapter failed to compile"
    );
    for arg in [
        object.display().to_string(),
        "-mthumb".into(),
        "-mcpu=arm946e-s+nofp".into(),
        format!("-specs={}", specs.display()),
        format!("-Wl,-Map={}", out.join("sounda.map").display()),
        "-Wl,--start-group".into(),
        "-lmm9".into(),
        "-lnds9".into(),
        "-lc".into(),
        "-lgcc".into(),
        "-Wl,--end-group".into(),
    ] {
        println!("cargo:rustc-link-arg-bin=sounda={arg}");
    }
}
