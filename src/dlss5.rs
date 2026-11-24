// Aplica a DLSS 5 (Neural Rendering) num jogo ja instalado.
//
// A rota OptiScaler usa o DLSS do jogo. A rota Feeder usa o DLSS5oneclick
// para jogos sem DLSS nativo. O kit OptiScaler instalado e:
//
//   dxgi.dll              OptiScaler com o passe de Neural Rendering
//                         (renomeado para o nome que o jogo carrega)
//   nvngx.dll_dlssnr.dll  snippet que o OptiScaler exige ao lado do modelo
//   nvngx_dlssnr.dll      o modelo da DLSS 5 (~254 MB, nao vai no repositorio)
//   OptiScaler.ini        configuracao ajustada para ESTA maquina
//   OptiScaler\D3D12_OptiScaler\D3D12Core.dll
//
// Regras de seguranca:
//   - nada e sobrescrito sem backup em _dlss5forge\backup-<carimbo>\
//   - tudo que este comando colocou fica em _dlss5forge\install.txt
//   - --undo restaura os backups e remove SO os arquivos que estavam no registro
//   - um dxgi.dll de outro mod nunca e sobrescrito: escolhe-se outro nome

use crate::ansi::*;
use crate::machine::Machine;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const MARK: &str = "_dlss5forge";
const OPTIMIZED_INI: &str = "optimized-optiscaler.ini";
const DX12_PROFILE: &str = "forge-dx12-resolution.txt";
const FEEDER_CONSUMER: &str = "renodx-dlss5.addon64";
const FEEDER_CONSUMER_BACKUP: &str = "renodx-dlss5.addon64.disabled";
const FEEDER_RESHADE: &str = "ReShade64.dll";
const FEEDER_RESHADE_BACKUP: &str = "dxgi.dll.feeder-backup";
const UNITY_SCREEN: [&str; 3] = [
    "Screenmanager Resolution Width_h182942802",
    "Screenmanager Resolution Height_h2627697771",
    "Screenmanager Resolution Use Native_h1405027254",
];
const REG_GPU: &str = r"HKCU\Software\Microsoft\DirectX\UserGpuPreferences";

/// Nomes que um jogo de DirectX carrega do proprio diretorio.
/// `dxgi.dll` resolve quase tudo; os outros sao plano B quando ele ja esta ocupado.
const PROXIES: [&str; 5] = ["dxgi.dll", "winmm.dll", "d3d12.dll", "version.dll", "dbghelp.dll"];

fn has(args: &[String], f: &str) -> bool {
    args.iter().any(|a| a == f)
}

fn val<'a>(args: &'a [String], f: &str) -> Option<&'a str> {
    let i = args.iter().position(|a| a == f)?;
    args.get(i + 1).map(|s| s.as_str())
}

fn strip_verbatim(p: &Path) -> String {
    let s = p.display().to_string();
    if let Some(r) = s.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{r}")
    } else if let Some(r) = s.strip_prefix(r"\\?\") {
        r.to_string()
    } else {
        s
    }
}

fn stamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

fn resolve_dir(target: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(target);
    if p.is_dir() {
        return Ok(p);
    }
    if p.is_file() {
        return Ok(p.parent().map(|x| x.to_path_buf()).unwrap_or_else(|| PathBuf::from(".")));
    }
    Err(format!("caminho nao existe: {target}"))
}

fn kit_dir(args: &[String]) -> Option<PathBuf> {
    if let Some(d) = val(args, "--from") {
        return Some(PathBuf::from(d));
    }
    if let Some(runtime) = crate::distribution::runtime_dir() {
        return Some(runtime.join("dlss5"));
    }
    let mut cands = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(p) = exe.parent() {
            cands.push(p.join("runtime").join("dlss5"));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        cands.push(cwd.join("runtime").join("dlss5"));
    }
    cands.into_iter().find(|p| p.is_dir())
}

// ------------------------------------------------------------------ jogo

fn is_helper(name: &str) -> bool {
    let n = name.to_lowercase();
    const PREFIXES: [&str; 15] = [
        "unins", "setup", "installer", "vcredist", "vc_redist", "dxsetup", "dxwebsetup",
        "oalinst", "crashreport", "crashhandler", "unitycrashhandler", "epicwebhelper",
        "eac_", "easyanticheat", "gamelaunchhelper",
    ];
    PREFIXES.iter().any(|prefix| n.starts_with(prefix)) || n.ends_with("_be.exe")
}

/// Procura binários do jogo no diretório informado e em subpastas.
fn find_exe(game: &Path) -> (Option<PathBuf>, Vec<PathBuf>) {
    const IGNORED_DIRS: &[&str] = &[
        MARK, "runtime", "cache", "reshade-shaders", "host64", "engine", "plugins", "content",
        "__installer", "_commonredist", "redist", "mod", "mods",
    ];
    let is_bin_dir = |name: &str| {
        matches!(name, "binaries" | "win64" | "win32" | "x64" | "x86" | "bin" | "bin64")
    };
    let mut found = Vec::new();
    let mut pending = std::collections::VecDeque::from([(game.to_path_buf(), 0)]);
    for _ in 0..256 {
        let Some((dir, depth)) = pending.pop_front() else { break };
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        let mut subdirs = Vec::new();
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else { continue };
            let name = entry.file_name().to_string_lossy().to_lowercase();
            let path = entry.path();
            if kind.is_dir() && depth < 4 && !IGNORED_DIRS.contains(&name.as_str()) {
                subdirs.push((!is_bin_dir(&name), path));
            } else if kind.is_file() && name.ends_with(".exe") && !is_helper(&name) {
                let launch_tool = name.contains("launcher") || name.contains("editor");
                let shipping = name.ends_with("-shipping.exe");
                let binary_folder = dir.file_name().is_some_and(|n| is_bin_dir(&n.to_string_lossy().to_lowercase()));
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                found.push(((!launch_tool, shipping, binary_folder, size), path));
            }
        }
        subdirs.sort_unstable();
        for (other, path) in subdirs.into_iter().rev() {
            if other { pending.push_back((path, depth + 1)); }
            else { pending.push_front((path, depth + 1)); }
        }
    }
    found.sort_unstable_by(|(a, pa), (b, pb)| b.cmp(a).then_with(|| pa.cmp(pb)));
    let found: Vec<_> = found.into_iter().map(|(_, path)| path).collect();
    let best = found.first().cloned();
    (best, found)
}

fn read_head(p: &Path, cap: u64) -> Vec<u8> {
    use std::io::Read;
    let f = match fs::File::open(p) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };
    let mut buf = Vec::new();
    let _ = f.take(cap).read_to_end(&mut buf);
    buf
}

fn has_bytes(hay: &[u8], needle: &str) -> bool {
    let n = needle.as_bytes();
    if hay.len() < n.len() {
        return false;
    }
    hay.windows(n.len()).any(|w| w.eq_ignore_ascii_case(n))
}

/// Valor: tabela de certificado (Authenticode) presente no cabecalho PE.
/// Nao valida a cadeia; so evita instalar um arquivo que nem assinado e.
fn pe_signed(p: &Path) -> bool {
    let h = read_head(p, 8192);
    if h.len() < 0x40 || &h[0..2] != b"MZ" {
        return false;
    }
    let pe = u32::from_le_bytes([h[0x3c], h[0x3d], h[0x3e], h[0x3f]]) as usize;
    if pe + 24 > h.len() || &h[pe..pe + 4] != b"PE\0\0" {
        return false;
    }
    let opt = pe + 24;
    if opt + 2 > h.len() {
        return false;
    }
    let magic = u16::from_le_bytes([h[opt], h[opt + 1]]);
    let dirs = match magic {
        0x20b => opt + 0x70, // PE32+
        0x10b => opt + 0x60, // PE32
        _ => return false,
    };
    let cert = dirs + 4 * 8;
    if cert + 8 > h.len() {
        return false;
    }
    let size = u32::from_le_bytes([h[cert + 4], h[cert + 5], h[cert + 6], h[cert + 7]]);
    size > 0
}

/// Confere se o arquivo do kit e mesmo o modelo da DLSS 5. Um `nvngx_dlssd.dll`
/// renomeado (46 MB) instala sem erro e nao faz nada; por isso barramos pelo
/// tamanho e pela assinatura antes de copiar 254 MB para a pasta do jogo.
fn model_check(p: &Path) -> Result<String, String> {
    let sz = fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    let mb = sz as f64 / (1024.0 * 1024.0);
    if sz < 150 * 1024 * 1024 {
        return Err(format!(
            "nvngx_dlssnr.dll no kit tem so {mb:.0} MB (o modelo da DLSS 5 tem ~254 MB).\n  \
             Um nvngx_dlssd.dll renomeado nao serve: e outro modelo.\n  \
             Baixe de novo: .\\tools\\fetch-dlss5.ps1 -Force"
        ));
    }
    if !pe_signed(p) {
        return Err(format!(
            "nvngx_dlssnr.dll ({mb:.0} MB) nao tem assinatura no cabecalho PE.\n  \
             O modelo oficial e assinado pela NVIDIA; um arquivo sem assinatura pode\n  \
             ser dump corrompido. Se for build comunitario de proposito, use --allow-unsigned."
        ));
    }
    Ok(format!("{mb:.0} MB, assinado"))
}

