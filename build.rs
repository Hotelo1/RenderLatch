use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=RUSTFLAGS");
    println!("cargo:rerun-if-env-changed=CARGO_ENCODED_RUSTFLAGS");
    println!("cargo:rerun-if-env-changed=RENDERLATCH_RELEASE");
    println!("cargo:rustc-check-cfg=cfg(renderlatch_release)");
    if env::var("RENDERLATCH_RELEASE").as_deref() == Ok("1") {
        println!("cargo:rustc-cfg=renderlatch_release");
    }

    let flags = env::var("CARGO_ENCODED_RUSTFLAGS")
        .unwrap_or_default()
        .replace('\u{1f}', " ")
        .replace('\u{1}', " ");
    let flags = if flags.trim().is_empty() {
        "(nenhuma flag extra)".to_string()
    } else {
        flags.trim().to_string()
    };
    println!("cargo:rustc-env=FORGE_RUSTFLAGS={flags}");

    let profile = env::var("PROFILE").unwrap_or_else(|_| "desconhecido".into());
    println!("cargo:rustc-env=FORGE_PROFILE={profile}");

    let target = env::var("TARGET").unwrap_or_else(|_| "desconhecido".into());
    println!("cargo:rustc-env=FORGE_TARGET={target}");
    if target == "x86_64-pc-windows-msvc" {
        windows_resource();
    }
}

fn resource_compiler() -> PathBuf {
    if let Some(tool) = env::var_os("RC") {
        return tool.into();
    }
    if let Some(tool) = env::var_os("PATH").into_iter().flat_map(|paths| {
        env::split_paths(&paths).map(|dir| dir.join("rc.exe")).collect::<Vec<_>>()
    }).find(|p| p.is_file()) {
        return tool;
    }
    let root = env::var_os("ProgramFiles(x86)").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)"));
    let mut windows_kits: Vec<_> = fs::read_dir(root.join(r"Windows Kits\10\bin"))
        .into_iter().flatten().flatten()
        .map(|entry| entry.path().join(r"x64\rc.exe"))
        .filter(|path| path.is_file())
        .collect();
    windows_kits.sort();
    windows_kits.pop().expect("Install the Windows SDK or set RC to rc.exe")
}

fn windows_resource() {
    let icon = Path::new("assets/renderlatch.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    println!("cargo:rerun-if-env-changed=RC");
    let icon = fs::canonicalize(icon).expect("Find the application icon");
    let icon = icon.to_string_lossy().trim_start_matches(r"\\?\").replace('\\', "/");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let script = out.join("renderlatch.rc");
    let resource = out.join("renderlatch.res");
    let version = env!("CARGO_PKG_VERSION");
    let parts = format!("{},{},{},0", env!("CARGO_PKG_VERSION_MAJOR"), env!("CARGO_PKG_VERSION_MINOR"), env!("CARGO_PKG_VERSION_PATCH"));
    fs::write(&script, format!(r#"1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {parts}
PRODUCTVERSION {parts}
FILEOS 0x40004
FILETYPE 1
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904B0"
        BEGIN
            VALUE "FileDescription", "RenderLatch"
            VALUE "ProductName", "RenderLatch"
            VALUE "FileVersion", "{version}"
            VALUE "ProductVersion", "{version}"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x409, 1200
    END
END
"#)).expect("Write Windows version and icon resources");
    let status = Command::new(resource_compiler()).arg("/nologo")
        .arg("/fo").arg(&resource).arg(&script).status().expect("Run rc.exe");
    assert!(status.success(), "rc.exe failed to compile the app resources");
    println!("cargo:rustc-link-arg-bin=forge={}", resource.display());
}
