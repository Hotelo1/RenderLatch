use std::fs;
use std::io;
use std::path::PathBuf;

pub const PORTABLE: bool = cfg!(renderlatch_release);

fn data_dir() -> io::Result<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(|dir| PathBuf::from(dir).join("RenderLatch"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "LOCALAPPDATA is unavailable"))
}

pub fn runtime_dir() -> Option<PathBuf> {
    PORTABLE.then(|| data_dir().ok().map(|dir| dir.join("runtime"))).flatten()
}

pub fn prepare_ui() -> io::Result<PathBuf> {
    let root = data_dir()?.join("app").join(env!("CARGO_PKG_VERSION"));
    let tools = root.join("tools");
    fs::create_dir_all(&tools)?;
    let files: &[(&str, &[u8])] = &[
        ("optical-f5.ps1", include_bytes!("../tools/optical-f5.ps1")),
        ("gui.ps1", include_bytes!("../tools/gui.ps1")),
        ("update-app.ps1", include_bytes!("../tools/update-app.ps1")),
        ("TaskbarIdentity.cs", include_bytes!("../tools/TaskbarIdentity.cs")),
        ("gui.xaml", include_bytes!("../tools/gui.xaml")),
        ("game-library.xaml", include_bytes!("../tools/game-library.xaml")),
        ("find-games.ps1", include_bytes!("../tools/find-games.ps1")),
        ("fetch-dlss5.ps1", include_bytes!("../tools/fetch-dlss5.ps1")),
        ("fetch-runtime.ps1", include_bytes!("../tools/fetch-runtime.ps1")),
        ("kit-download.ps1", include_bytes!("../tools/kit-download.ps1")),
        ("renderlatch.ico", include_bytes!("../assets/renderlatch.ico")),
    ];
    for (name, content) in files {
        let path = tools.join(name);
        if fs::read(&path).is_ok_and(|saved| saved == *content) {
            continue;
        }
        let pending = path.with_extension(format!("{}.tmp", std::process::id()));
        fs::write(&pending, content)?;
        if let Err(err) = fs::rename(&pending, &path) {
            let _ = fs::remove_file(&pending);
            return Err(err);
        }
    }
    Ok(tools.join("gui.ps1"))
}

pub fn launch_error(message: &str) {
    #[link(name = "user32")]
    extern "system" {
        fn MessageBoxW(hwnd: isize, text: *const u16, caption: *const u16, flags: u32) -> i32;
    }
    let text: Vec<_> = message.encode_utf16().chain(Some(0)).collect();
    let caption: Vec<_> = "RenderLatch".encode_utf16().chain(Some(0)).collect();
    unsafe { MessageBoxW(0, text.as_ptr(), caption.as_ptr(), 0x10); }
}