/// manifest.json do que aquele comando colocou na pasta do executável:
/// arquivo, tamanho e ação. Igual os manifests dos outros comandos.
fn write_manifest(dest: &Path, record: &[String]) {
    let mut files: Vec<(String, u64, bool)> = Vec::new();
    for l in record.iter().filter(|l| l.starts_with("F|")) {
        let p: Vec<&str> = l.split('|').collect();
        let sz = p.get(2).copied().unwrap_or("").parse::<u64>().unwrap_or(0);
        files.push((p[1].to_string(), sz, false));
        // o backup, se houve, também tem historico aqui:
        if let Some(b) = p.get(3).filter(|b| !b.is_empty()) {
            let bp = dest.join(MARK).join(b);
            let bsz = fs::metadata(&bp).map(|m| m.len()).unwrap_or(sz);
            files.push((format!("backup::{}", p[1]), bsz, true));
        }
    }
    files.sort();
    let mut out = String::from("{\n");
    out.push_str(&format!("  \"ver\": \"{}\",\n", env!("CARGO_PKG_VERSION")));
    out.push_str(&format!("  \"gerado_em\": \"{}\",\n", stamp_iso()));
    out.push_str("  \"arquivos\": [\n");
    for (i, (name, sz, is_backup)) in files.iter().enumerate() {
        let comma = if i + 1 < files.len() { "," } else { "" };
        out.push_str(&format!(
            "    {{ \"arquivo\": \"{}\", \"tamanho_bytes\": {}{} }}{}\n",
            name.replace('\\', "\\\\"), sz, if *is_backup { ", \"tipo\": \"backup\"" } else { "" }, comma
        ));
    }
    out.push_str("  ]\n}\n");
    let _ = fs::write(dest.join(MARK).join("manifest.json"), out);
}

fn stamp_iso() -> String {
    // "%Y-%m-%d %H:%M:%S" (compatível com o que aplicamos na GUIA)
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs().to_string()).unwrap_or_default()
}

/// Estimativa do DirectX do jogo pela tabela de imports do .exe e pelo que ha
/// na pasta. OptiScaler cobre os dois, isto e so para o aviso ficar correto.
fn guess_api(exe: &Path, game: &Path) -> &'static str {
    let head = read_head(exe, 200 * 1024 * 1024);
    let dx12 = has_bytes(&head, "d3d12.dll");
    let dx11 = has_bytes(&head, "d3d11.dll");
    if game.join("D3D12Core.dll").is_file() || game.join("D3D12").is_dir() {
        return "DX12";
    }
    match (dx12, dx11) {
        (true, _) => "DX12",
        (false, true) => "DX11",
        _ => {
            if game.join("UnityPlayer.dll").is_file() {
                "DX11 (Unity)"
            } else {
                "desconhecido"
            }
        }
    }
}

fn feeder_api_from_log(log: &str) -> Option<&'static str> {
    log.lines().rev().find_map(|line| {
        let (_, session) = line.split_once("feed: opening ")?;
        // The DX11 path creates its own D3D12 device for NGX. "same-device"
        // identifies a game that already renders in D3D12.
        match session.trim_end_matches(|c: char| c.is_whitespace() || c == '#') {
            "D3D12 session" => Some("DX11"),
            "same-device D3D12 session" => Some("DX12"),
            "D3D12 session (Vulkan transport)" => Some("Vulkan"),
            "D3D12 session (OpenGL transport)" => Some("OpenGL"),
            _ => None,
        }
    })
}

/// Procura DLSS proprio do jogo ate 4 niveis abaixo (Unreal usa Binaries\Win64).
fn own_dlss(game: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
        if depth == 0 || out.len() >= 12 {
            return;
        }
        let rd = match fs::read_dir(dir) {
            Ok(r) => r,
            Err(_) => return,
        };
        let mut subs = Vec::new();
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                let n = p.file_name().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
                if n != MARK && n != "runtime" && n != "cache" {
                    subs.push(p);
                }
            } else if let Some(n) = p.file_name().map(|x| x.to_string_lossy().to_lowercase()) {
                if matches!(n.as_str(), "nvngx_dlss.dll" | "nvngx_dlssg.dll" | "nvngx_dlssd.dll"
                    | "sl.dlss.dll" | "sl.dlss_g.dll" | "sl.dlss_d.dll" | "sl.dlss_nr.dll")
                {
                    out.push(p);
                }
            }
        }
        for s in subs {
            walk(&s, depth - 1, out);
        }
    }
    walk(game, 4, &mut out);
    out
}

fn feeder_recorded(dir: &Path) -> bool {
    dir.join("dlss5-feed.dlss5oneclick").is_file()
        || dir.join("nvngx_dlss.dll.dlss5oneclick").is_file()
}

fn feeder(args: &[String], game: &Path, exe: &Path) -> i32 {
    let dest = exe.parent().unwrap_or(game);
    if game.join(MARK).join("install.txt").is_file() || dest.join(MARK).join("install.txt").is_file() {
        eprintln!("O kit OptiScaler do Forge já está instalado neste jogo. Desfaça essa instalação antes de trocar para Feeder.");
        return 2;
    }
    if has(args, "--undo") && !has(args, "--dry-run") && !feeder_recorded(dest) {
        eprintln!("Nenhuma instalação Feeder registrada pelo DLSS5oneclick nesta pasta.");
        return 2;
    }

    let installer = if has(args, "--oneclick") {
        val(args, "--oneclick").map(PathBuf::from)
    } else {
        std::env::current_exe().ok().and_then(|forge| {
            forge.parent().map(|dir| dir.join("tools").join("dlss5oneclick.exe"))
        })
    };
    let Some(installer) = installer.filter(|p| p.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("exe"))) else {
        eprintln!("A rota Feeder precisa do DLSS5oneclick. Selecione o executável na interface ou use --oneclick <caminho.exe>.");
        eprintln!("Releases: https://github.com/faisalkindi/DLSS5oneclick/releases");
        return 2;
    };
    let installer = match installer.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Falha ao resolver o caminho do DLSS5oneclick. {e}");
            return 2;
        }
    };
    let current = std::env::current_exe().ok().and_then(|p| p.canonicalize().ok());
    if current.as_ref() == Some(&installer) {
        eprintln!("Selecione o executável do DLSS5oneclick.");
        return 2;
    }

    let Ok(target) = exe.canonicalize() else {
        eprintln!("Não foi possível resolver o executável do jogo.");
        return 2;
    };
    println!("  Rota Feeder via DLSS5oneclick");
    println!("  Jogo  {}", strip_verbatim(&target));
    let mut command = Command::new(installer);
    command.arg(strip_verbatim(&target)).args(["--mode=feeder", "--engine=reshade"]);
    if has(args, "--dry-run") || (!has(args, "--yes") && !has(args, "--undo")) {
        command.arg("--check");
    } else if has(args, "--undo") {
        command.arg("--remove");
    }
    // Não herda a opção do instalador que desativa a checagem de anti-cheat.
    command.env_remove("DLSS5ONECLICK_IGNORE_ANTICHEAT");
    match command.status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(e) => {
            eprintln!("Falha ao iniciar o DLSS5oneclick. {e}");
            2
        }
    }
}

// ------------------------------------------------------------------ registro

/// Registro de uma instalacao anterior deste comando: (rel, tamanho, backup).
fn previous_record(game: &Path) -> Vec<(String, String, String)> {
    let p = game.join(MARK).join("install.txt");
    let txt = match fs::read_to_string(p) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    txt.lines()
        .filter_map(|l| {
            let mut it = l.split('|');
            if it.next()? != "F" {
                return None;
            }
            let rel = it.next()?.to_string();
            let size = it.next().unwrap_or("").to_string();
            let backup = it.next().unwrap_or("").to_string();
            Some((rel, size, backup))
        })
        .collect()
}

// ------------------------------------------------------------------ ini

/// Troca uma chave dentro de uma secao especifica, preservando comentarios,
/// ordem e CRLF. Se a chave nao existir, e inserida no fim da secao.
fn ini_set(text: &str, section: &str, key: &str, value: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut done = false;
    for line in text.lines() {
        let t = line.trim().trim_start_matches('\u{feff}');
        if t.starts_with('[') && t.ends_with(']') {
            if cur.eq_ignore_ascii_case(section) && !done {
                out.push(format!("{key}={value}"));
                done = true;
            }
            cur = t[1..t.len() - 1].trim().to_string();
            out.push(line.to_string());
            continue;
        }
        if cur.eq_ignore_ascii_case(section) {
            let k = line.split('=').next().unwrap_or("").trim();
            if !k.starts_with(';') && k.eq_ignore_ascii_case(key) {
                out.push(format!("{key}={value}"));
                done = true;
                continue;
            }
        }
        out.push(line.to_string());
    }
    if cur.eq_ignore_ascii_case(section) && !done {
        out.push(format!("{key}={value}"));
    }
    let mut s = out.join("\r\n");
    s.push_str("\r\n");
    s
}

/// Escala de trabalho do modelo conforme a VRAM. O custo cai com o quadrado:
/// 0.75 gasta ~56% do passe, 0.5 gasta ~25%. A imagem inteira continua cheia.
fn working_scale(vram_mb: u32) -> &'static str {
    if vram_mb >= 15_000 {
        "1.0"
    } else if vram_mb >= 11_000 {
        "0.85"
    } else if vram_mb >= 7_500 {
        "0.75"
    } else {
        "0.5"
    }
}

fn performance_scale(vram_mb: u32, resolution: Option<(u32, u32, u32)>) -> u32 {
    let scale = match vram_mb {
        15_000.. => 85,
        11_000.. => 75,
        7_500.. => 65,
        _ => 50,
    };
    match resolution {
        Some((w, h, _)) if u64::from(w) * u64::from(h) >= 2560 * 1440 => (scale - 10).max(50),
        _ => scale,
    }
}

