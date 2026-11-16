use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=assets/miningmod.ico");
    println!("cargo:rerun-if-changed=assets/miningmod.rc");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("miningmod.res");
    let sdk = PathBuf::from(env::var_os("ProgramFiles(x86)").expect("Windows SDK required"))
        .join("Windows Kits/10/bin");
    let mut versions: Vec<_> = fs::read_dir(sdk)
        .expect("Windows SDK required")
        .filter_map(Result::ok)
        .map(|e| e.path().join("x64/rc.exe"))
        .filter(|p| p.is_file())
        .collect();
    versions.sort();
    let rc = versions
        .last()
        .expect("Windows SDK resource compiler rc.exe required");
    let result = Command::new(rc)
        .args(["/nologo", "/fo"])
        .arg(&out)
        .arg("assets/miningmod.rc")
        .status()
        .expect("Resource compiler failed");
    assert!(result.success(), "Icon resource compilation failed");
    println!("cargo:rustc-link-arg={}", out.display());
}
