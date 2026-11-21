use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=icons/app.ico");
    println!("cargo:rerun-if-changed=icons/app.rc");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") { return; }
    let mut candidates = Vec::new();
    if let Some(path) = env::var_os("RC") { candidates.push(PathBuf::from(path)); }
    if let Some(path) = env::var_os("WindowsSdkDir") { candidates.push(PathBuf::from(path).join("bin")); }
    candidates.extend([PathBuf::from("C:/Program Files (x86)/Windows Kits/10/bin"),
        PathBuf::from("E:/Windows Kits/10/bin")]);
    let compiler = candidates.iter().find_map(|path| {
        if path.is_file() { return Some(path.clone()); }
        let mut versions = fs::read_dir(path).ok()?.flatten().map(|entry| entry.path()).collect::<Vec<_>>();
        versions.sort();
        versions.into_iter().rev().map(|version| version.join("x64/rc.exe")).find(|path| path.is_file())
    }).unwrap_or_else(|| PathBuf::from("rc.exe"));
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("app.res");
    let status = Command::new(compiler).arg("/nologo").arg("/fo").arg(&output)
        .arg("icons/app.rc").status().expect("无法运行 Windows 资源编译器");
    assert!(status.success(), "应用图标资源编译失败");
    println!("cargo:rustc-link-arg={}", output.display());
}