fn flat_setting(text: &str, key: &str, value: &str) -> String {
    let mut found = false;
    let mut out = String::new();
    for line in text.split_inclusive('\n') {
        if line.split_once('=').is_some_and(|(name, _)| name.trim().eq_ignore_ascii_case(key)) {
            found = true;
            out.push_str(&format!("{key}={value}"));
            if line.ends_with("\r\n") { out.push_str("\r\n"); }
            else if line.ends_with('\n') { out.push('\n'); }
        } else {
            out.push_str(line);
        }
    }
    if !found {
        let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
        if !out.is_empty() && !out.ends_with('\n') { out.push_str(newline); }
        out.push_str(&format!("{key}={value}{newline}"));
    }
    out
}

fn replace_config(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let temp = path.with_extension(format!("forge-{}.tmp", std::process::id()));
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&temp)?;
    let written = file.write_all(text.as_bytes());
    drop(file);
    let result = written.and_then(|_| fs::rename(&temp, path));
    if result.is_err() { let _ = fs::remove_file(temp); }
    result
}

fn unity_screen_key(exe: &Path, dest: &Path) -> Option<String> {
    if !dest.join("UnityPlayer.dll").is_file() { return None; }
    let data = format!("{}_Data", exe.file_stem()?.to_str()?);
    let app = fs::read_to_string(dest.join(data).join("app.info")).ok()?;
    let mut lines = app.lines().map(str::trim);
    let company = lines.next()?.trim_start_matches('\u{feff}');
    let product = lines.next()?;
    if [company, product].iter().any(|name| name.is_empty() || name.contains(['\\', '/', '\0'])) {
        return None;
    }
    Some(format!(r"Software\{company}\{product}"))
}

fn unity_screen_values(key: &str) -> Option<[u32; 3]> {
    let [width, height, native] = UNITY_SCREEN;
    let read = |name| crate::win::reg_dword("HKCU", key, name);
    Some([read(width)?, read(height)?, read(native)?])
}

fn set_unity_screen(key: &str, before: [u32; 3], after: [u32; 3]) -> Result<(), String> {
    let path = format!(r"HKCU\{key}");
    let write = |index: usize, value: u32| {
        crate::win::run("reg.exe", &["add", &path, "/v", UNITY_SCREEN[index],
            "/t", "REG_DWORD", "/d", &value.to_string(), "/f"]).is_some()
    };
    for i in 0..UNITY_SCREEN.len() {
        if before[i] != after[i] && !write(i, after[i]) {
            for j in 0..i {
                if before[j] != after[j] { write(j, before[j]); }
            }
            return Err("Falha ao gravar a resolução no Windows. Confira o perfil salvo antes de tentar novamente.".into());
        }
    }
    Ok(())
}

fn require_game_closed(exe: &Path) -> Result<(), String> {
    let name = exe.file_name().and_then(|name| name.to_str()).ok_or("Executável inválido.")?;
    let filter = format!("IMAGENAME eq {name}");
    let listing = crate::win::run("tasklist.exe", &["/FI", &filter, "/FO", "CSV", "/NH"])
        .ok_or("Não foi possível verificar se o jogo está fechado.")?;
    let prefix = format!("\"{}\",", name.to_lowercase());
    if listing.lines().any(|line| line.to_lowercase().starts_with(&prefix)) {
        return Err("Feche o jogo antes de alterar a instalação ou o perfil.".into());
    }
    Ok(())
}

fn read_dx12_profile(path: &Path) -> Result<Option<([u32; 3], [u32; 3])>, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Falha ao ler o perfil de resolução. {e}")),
    };
    let values: Vec<u32> = text.split_whitespace().map(str::parse).collect::<Result<_, _>>()
        .map_err(|_| "Perfil de resolução inválido.")?;
    match values.as_slice() {
        [w, h, n, tw, th, tn] => Ok(Some(([*w, *h, *n], [*tw, *th, *tn]))),
        _ => Err("Perfil de resolução inválido.".into()),
    }
}

fn tune_dx12_unity(exe: &Path, dest: &Path, dry_run: bool) -> Result<bool, String> {
    let Some((key, current)) = unity_screen_key(exe, dest)
        .and_then(|key| unity_screen_values(&key).map(|values| (key, values))) else {
        println!("Este jogo DX12 não tem um perfil de resolução compatível com o Forge.");
        return Ok(false);
    };
    let profile = dest.join(DX12_PROFILE);
    let saved = read_dx12_profile(&profile)?;
    let (before, already_tuned) = match saved {
        Some((original, applied)) if current == applied => (original, true),
        _ => (current, false),
    };
    let gpu = Machine::detect().gpu.filter(|gpu| gpu.vram_mb > 0)
        .ok_or("A VRAM da GPU não foi detectada.")?;
    let [width, height, native] = before;
    let (width, height) = if native == 1 {
        gpu.res.map_or((width, height), |(w, h, _)| (w, h))
    } else { (width, height) };
    if width > 32_768 || height > 32_768 {
        return Err("A resolução detectada é inválida.".into());
    }
    if width < 1280 || height <= 720 {
        println!("A resolução atual já está em 720p ou abaixo.");
        return Ok(false);
    }
    let percent = if gpu.vram_mb >= 11_000 { 90 } else if gpu.vram_mb >= 7_500 { 84 } else { 75 };
    let wanted_height = (height * percent / 100).max(720);
    let target_height = if width * 9 == height * 16 {
        [2160, 1800, 1440, 1200, 1080, 900, 720].into_iter()
            .find(|h| *h <= wanted_height).unwrap_or(720)
    } else { wanted_height & !1 };
    let target = [(width * target_height / height) & !1, target_height, 0];
    if !dry_run && current != target {
        require_game_closed(exe)?;
        let [w, h, n] = before;
        let [tw, th, tn] = target;
        replace_config(&profile, &format!("{w} {h} {n}\n{tw} {th} {tn}\n"))
            .map_err(|e| format!("Falha ao guardar a resolução anterior. {e}"))?;
        set_unity_screen(&key, current, target)?;
    }
    println!("GPU {} · {} MB de VRAM", gpu.name, gpu.vram_mb);
    println!("Perfil DX12 · resolução do jogo {width} × {height} → {} × {}", target[0], target[1]);
    if dry_run { println!("Prévia · a configuração do jogo foi preservada."); }
    else if already_tuned && current == target { println!("O perfil já está aplicado."); }
    else { println!("Abra o jogo novamente em DX12 e confira a resolução e o tempo por quadro."); }
    println!("Use Restaurar resolução no Forge para voltar à configuração anterior.");
    Ok(true)
}

fn restore_dx12_unity(exe: &Path, dest: &Path) -> Result<(), String> {
    let profile = dest.join(DX12_PROFILE);
    let Some((original, applied)) = read_dx12_profile(&profile)? else {
        return Err("Não há perfil DX12 salvo para este jogo.".into());
    };
    let key = unity_screen_key(exe, dest).ok_or("Não foi possível identificar as preferências deste jogo.")?;
    let current = unity_screen_values(&key).ok_or("Não foi possível ler a resolução atual.")?;
    if current != applied && current != original {
        return Err("A resolução foi alterada depois do Forge. As preferências atuais foram preservadas.".into());
    }
    require_game_closed(exe)?;
    set_unity_screen(&key, current, original)?;
    fs::remove_file(&profile).map_err(|e| format!("Resolução restaurada. Falha ao limpar o perfil salvo. {e}"))?;
    println!("Resolução anterior restaurada. Abra o jogo novamente.");
    Ok(())
}

fn optimize_game(exe: &Path, dest: &Path, dry_run: bool) -> Result<bool, String> {
    let mark = dest.join(MARK);
    let opti = mark.join("install.txt").is_file() && !mark.join(FEEDER_CONSUMER_BACKUP).is_file();
    let feeder = feeder_recorded(dest);
    let path = match (opti, feeder) {
        (true, false) => dest.join("OptiScaler.ini"),
        (false, true) => {
            let log = read_head(&dest.join("dlss5-feed.log"), 2 * 1024 * 1024);
            let recorded = feeder_api_from_log(&String::from_utf8_lossy(&log));
            let api = recorded.unwrap_or_else(|| guess_api(exe, dest));
            if recorded.is_some() {
                println!("API da última sessão do Feeder · {api}");
            } else {
                println!("API estimada pelos arquivos do jogo · {api}");
            }
            if api == "DX12" {
                return tune_dx12_unity(exe, dest, dry_run);
            }
            if api != "DX11" {
                println!("O Feeder só ajusta a resolução de trabalho em DirectX 11.");
                println!("Se o jogo oferecer DirectX 11, escolha esse modo ao iniciar, chegue ao menu, feche o jogo e tente novamente.");
                return Ok(false);
            }
            dest.join("dlss5-feed.cfg")
        }
        (true, true) => return Err("Há registros de OptiScaler e Feeder na mesma pasta. Remova a instalação conflitante antes de otimizar.".into()),
        (false, false) => return Err("Instale o DLSS 5 neste jogo com Ver prévia e Aplicar antes de otimizar.".into()),
    };
    let gpu = Machine::detect().gpu.filter(|gpu| gpu.vram_mb > 0)
        .ok_or("A VRAM da GPU não foi detectada. A configuração anterior foi preservada.")?;
    let scale = performance_scale(gpu.vram_mb, gpu.res);
    let original = fs::read_to_string(&path).map_err(|e| format!("Falha ao ler {}. {e}", path.display()))?;
    let (updated, key, value) = if opti {
        if !original.lines().any(|line| line.trim().trim_start_matches('\u{feff}').eq_ignore_ascii_case("[DlssNr]")) {
            return Err("A seção [DlssNr] não foi encontrada no OptiScaler.ini.".into());
        }
        let value = format!("{:.2}", f64::from(scale) / 100.0);
        let updated = ini_set(&original, "DlssNr", "WorkingScale", &value);
        (updated, "WorkingScale", value)
    } else {
        let value = scale.to_string();
        let updated = flat_setting(&original, "work_resolution", &value);
        let updated = flat_setting(&updated, "quality_preset", "custom");
        (flat_setting(&updated, "auto_profile_applied", "1"), "work_resolution", value)
    };
    if !dry_run && updated != original {
        if opti {
            // O undo reconhece a alteração feita pelo Forge mesmo se o tamanho do INI mudar.
            replace_config(&mark.join(OPTIMIZED_INI), &updated)
                .map_err(|e| format!("Falha ao registrar o perfil. {e}"))?;
        }
        replace_config(&path, &updated).map_err(|e| format!("Falha ao gravar {}. {e}", path.display()))?;
    }
    println!("GPU {} · {} MB de VRAM", gpu.name, gpu.vram_mb);
    if let Some((w, h, _)) = gpu.res { println!("Resolução detectada {w} × {h}"); }
    if dry_run { println!("Prévia · nenhum arquivo foi alterado"); }
    println!("Perfil de desempenho · {key}={value}");
    println!("Configuração em {}", path.display());
    println!("Abra o jogo e compare o tempo por quadro. Este perfil foi escolhido pelo hardware.");
    Ok(true)
}

// ------------------------------------------------------------------ itens

struct Item {
    src: PathBuf,
    rel: String,
    text: Option<String>,
}

#[derive(Default)]
struct OptiProfile {
    settings: Vec<(&'static str, &'static str, &'static str)>,
    files: Vec<&'static str>,
}

fn opti_profile(args: &[String], gpu_name: &str, native_fg: bool) -> Result<OptiProfile, String> {
    const LOADER: &str = "amd_fidelityfx_loader_dx12.dll";
    let mut profile = OptiProfile::default();
    let name = gpu_name.to_ascii_uppercase();
    let rdna4 = name.contains("RADEON") && name.contains("RX 90");
    let rtx = name.contains("RTX");
    if has(args, "--upscaler") {
        let upscaler = match val(args, "--upscaler") {
            Some("auto") => "auto",
            Some("dlss") if rtx => "dlss",
            Some("dlss") => return Err("O upscaler DLSS exige uma GeForce RTX detectada.".into()),
            Some(mode @ ("fsr3" | "fsr4")) => {
                if mode == "fsr4" && !rdna4 {
                    return Err("O FSR 4.0.2 deste kit exige Radeon RX 9000 compatível. Escolha FSR 3.1 ou outro upscaler.".into());
                }
                let index = if mode == "fsr4" { "0" } else { "1" };
                profile.settings.push(("FSR", "UpscalerIndex", index));
                profile.files.extend([LOADER, "amd_fidelityfx_upscaler_dx12.dll"]);
                "ffx"
            }
            Some("xess") => {
                profile.files.push("libxess.dll");
                "xess"
            }
            _ => return Err("Use --upscaler auto, dlss, fsr3, fsr4 ou xess.".into()),
        };
        profile.settings.push(("Upscalers", "Dx12Upscaler", upscaler));
    }
    if has(args, "--framegen") {
        let output = match val(args, "--framegen") {
            Some("off") => "nofg",
            Some(mode @ ("fsr3" | "fsr4")) => {
                if mode == "fsr4" && !rdna4 {
                    return Err("O FSR 4 Frame Generation deste kit exige Radeon RX 9000 compatível. Escolha FSR 3.1 FG ou outro modo.".into());
                }
                let index = if mode == "fsr4" { "0" } else { "1" };
                profile.settings.push(("FSR", "FGIndex", index));
                profile.files.extend([LOADER, "amd_fidelityfx_framegeneration_dx12.dll"]);
                "fsrfg"
            }
            Some("xess") => {
                profile.files.extend(["libxess_fg.dll", "libxell.dll"]);
                "xefg"
            }
            Some("dlss") if rtx && (name.contains("RTX 40") || name.contains("RTX 50")) => {
                profile.files.extend([
                    r"streamline\sl.interposer.dll", r"streamline\sl.common.dll",
                    r"streamline\sl.dlss_g.dll", r"streamline\sl.reflex.dll",
                    r"streamline\sl.pcl.dll", r"streamline\nvngx_dlssg.dll",
                ]);
                "dlssg"
            }
            Some("dlss") => return Err("O DLSS Frame Generation deste perfil exige GeForce RTX 40 ou 50.".into()),
            _ => return Err("Use --framegen off, dlss, fsr3, fsr4 ou xess.".into()),
        };
        let input = if output == "nofg" {
            "nofg"
        } else {
            match val(args, "--fg-input").unwrap_or("auto") {
                "auto" => if native_fg { "dlssg" } else { "upscaler" },
                "dlssg" => "dlssg",
                "fsrfg" => "fsrfg",
                "fsrfg30" => "fsrfg30",
                "upscaler" => "upscaler",
                _ => return Err("Use --fg-input auto, dlssg, fsrfg, fsrfg30 ou upscaler.".into()),
            }
        };
        let enabled = if output == "nofg" { "false" } else { "true" };
        profile.settings.extend([
            ("FrameGen", "Enabled", enabled), ("FrameGen", "FGInput", input),
            ("FrameGen", "FGOutput", output), ("FrameGen", "FGNvngxReplacement", "None"),
        ]);
    } else if has(args, "--fg-input") {
        return Err("Use --fg-input junto com --framegen.".into());
    }
    profile.files.sort_unstable();
    profile.files.dedup();
    Ok(profile)
}

fn overlay_dependencies(profile: &mut OptiProfile, gpu_name: &str) {
    profile.files.extend([
        "amd_fidelityfx_loader_dx12.dll", "amd_fidelityfx_upscaler_dx12.dll",
        "amd_fidelityfx_framegeneration_dx12.dll", "libxess.dll", "libxess_fg.dll", "libxell.dll",
    ]);
    if gpu_name.contains("RTX 40") || gpu_name.contains("RTX 50") {
        profile.files.extend([
            r"streamline\sl.interposer.dll", r"streamline\sl.common.dll",
            r"streamline\sl.dlss_g.dll", r"streamline\sl.reflex.dll",
            r"streamline\sl.pcl.dll", r"streamline\nvngx_dlssg.dll",
        ]);
    }
    profile.files.sort_unstable();
    profile.files.dedup();
    profile.settings.push(("Menu", "ShortcutKey", "0x2D"));
}

fn feeder_overlay_proxy(exe: &Path, game: &Path, prev: &[String]) -> Result<&'static str, String> {
    let reshade = game.join(MARK).join(FEEDER_RESHADE_BACKUP);
    if !game.join("dlss5-feed.addon64").is_file()
        || !(reshade.is_file() || (game.join("dxgi.dll").is_file() && game.join("dxgi.dll.dlss5oneclick").is_file()))
    {
        return Err("Instale primeiro o Feeder de 64 bits com ReShade.".into());
    }
    if game.join(FEEDER_RESHADE).exists() && !prev.iter().any(|p| p.eq_ignore_ascii_case(FEEDER_RESHADE)) {
        return Err("Já existe um ReShade64.dll de outra instalação nesta pasta.".into());
    }
    if reshade.is_file() && game.join("dxgi.dll").exists() {
        return Err("Outro dxgi.dll foi instalado após a migração do ReShade. Resolva o conflito antes de aplicar.".into());
    }
    let log = read_head(&game.join("dlss5-feed.log"), 2 * 1024 * 1024);
    let api = feeder_api_from_log(&String::from_utf8_lossy(&log)).unwrap_or_else(|| guess_api(exe, game));
    if api != "DX12" {
        return Err("Abra e feche o jogo em DirectX 12 antes de preparar o menu do OptiScaler.".into());
    }
    if !game.join(MARK).join(FEEDER_CONSUMER_BACKUP).is_file()
        && !(game.join(FEEDER_CONSUMER).is_file() && game.join("renodx-dlss5.addon64.dlss5oneclick").is_file())
    {
        return Err("O Forge só troca o consumidor RenoDX registrado pelo OneClick.".into());
    }
    let image = read_head(exe, 128 * 1024 * 1024);
    let unity = read_head(&game.join("UnityPlayer.dll"), 64 * 1024 * 1024);
    ["winmm.dll", "version.dll"].into_iter().find(|name| {
        prev.iter().any(|rel| rel.eq_ignore_ascii_case(name))
            || (!game.join(name).exists() && (has_bytes(&image, name) || has_bytes(&unity, name)))
    }).ok_or_else(|| "Nenhum carregador livre compatível com o jogo foi detectado (winmm.dll ou version.dll).".into())
}

fn build_items(kit: &Path, game: &Path, prev: &[String], scale: &str, profile: &OptiProfile, overlay_proxy: Option<&str>) -> Result<Vec<Item>, String> {
    let opti_dir = if overlay_proxy.is_some() { kit.join("feeder") } else { kit.to_path_buf() };
    let opti = opti_dir.join("OptiScaler.dll");
    let snip = kit.join("nvngx.dll_dlssnr.dll");
    let model = kit.join("nvngx_dlssnr.dll");
    let ini = opti_dir.join("OptiScaler.ini");

    for p in [&opti, &model, &ini].into_iter().chain(overlay_proxy.is_none().then_some(&snip)) {
        if !p.is_file() {
            let missing = p.strip_prefix(kit).unwrap_or(p);
            return Err(format!("Faltando no kit: {}. Atualize o kit em Ferramentas.", missing.display()));
        }
    }
    // Nome de proxy livre. Um dxgi.dll de outro mod nunca e sobrescrito.
    let mut proxy = overlay_proxy;
    for name in PROXIES {
        if proxy.is_some() { break; }
        let taken = game.join(name).exists();
        let ours = prev.iter().any(|p| p.eq_ignore_ascii_case(name));
        if !taken || ours {
            proxy = Some(name);
            break;
        }
    }
    let proxy = match proxy {
        Some(p) => p,
        None => {
            return Err(
                "todos os nomes de proxy (dxgi.dll, winmm.dll, d3d12.dll...) ja estao ocupados \
                 por outros mods nesta pasta"
                    .to_string(),
            )
        }
    };

    let mut items = vec![Item { src: opti, rel: proxy.to_string(), text: None }];
    if overlay_proxy.is_some() {
        let backup = game.join(MARK).join(FEEDER_RESHADE_BACKUP);
        let src = if backup.is_file() { backup } else { game.join("dxgi.dll") };
        items.push(Item { src, rel: FEEDER_RESHADE.to_string(), text: None });
    } else {
        items.push(Item { src: snip, rel: "nvngx.dll_dlssnr.dll".to_string(), text: None });
    }
    items.push(Item { src: model, rel: "nvngx_dlssnr.dll".to_string(), text: None });

    // O jogo precisa de um nvngx_dlss.dll local para o OptiScaler interceptar.
    // Se so tem o do driver, copiamos o nosso (assinado, 310.9.16).
    let local_dlss = game.join("nvngx_dlss.dll");
    let has_local = local_dlss.is_file()
        || prev.iter().any(|p| p.eq_ignore_ascii_case("nvngx_dlss.dll"));
    if !has_local {
        let mut cands = Vec::new();
        if let Some(runtime) = crate::distribution::runtime_dir() {
            cands.push(runtime.join("nvngx_dlss.dll"));
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(p) = exe.parent() {
                cands.push(p.join("runtime").join("nvngx_dlss.dll"));
            }
        }
        if let Ok(cwd) = std::env::current_dir() {
            cands.push(cwd.join("runtime").join("nvngx_dlss.dll"));
        }
        if let Some(src) = cands.into_iter().find(|p| p.is_file()) {
            items.push(Item { src, rel: "nvngx_dlss.dll".to_string(), text: None });
        }
    }

    let installed_ini = game.join("OptiScaler.ini");
    let source_ini = if prev.iter().any(|rel| rel.eq_ignore_ascii_case("OptiScaler.ini")) && installed_ini.is_file() {
        &installed_ini
    } else { &ini };
    let ini_txt = fs::read_to_string(source_ini).map_err(|e| format!("nao consegui ler OptiScaler.ini: {e}"))?;
    let ini_txt = ini_set(&ini_txt, "DlssNr", "Enabled", "true");
    let ini_txt = ini_set(&ini_txt, "DlssNr", "WorkingScale", scale);
    let mut ini_txt = ini_set(&ini_txt, "ProcessFilter", "TargetProcessName", "auto");
    for (section, key, value) in &profile.settings {
        let header = format!("[{section}]");
        if !ini_txt.lines().any(|line| line.trim().trim_start_matches('\u{feff}').eq_ignore_ascii_case(&header)) {
            ini_txt.push_str(&format!("\r\n[{section}]\r\n"));
        }
        ini_txt = ini_set(&ini_txt, section, key, value);
    }
    for name in &profile.files {
        let rel = format!(r"OptiScaler\{name}");
        if game.join(&rel).is_file() { continue; }
        let src = if let Some(name) = name.strip_prefix(r"streamline\") {
            kit.parent().unwrap_or(kit).join(name)
        } else { kit.join(&rel) };
        if game.join(&rel).exists() || !src.is_file() {
            return Err(format!("Dependência indisponível: {rel}. Atualize o kit em Ferramentas ou execute tools\\fetch-runtime.ps1 para o Streamline."));
        }
        items.push(Item { src, rel, text: None });
    }
    items.push(Item { src: ini, rel: "OptiScaler.ini".to_string(), text: Some(ini_txt) });

    let core = kit.join("D3D12_OptiScaler").join("D3D12Core.dll");
    if core.is_file() {
        items.push(Item {
            src: core,
            rel: r"OptiScaler\D3D12_OptiScaler\D3D12Core.dll".to_string(),
            text: None,
        });
    }

    Ok(items)
}

// ------------------------------------------------------------------ install

fn usage() {
    println!("  {}", bold("forge dlss5 --game \"<pasta do jogo>\" [opcoes]"));
    println!();
    println!("  {}", dim("A pasta e a que tem o .exe do jogo (Unreal: ...\\Binaries\\Win64)."));
    println!();
    println!("  {}", bold("OPCOES"));
    println!("    {}  aplica de verdade (sem isso so mostra o plano)", pad("--yes", 20));
    println!("    {}  desfaz a instalação pela rota registrada", pad("--undo", 20));
    println!("    {}  ajusta o perfil de desempenho da instalação existente", pad("--optimize", 20));
    println!("    {}  restaura a resolução anterior do jogo Unity em DX12", pad("--restore-resolution", 20));
    println!("    {}  usa ReShade + Feeder via DLSS5oneclick", pad("--feeder", 20));
    println!("    --upscaler <auto|dlss|fsr3|fsr4|xess>    OptiScaler em DX12");
    println!("    --framegen <off|dlss|fsr3|fsr4|xess>      saída de Frame Generation em DX12");
    println!("    --fg-input <auto|upscaler|dlssg|fsrfg|fsrfg30>");
    println!("    --route                 identifica a rota de instalação sem alterar arquivos");
    println!("    --in-game-menu          prepara o overlay completo do OptiScaler em DX12");
    println!("    {}  caminho do instalador externo (ou tools\\dlss5oneclick.exe)", pad("--oneclick EXE", 20));
    println!("    {}  mostra o plano e sai", pad("--dry-run", 20));
    println!("    {}  escala do modelo, 0.1 a 1.0 (padrao: pela VRAM)", pad("--scale N", 22));
    println!("    {}  pasta do kit (padrao: runtime\\dlss5)", pad("--from DIR", 22));
    println!("    {}  mantém o OptiScaler sem DLSS detectado (modo legado)", pad("--force", 22));
    println!("    {}  aceita modelo sem assinatura no kit", pad("--allow-unsigned", 22));
    println!();
    println!("  {}", bold("ANTES"));
    println!("    {}", cya(".\\tools\\fetch-dlss5.ps1"));
    println!("    {}", dim("baixa uma vez OptiScaler + o modelo (~300 MB) para runtime\\dlss5"));
    println!();
}

pub fn cmd(args: &[String]) -> i32 {
    if has(args, "--help") || has(args, "-h") {
        usage();
        return 0;
    }
    let game_arg = match val(args, "--game") {
        Some(g) => g.to_string(),
        None => {
            usage();
            return 2;
        }
    };
    let game = match resolve_dir(&game_arg) {
        Ok(d) => d,
        Err(e) => {
            println!("{} {e}", red("erro:"));
            return 2;
        }
    };

    let selected = Path::new(&game_arg);
    let (exe, all_exes) = if selected.is_file() && selected.extension().is_some_and(|x| x.eq_ignore_ascii_case("exe")) {
        (Some(selected.to_path_buf()), Vec::new())
    } else {
        find_exe(&game)
    };
    let Some(exe) = exe else {
        if has(args, "--undo") && !has(args, "--feeder") {
            return undo(args);
        }
        eprintln!("Nenhum executável de jogo encontrado. Aponte para o .exe ou para a pasta dele.");
        return 2;
    };
    let dest = exe.parent().unwrap_or(&game);
    if has(args, "--restore-resolution") {
        if has(args, "--dry-run") || has(args, "--yes") || has(args, "--undo") || has(args, "--optimize") {
            eprintln!("Use --restore-resolution como ação separada.");
            return 2;
        }
        return match restore_dx12_unity(&exe, dest) {
            Ok(()) => 0,
            Err(e) => { eprintln!("{e}"); 2 }
        };
    }
    if has(args, "--optimize") {
        if has(args, "--undo") || has(args, "--yes") {
            eprintln!("Use --optimize como uma ação separada de --undo e --yes.");
            return 2;
        }
        return match optimize_game(&exe, dest, has(args, "--dry-run")) {
            Ok(true) => 0,
            Ok(false) => 3,
            Err(e) => { eprintln!("{e}"); 2 }
        };
    }
    let prev_rec = previous_record(dest);
    let prev: Vec<String> = prev_rec.iter().map(|(r, _, _)| r.clone()).collect();
    let oneclick_installed = feeder_recorded(dest);
    let hybrid_installed = dest.join(MARK).join(FEEDER_CONSUMER_BACKUP).is_file();
    let menu_requested = has(args, "--in-game-menu");
    if has(args, "--undo") && hybrid_installed && !has(args, "--route") {
        return undo(args);
    }
    let opti_options = ["--upscaler", "--framegen", "--fg-input"].iter().any(|option| has(args, option));
    if !has(args, "--route") && has(args, "--undo") && !has(args, "--feeder") && (game.join(MARK).join("install.txt").is_file()
        || dest.join(MARK).join("install.txt").is_file() || !oneclick_installed)
    {
        return undo(args);
    }
    let feeder_selected = has(args, "--feeder") || oneclick_installed;
    let dlss = if feeder_selected { Vec::new() } else { own_dlss(&game) };
    let has_native = dlss.iter().any(|p| {
        p != &dest.join("nvngx_dlss.dll")
            || !prev_rec.iter().any(|(rel, _, backup)| rel.eq_ignore_ascii_case("nvngx_dlss.dll") && backup.is_empty())
    });
    let force = has(args, "--force");
    let installed_opti = dest.join(MARK).join("install.txt").is_file();
    let use_feeder = feeder_selected || (!has_native && !force && !installed_opti);
    if has(args, "--route") {
        let route = if hybrid_installed {
            "feeder-overlay"
        } else if use_feeder {
            "feeder"
        } else if guess_api(&exe, dest) == "DX11" {
            "opti-dx11"
        } else {
            "opti"
        };
        println!("{route}");
        return 0;
    }
    let feeder_overlay = oneclick_installed && (menu_requested || hybrid_installed);
    if use_feeder && !feeder_overlay {
        if menu_requested {
            eprintln!("Instale primeiro o Feeder pelo Forge e depois habilite o menu do OptiScaler.");
            return 2;
        }
        if opti_options {
            eprintln!("O jogo usa Feeder. Upscaling e Frame Generation do OptiScaler estão indisponíveis nesta rota.");
            return 2;
        }
        if !feeder_selected {
            println!("DLSS nativo não detectado. Selecionando a rota Feeder.");
        }
        return feeder(args, &game, &exe);
    }

    let kit = match kit_dir(args) {
        Some(k) => k,
        None => {
            println!("{} kit da DLSS 5 nao encontrado (runtime\\dlss5).", red("erro:"));
            println!("  {}", bold("Baixe uma vez:"));
            println!("    {}", cya(".\\tools\\fetch-dlss5.ps1"));
            println!();
            println!("  {}", dim("Ele traz OptiScaler + o modelo nvngx_dlssnr.dll e valida as assinaturas."));
            return 2;
        }
    };

    // Confere o modelo antes de anunciar qualquer coisa: sem isso um arquivo
    // errado no kit so apareceria como jogo que "nao faz nada".
    let model_info = match model_check(&kit.join("nvngx_dlssnr.dll")) {
        Ok(s) => s,
        Err(e) if has(args, "--allow-unsigned") => {
            eprintln!("  {} {e}", yel("aviso:"));
            "tamanho/assinatura nao confirmados (--allow-unsigned)".to_string()
        }
        Err(e) => {
            println!("{} {e}", red("erro:"));
            return 2;
        }
    };

    let yes = has(args, "--yes");
    let dry = has(args, "--dry-run");
    let machine = Machine::detect();
    let vram = machine.gpu.as_ref().map(|g| g.vram_mb).unwrap_or(0);
    let scale = match val(args, "--scale") {
        Some(s) => match s.parse::<f32>() {
            Ok(v) if (0.1..=1.0).contains(&v) => format!("{v:.2}"),
            _ => {
                println!("{} --scale aceita 0.1 a 1.0 (ex: --scale 0.5)", red("erro:"));
                return 2;
            }
        },
        None => working_scale(vram).to_string(),
    };

    let api = guess_api(&exe, dest);
    if (opti_options || menu_requested) && api == "DX11" && !feeder_overlay {
        eprintln!("Os perfis de upscaling e Frame Generation do Forge são para o OptiScaler em DirectX 12.");
        return 2;
    }
    let native_fg = dlss.iter().any(|p| {
        !p.starts_with(dest.join("OptiScaler"))
            && p.file_name().is_some_and(|name| name.eq_ignore_ascii_case("sl.dlss_g.dll"))
    });
    let gpu_name = machine.gpu.as_ref().map(|gpu| gpu.name.as_str()).unwrap_or_default();
    let mut profile = match opti_profile(args, gpu_name, native_fg) {
        Ok(profile) => profile,
        Err(e) => { eprintln!("{e}"); return 2; }
    };
    if menu_requested || feeder_overlay { overlay_dependencies(&mut profile, gpu_name); }
    let overlay_proxy = if feeder_overlay {
        let proxy = match feeder_overlay_proxy(&exe, dest, &prev) {
            Ok(proxy) => proxy,
            Err(e) => { eprintln!("{e}"); return 2; }
        };
        if !has(args, "--upscaler") && !hybrid_installed {
            profile.settings.push(("Upscalers", "Dx12Upscaler", "dlss"));
        }
        if !dest.join(MARK).join(FEEDER_RESHADE_BACKUP).is_file() || val(args, "--framegen") == Some("dlss") {
            profile.settings.push(("DLSSG", "UseGamesReflexMarkers", "false"));
        }
        profile.settings.extend([
            ("Plugins", "LoadReshade", "true"), ("Inputs", "EnableDlssInputs", "true"),
            ("Hooks", "HookOriginalNvngxOnly", "false"),
        ]);
        println!("  OptiScaler via {proxy} (Insert) carrega o ReShade e o Feeder.");
        if !dest.join(MARK).join(FEEDER_RESHADE_BACKUP).is_file() {
            println!("  O dxgi.dll do OneClick será guardado para restaurar o carregamento original ao desfazer.");
        }
        println!("  RenoDX será guardado para evitar dois consumidores neurais.");
        Some(proxy)
    } else { None };

    crate::report::header(env!("CARGO_PKG_VERSION"));
    println!("  {} {}", bold("jogo   "), game.display());
    if dest != game {
        println!("  {} {}", bold("destino"), dest.display());
    }
    println!("  {} {}", bold("exe    "), exe.display());
    println!("  {} {}", bold("kit    "), kit.display());
    println!("  {} {}", bold("modelo "), model_info);
    println!("  {} {}   {} {}", bold("DirectX"), api, bold("escala"), scale);
    println!();

    if all_exes.len() > 1 {
        println!("  {} outros executaveis na pasta:", dim("nota:"));
        for e in all_exes.iter().skip(1).take(3) {
            println!("    {}", dim(e.file_name().unwrap_or_default().to_string_lossy()));
        }
        println!();
    }

    if has_native {
        let sample = dlss
            .iter()
            .find(|p| {
                p.file_name()
                    .map(|n| n.to_string_lossy().eq_ignore_ascii_case("nvngx_dlss.dll"))
                    .unwrap_or(false)
            })
            .or_else(|| dlss.first())
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        println!("  {} DLSS do jogo encontrado ({})", grn("ok:"), sample);
    }

    let items = match build_items(&kit, dest, &prev, &scale, &profile, overlay_proxy) {
        Ok(i) => i,
        Err(e) => {
            println!("{} {e}", red("erro:"));
            return 2;
        }
    };
    if !profile.settings.is_empty() {
        println!("  Perfil OptiScaler em DX12");
        for (section, key, value) in &profile.settings {
            println!("    [{section}] {key}={value}");
        }
        if profile.settings.contains(&("FrameGen", "Enabled", "true")) {
            println!("  Reinicie o jogo e confira Frame Generation no overlay do OptiScaler (Insert).");
        }
    }

    // ------------------------------------------------------------- tabela
    let total: u64 = items
        .iter()
        .map(|i| {
            i.text
                .as_ref()
                .map(|t| t.len() as u64)
                .unwrap_or_else(|| fs::metadata(&i.src).map(|m| m.len()).unwrap_or(0))
        })
        .sum();

    println!();
    println!("  {} {} {}", pad("arquivo", 44), pad("MB", 7), "acao");
    for i in &items {
        let sz = i
            .text
            .as_ref()
            .map(|t| t.len() as u64)
            .unwrap_or_else(|| fs::metadata(&i.src).map(|m| m.len()).unwrap_or(0));
        let dst = dest.join(&i.rel);
        // Um arquivo que ja esta no registro anterior e nosso: reaplicar nao e
        // "substituir arquivo do jogo", e so atualizar o kit. O backup original
        // (se havia) continua valendo.
        let nosso = prev.iter().any(|p| p.eq_ignore_ascii_case(&i.rel));
        let acao = if nosso {
            "atualizar (do forge)"
        } else if dst.exists() {
            "substituir (com backup)"
        } else {
            "criar"
        };
        println!(
            "  {} {:>7.1} {}",
            pad(&i.rel, 44),
            sz as f64 / (1024.0 * 1024.0),
            if acao.starts_with("substituir") { yel(acao) } else { cya(acao) }
        );
    }
    println!();
    println!("  {} {:.0} MB no total", bold("tamanho:"), total as f64 / (1024.0 * 1024.0));

    if !has_native && !feeder_overlay {
        println!();
        println!("  {}", yel("--force usado: o jogo nao tem DLSS detectado."));
    }

    println!();
    println!("  {}", dim("Anti-cheat: DLL extra pode ser bloqueada. Use so em modo offline."));
    println!("  {}", dim("O jogo original nunca e alterado; so a pasta recebe arquivos novos."));

    if dry || !yes {
        println!();
        if dry {
            println!("  {}", bold("dry-run: nada foi alterado."));
        } else {
            println!("  {} {}", bold("nada foi alterado."), "Confirme com --yes:");
            println!(
                "    {}",
                dim(&format!(
                    "forge dlss5 --game \"{}\" --yes",
                    game.display()
                ))
            );
        }
        println!();
        return 0;
    }
    if opti_options || menu_requested || feeder_overlay {
        if let Err(e) = require_game_closed(&exe) {
            eprintln!("{e}");
            return 2;
        }
    }

    // ------------------------------------------------------------- backup
    let bk = dest.join(MARK).join(format!("backup-{}", stamp()));
    let mut record: Vec<String> = Vec::new();
    let mut criados = Vec::new();
    let mut ok = 0;
    let mut erros = 0;
    let mut dirs = Vec::new();
    let reshade_backup = dest.join(MARK).join(FEEDER_RESHADE_BACKUP);
    let migrate_reshade = feeder_overlay && !reshade_backup.is_file();
    let mut reshade_migrated = false;
    if feeder_overlay && !hybrid_installed {
        let mark = dest.join(MARK);
        if let Err(e) = fs::create_dir_all(&mark).and_then(|_| {
            fs::rename(dest.join(FEEDER_CONSUMER), mark.join(FEEDER_CONSUMER_BACKUP))
        }) {
            eprintln!("Não foi possível guardar o RenoDX. {e}");
            return 2;
        }
    }

    for i in &items {
        let dst = dest.join(&i.rel);
        if (opti_options || menu_requested || feeder_overlay) && i.rel == "OptiScaler.ini" && erros > 0 {
            println!("  Configuração preservada porque uma dependência falhou.");
            continue;
        }
        if let Some(parent) = dst.parent() {
            if parent != dest {
                // registra so as pastas que ESTE comando criou, para o undo
                // poder apaga-las sem tocar em pasta que ja existia
                let mut chain: Vec<PathBuf> = Vec::new();
                let mut cur = parent.to_path_buf();
                while cur != dest && !cur.exists() {
                    chain.push(cur.clone());
                    match cur.parent() {
                        Some(p) => cur = p.to_path_buf(),
                        None => break,
                    }
                }
                for d in chain.iter().rev() {
                    if fs::create_dir_all(d).is_ok() {
                        if let Ok(rel) = d.strip_prefix(&dest) {
                            let rel = rel.display().to_string();
                            if !dirs.contains(&rel) {
                                dirs.push(rel);
                            }
                        }
                    }
                }
            }
        }

        let mut backup_rel = String::new();
        // Arquivo que ja estava no registro anterior e nosso: nao gera backup
        // novo, reaproveita o backup ORIGINAL para o undo continuar voltando
        // ao estado de fabrica mesmo depois de reaplicar.
        let prior = prev_rec.iter().find(|(r, _, _)| r == &i.rel);
        if let Some((_, _, pb)) = prior {
            backup_rel = pb.clone();
        } else if dst.exists() {
            let bdst = bk.join(&i.rel);
            if let Some(p) = bdst.parent() {
                let _ = fs::create_dir_all(p);
            }
            if fs::copy(&dst, &bdst).is_err() {
                println!("  {} backup falhou: {}", red("X"), i.rel);
                erros += 1;
                continue;
            }
            backup_rel = bdst
                .strip_prefix(dest.join(MARK))
                .map(|p| p.display().to_string())
                .unwrap_or_default();
        }

        if migrate_reshade && i.rel == "OptiScaler.ini" {
            if let Err(e) = fs::rename(dest.join("dxgi.dll"), &reshade_backup) {
                eprintln!("Não foi possível migrar o ReShade. {e}");
                erros += 1;
                continue;
            }
            reshade_migrated = true;
        }
        let res = match &i.text {
            Some(t) => fs::write(&dst, t).map_err(|e| e.to_string()),
            None => fs::copy(&i.src, &dst).map(|_| ()).map_err(|e| e.to_string()),
        };
        match res {
            Ok(_) => {
                ok += 1;
                let sz = fs::metadata(&dst).map(|m| m.len()).unwrap_or(0);
                criados.push((i.rel.clone(), sz));
                record.push(format!("F|{}|{}|{}", i.rel, sz, backup_rel));
            }
            Err(e) => {
                println!("  {} {}: {}", red("X"), i.rel, e);
                erros += 1;
            }
        }
    }
    if erros > 0 && reshade_migrated {
        if let Err(e) = fs::rename(&reshade_backup, dest.join("dxgi.dll")) {
            eprintln!("Não foi possível restaurar o dxgi.dll. O backup foi preservado. {e}");
        }
    }

    for d in &dirs {
        record.push(format!("D|{d}"));
    }
    // Perfis novos não retiram DLLs opcionais de um perfil anterior.
    if let Ok(previous) = fs::read_to_string(dest.join(MARK).join("install.txt")) {
        let mut keys: std::collections::HashSet<_> = record.iter()
            .filter_map(|line| line.split('|').nth(1).map(str::to_ascii_lowercase))
            .collect();
        for line in previous.lines() {
            if let Some(path) = line.split('|').nth(1) {
                if keys.insert(path.to_ascii_lowercase()) { record.push(line.to_owned()); }
            }
        }
    }

    let _ = fs::create_dir_all(dest.join(MARK));
    let _ = fs::write(
        dest.join(MARK).join("install.txt"),
        format!("{}\r\n", record.join("\r\n")),
    );
    write_manifest(&dest, &record);
    if erros > 0 && feeder_overlay && !hybrid_installed {
        eprintln!("A instalação do overlay falhou. Restaurando o consumidor anterior.");
        let _ = undo(args);
        return 1;
    }

    // preferencia de GPU de alto desempenho para o exe do jogo
    let exe_s = strip_verbatim(&exe);
    if !feeder_overlay {
        crate::win::run(
            "reg.exe",
            &["add", REG_GPU, "/v", &exe_s, "/t", "REG_SZ", "/d", "GpuPreference=2;", "/f"],
        );
    }

    // guia para o usuario
    let guia = format!(
        "DLSS 5 aplicada pelo DLSS5 Forge\r\n\
         =============================\r\n\r\n\
         Jogo: {}\r\n\
         Arquivos colocados: {}\r\n\r\n\
         NO JOGO:\r\n\
         1. Abra o jogo. Se o overlay do OptiScaler nao aparecer, aperte Insert.\r\n\
         2. Confira em DLSS Neural Rendering que 'Enable Neural Rendering' esta ligado.\r\n\
         3. F6 liga/desliga a DLSS 5 para comparar.\r\n\
         4. Em DLSS Super Resolution, deixe o jogo em Qualidade ou Equilibrado.\r\n\
         5. Desligue MSAA/SSAA do jogo (conflita com o upscaler).\r\n\r\n\
         Custo: o passe neural gasta frametime. Se cair demais, reduza\r\n\
         'Model resolution' no overlay (ou WorkingScale no OptiScaler.ini).\r\n\r\n\
         DESFAZER (restaura os originais):\r\n\
           forge.exe dlss5 --game \"{}\" --undo\r\n",
        dest.display(),
        ok,
        dest.display()
    );
    let _ = fs::write(dest.join(MARK).join("COMO-DESFAZER.txt"), guia);

    println!();
    println!(
        "  {} {} arquivo(s) aplicado(s), {} erro(s)",
        if erros == 0 { grn("pronto:") } else { yel("parcial:") },
        ok,
        erros
    );
    if ok > 0 {
        println!("  {} {}", dim("backup  "), bk.display());
        println!("  {} {}", dim("registro"), dest.join(MARK).join("install.txt").display());
        println!("  {} {}", dim("manifest"), dest.join(MARK).join("manifest.json").display());
    }
    println!();
    println!("  {}", bold("NO JOGO"));
    println!("    {} o overlay do OptiScaler abre com Insert", dim("1."));
    println!("    {} 'Enable Neural Rendering' deve estar ligado (ja deixamos ligado)", dim("2."));
    println!("    {} F6 compara com/sem DLSS 5", dim("3."));
    println!("    {} escala do modelo: {} (VRAM {} MB)", dim("4."), cya(&scale), vram);
    println!();
    println!("  {}", bold("se cair o fps"));
    println!("    {}", dim("baixe 'Model resolution' no overlay: 0.75 -> 0.5 corta ~50% do passe"));
    println!();
    println!("  {}", bold("desfazer"));
    println!(
        "    {}",
        dim(&format!("forge dlss5 --game \"{}\" --undo", dest.display()))
    );
    println!();

    if erros == 0 {
        0
    } else {
        1
    }
}

// ------------------------------------------------------------------ testes

#[cfg(test)]
mod tests {
    use super::*;

    const INI: &str = "[DlssNr]\r\n; comentario que deve ficar\r\nEnabled=auto\r\nWorkingScale=auto\r\n\r\n[ProcessFilter]\r\nTargetProcessName=auto\r\n";

    #[test]
    fn ini_troca_so_a_chave_da_secao() {
        let a = ini_set(INI, "DlssNr", "Enabled", "true");
        assert!(a.contains("Enabled=true"));
        assert!(a.contains("; comentario que deve ficar"));
        // a chave de outra secao nao e tocada
        assert!(a.contains("TargetProcessName=auto"));
        // nao duplica a chave
        assert_eq!(a.matches("Enabled=").count(), 1);
    }

    #[test]
    fn ini_insere_chave_que_nao_existe() {
        let a = ini_set(INI, "DlssNr", "WorkingScale", "0.75");
        assert!(a.contains("WorkingScale=0.75"));
        let b = ini_set(&a, "DlssNr", "TransferStrength", "0.5");
        assert!(b.contains("TransferStrength=0.5"));
        assert_eq!(b.matches("WorkingScale=").count(), 1);
    }

    #[test]
    fn ini_preserva_crlf() {
        let a = ini_set(INI, "DlssNr", "Enabled", "true");
        assert!(a.contains("\r\n"));
        // nenhum \n solto: todo \n tem \r antes
        let bytes = a.as_bytes();
        for (i, b) in bytes.iter().enumerate() {
            if *b == b'\n' {
                assert!(i > 0 && bytes[i - 1] == b'\r', "quebra de linha LF solta");
            }
        }
        assert!(a.ends_with("\r\n"));
    }

    #[test]
    fn escala_cai_com_a_vram() {
        assert_eq!(working_scale(16_000), "1.0");
        assert_eq!(working_scale(12_000), "0.85");
        assert_eq!(working_scale(8_151), "0.75");
        assert_eq!(working_scale(4_000), "0.5");
        // monotona: menos VRAM nunca pede escala maior
        let v = [24_000, 15_000, 14_000, 11_000, 8_000, 7_500, 7_000, 4_000];
        let s: Vec<f32> = v.iter().map(|x| working_scale(*x).parse().unwrap()).collect();
        for w in s.windows(2) {
            assert!(w[0] >= w[1], "escala subiu quando a VRAM caiu");
        }
    }

    #[test]
    fn helper_de_instalacao_nao_e_confundido_com_jogo() {
        for n in ["unins000.exe", "vcredist_x64.exe", "UnityCrashHandler64.exe"] {
            assert!(is_helper(n), "{n} deveria ser ignorado");
        }
        for n in ["Jogo-Win64-Shipping.exe", "Game.exe"] {
            assert!(!is_helper(n), "{n} deveria ser aceito");
        }
    }

    #[test]
    fn feeder_distingue_api_do_jogo_do_dispositivo_ngx() {
        let dx11 = "15:00:00  ### feed: opening D3D12 session ###";
        let dx12 = "15:00:01  ### feed: opening same-device D3D12 session ###";
        let vulkan = "15:00:02  ### feed: opening D3D12 session (Vulkan transport) ###";
        assert_eq!(feeder_api_from_log(dx11), Some("DX11"));
        assert_eq!(feeder_api_from_log(&format!("{dx11}\n{dx12}")), Some("DX12"));
        assert_eq!(feeder_api_from_log(vulkan), Some("Vulkan"));
        assert_eq!(feeder_api_from_log("[feed] NVSDK_NGX_D3D12_Init -> Success"), None);
    }

    #[test]
    fn optiscaler_separa_upscaler_e_frame_generation() {
        let args = ["--upscaler", "fsr4", "--framegen", "fsr3"].map(str::to_owned);
        assert!(opti_profile(&args, "NVIDIA GeForce RTX 5060", false).is_err());
        let profile = opti_profile(&args, "AMD Radeon RX 9070 XT", true).unwrap();
        assert!(profile.settings.contains(&("FSR", "UpscalerIndex", "0")));
        assert!(profile.settings.contains(&("FSR", "FGIndex", "1")));
        assert!(profile.settings.contains(&("FrameGen", "FGInput", "dlssg")));
        assert_eq!(profile.files.len(), 3);
    }
}

// ------------------------------------------------------------------ undo

fn undo(args: &[String]) -> i32 {
    let game_arg = match val(args, "--game") {
        Some(g) => g.to_string(),
        None => {
            println!("{} use: forge dlss5 --game \"<pasta do jogo>\" --undo", red("erro:"));
            return 2;
        }
    };
    let game = match resolve_dir(&game_arg) {
        Ok(d) => d,
        Err(e) => {
            println!("{} {e}", red("erro:"));
            return 2;
        }
    };
    // Se o cliente passou a pasta raiz, o nosso registro está junto do .exe.
    let dest = if game.join(MARK).join("install.txt").is_file() {
        game.clone()
    } else if let Some(exe) = find_exe(&game).0 {
        exe.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| game.clone())
    } else {
        game.clone()
    };
    let base = dest.join(MARK);
    let rec_path = base.join("install.txt");
    let txt = match fs::read_to_string(&rec_path) {
        Ok(t) => t,
        Err(_) => {
            println!("{} nenhum registro em {}", red("erro:"), rec_path.display());
            println!("  {}", dim("Sem o registro nao da para saber o que e nosso. Nada foi tocado."));
            return 2;
        }
    };
    let restore_feeder = base.join(FEEDER_CONSUMER_BACKUP);
    let hybrid = restore_feeder.is_file();
    let restore_reshade = base.join(FEEDER_RESHADE_BACKUP);
    if hybrid && dest.join(FEEDER_CONSUMER).exists() {
        eprintln!("O nome do add-on RenoDX já está ocupado. Resolva o conflito antes de restaurar.");
        return 2;
    }
    if hybrid {
        if let Some(exe) = find_exe(&dest).0 {
            if let Err(e) = require_game_closed(&exe) {
                eprintln!("{e}");
                return 2;
            }
        }
        if restore_reshade.is_file() && dest.join("dxgi.dll").exists() {
            eprintln!("O nome dxgi.dll já está ocupado. Resolva o conflito antes de restaurar o ReShade.");
            return 2;
        }
    }

    let mut removidos = 0;
    let mut restaurados = 0;
    let mut mantidos = 0;
    let mut dirs: Vec<String> = Vec::new();

    for line in txt.lines() {
        let p: Vec<&str> = line.split('|').collect();
        if p.len() < 2 {
            continue;
        }
        match p[0] {
            "F" => {
                let rel = p[1];
                let size = p.get(2).copied().unwrap_or("");
                let backup = p.get(3).copied().unwrap_or("");
                let dst = dest.join(rel);
                if !backup.is_empty() {
                    let b = base.join(backup);
                    if b.is_file() {
                        if let Some(parent) = dst.parent() {
                            let _ = fs::create_dir_all(parent);
                        }
                        if fs::copy(&b, &dst).is_ok() {
                            restaurados += 1;
                        }
                        continue;
                    }
                }
                if dst.is_file() {
                    let sz = fs::metadata(&dst).map(|m| m.len()).unwrap_or(0);
                    let optimized_ini = rel.eq_ignore_ascii_case("OptiScaler.ini")
                        && fs::read(base.join(OPTIMIZED_INI)).is_ok_and(|saved| {
                            fs::read(&dst).is_ok_and(|current| current == saved)
                        });
                    if sz.to_string() == size || optimized_ini {
                        if fs::remove_file(&dst).is_ok() {
                            removidos += 1;
                            // candidatas a limpeza: a pasta do arquivo e as
                            // pastas-pai acima dela, ate a raiz do jogo
                            let mut cur = dst.parent().map(|p| p.to_path_buf());
                            while let Some(p) = cur {
                                if p == dest {
                                    break;
                                }
                                match p.strip_prefix(&dest) {
                                    Ok(r) => {
                                        let r = r.display().to_string();
                                        if !r.is_empty() && !dirs.contains(&r) {
                                            dirs.push(r);
                                        }
                                    }
                                    Err(_) => break,
                                }
                                cur = p.parent().map(|x| x.to_path_buf());
                            }
                        }
                    } else {
                        mantidos += 1;
                    }
                }
            }
            "D" => {
                if !dirs.contains(&p[1].to_string()) {
                    dirs.push(p[1].to_string());
                }
            }
            _ => {}
        }
    }

    // Remove so pastas que ficaram VAZIAS (mais fundo primeiro). Se o usuario
    // colocou algo dentro, a pasta fica: nada dele e apagado.
    dirs.sort_by_key(|d| std::cmp::Reverse(d.matches('\\').count()));
    for d in dirs {
        let p = dest.join(&d);
        let _ = fs::remove_dir(&p);
    }

    // remove a preferencia de GPU so se ainda for exatamente a nossa
    if !hybrid {
        if let Some(exe) = find_exe(&game).0 {
        let exe_s = strip_verbatim(&exe);
        if let Some(cur) = crate::win::reg_str("HKCU", r"Software\Microsoft\DirectX\UserGpuPreferences", &exe_s) {
            if cur.trim() == "GpuPreference=2;" {
                crate::win::run("reg.exe", &["delete", REG_GPU, "/v", &exe_s, "/f"]);
            }
        }
        }
    }
    if hybrid {
        if restore_reshade.is_file() {
            if let Err(e) = fs::rename(&restore_reshade, dest.join("dxgi.dll")) {
                eprintln!("Falha ao restaurar o ReShade. O backup foi preservado. {e}");
                return 2;
            }
        }
        if let Err(e) = fs::rename(&restore_feeder, dest.join(FEEDER_CONSUMER)) {
            eprintln!("Falha ao restaurar o RenoDX. O backup foi preservado. {e}");
            return 2;
        }
        println!("  Consumidor RenoDX restaurado para o Feeder.");
    }

    println!();
    println!(
        "  {} {} restaurado(s), {} removido(s), {} mantido(s) por terem sido alterados",
        grn("undo:"),
        restaurados,
        removidos,
        mantidos
    );
    // o registro nao vale mais: uma proxima aplicacao comeca do zero, sem
    // reaproveitar backups desta rodada
    let _ = fs::remove_file(&rec_path);
    let _ = fs::remove_file(base.join(OPTIMIZED_INI));
    let _ = fs::remove_file(base.join("COMO-DESFAZER.txt"));

    // Se nao sobrou backup nenhum, a pasta de trabalho tambem sai: o jogo
    // volta a ficar exatamente como estava.
    let tem_backup = fs::read_dir(&base)
        .map(|r| {
            r.flatten().any(|e| {
                e.file_type().map(|t| t.is_dir()).unwrap_or(false)
                    && e.file_name().to_string_lossy().starts_with("backup-")
            })
        })
        .unwrap_or(false);
    if !tem_backup {
        let _ = fs::remove_dir(&base);
        println!("  {}", dim("nada ficou para tras: a pasta _dlss5forge foi removida"));
    } else {
        println!("  {}", dim(&format!("backups preservados em {}", base.display())));
    }
    println!();
    0
}
