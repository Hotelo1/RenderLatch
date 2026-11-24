// Camada Win32 minima: console, registro, NUMA, afinidade, processos.
// Tudo via FFI direto para nao depender de crates (build offline).

use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};

type Hkey = usize;
const HKCU: Hkey = 0x8000_0001;
const HKLM: Hkey = 0x8000_0002;

const RRF_RT_REG_DWORD: u32 = 0x0000_0010;
const RRF_RT_REG_SZ: u32 = 0x0000_0002;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub const ABOVE_NORMAL_PRIORITY_CLASS: u32 = 0x0000_8000;

#[link(name = "kernel32")]
extern "system" {
    fn GetStdHandle(n: u32) -> isize;
    fn GetConsoleMode(h: isize, m: *mut u32) -> i32;
    fn SetConsoleMode(h: isize, m: u32) -> i32;
    fn SetConsoleOutputCP(cp: u32) -> i32;
    fn GetNumaHighestNodeNumber(n: *mut u32) -> i32;
    fn GetNumaNodeProcessorMask(node: u32, mask: *mut usize) -> i32;
    fn GetCurrentProcess() -> isize;
    fn SetPriorityClass(p: isize, c: u32) -> i32;
    fn SetProcessAffinityMask(p: isize, m: usize) -> i32;
}

#[link(name = "advapi32")]
extern "system" {
    fn RegGetValueW(
        hkey: Hkey,
        subkey: *const u16,
        value: *const u16,
        flags: u32,
        ty: *mut u32,
        data: *mut u8,
        cb: *mut u32,
    ) -> i32;
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub unsafe fn console_utf8() {
    SetConsoleOutputCP(65001);
}

pub unsafe fn enable_vt() {
    let h = GetStdHandle((-11i32) as u32); // STD_OUTPUT_HANDLE
    let mut mode: u32 = 0;
    if GetConsoleMode(h, &mut mode) != 0 {
        SetConsoleMode(h, mode | 0x0004); // ENABLE_VIRTUAL_TERMINAL_PROCESSING
    }
}

fn root(scope: &str) -> Option<Hkey> {
    match scope {
        "HKCU" => Some(HKCU),
        "HKLM" => Some(HKLM),
        _ => None,
    }
}

/// Le um DWORD do registro. Ex.: ("HKLM", r"SYSTEM\...\GraphicsDrivers", "HwSchMode")
pub fn reg_dword(scope: &str, path: &str, name: &str) -> Option<u32> {
    let hk = root(scope)?;
    let p = wide(path);
    let n = wide(name);
    let mut data: u32 = 0;
    let mut cb: u32 = 4;
    let mut ty: u32 = 0;
    let rc = unsafe {
        RegGetValueW(
            hk,
            p.as_ptr(),
            n.as_ptr(),
            RRF_RT_REG_DWORD,
            &mut ty,
            &mut data as *mut u32 as *mut u8,
            &mut cb,
        )
    };
    if rc == 0 {
        Some(data)
    } else {
        None
    }
}

/// Le uma string do registro.
pub fn reg_str(scope: &str, path: &str, name: &str) -> Option<String> {
    let hk = root(scope)?;
    let p = wide(path);
    let n = wide(name);
    let mut buf = vec![0u16; 1024];
    let mut cb: u32 = (buf.len() * 2) as u32;
    let mut ty: u32 = 0;
    let rc = unsafe {
        RegGetValueW(
            hk,
            p.as_ptr(),
            n.as_ptr(),
            RRF_RT_REG_SZ,
            &mut ty,
            buf.as_mut_ptr() as *mut u8,
            &mut cb,
        )
    };
    if rc != 0 {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(0);
    Some(String::from_utf16_lossy(&buf[..len]))
}

pub fn numa_nodes() -> u32 {
    let mut n: u32 = 0;
    let ok = unsafe { GetNumaHighestNodeNumber(&mut n) };
    if ok == 0 {
        1
    } else {
        n + 1
    }
}

/// Mascara de afinidade de um no NUMA (0 = indisponivel).
pub fn numa_mask(node: u32) -> usize {
    let mut m: usize = 0;
    if unsafe { GetNumaNodeProcessorMask(node, &mut m) } != 0 && m != 0 {
        m
    } else {
        0
    }
}

pub fn set_affinity(mask: usize) -> bool {
    if mask == 0 {
        return false;
    }
    unsafe { SetProcessAffinityMask(GetCurrentProcess(), mask) != 0 }
}

pub fn set_priority(class: u32) -> bool {
    unsafe { SetPriorityClass(GetCurrentProcess(), class) != 0 }
}

/// Roda um comando e devolve stdout (janela oculta). None se falhar.
pub fn run(prog: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(prog)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Roda PowerShell de forma nao interativa.
pub fn ps(script: &str) -> Option<String> {
    run(
        "powershell.exe",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ],
    )
}

pub fn build_number() -> u32 {
    reg_str(
        "HKLM",
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
        "CurrentBuildNumber",
    )
    .and_then(|b| b.parse().ok())
    .unwrap_or(0)
}

pub fn hostname() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_else(|_| "PC".into())
}

/// Total e livre de RAM em MB (GlobalMemoryStatusEx via PowerShell evita struct fragil).
pub fn ram_mb() -> Option<(u32, u32)> {
    let out = ps("$o=Get-CimInstance Win32_OperatingSystem; '{0} {1}' -f [int]($o.TotalVisibleMemorySize/1024), [int]($o.FreePhysicalMemory/1024)")?;
    let mut it = out.split_whitespace();
    let total: u32 = it.next()?.parse().ok()?;
    let free: u32 = it.next()?.parse().ok()?;
    Some((total, free))
}

pub fn os_version() -> String {
    let build_s = reg_str(
        "HKLM",
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
        "CurrentBuildNumber",
    )
    .unwrap_or_else(|| "?".into());
    let mut name = reg_str(
        "HKLM",
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
        "ProductName",
    )
    .unwrap_or_else(|| "Windows".into());
    // O registro continua dizendo "Windows 10" no 11.
    if build_s.parse::<u32>().map(|b| b >= 22000).unwrap_or(false) {
        name = name.replace("Windows 10", "Windows 11");
    }
    let disp = reg_str(
        "HKLM",
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
        "DisplayVersion",
    )
    .unwrap_or_else(|| "?".into());
    format!("{name} {disp} (build {build_s})")
}

pub fn active_power_plan() -> (String, String) {
    match win_powercfg() {
        Some(raw) => {
            let guid = raw
                .split_whitespace()
                .find(|t| t.len() == 36 && t.matches('-').count() == 4)
                .map(|g| g.to_lowercase())
                .unwrap_or_else(|| "desconhecido".into());
            let name = friendly_plan_name(&raw, &guid);
            (guid, name)
        }
        None => ("desconhecido".into(), "desconhecido".into()),
    }
}

/// Os nomes de plano sao localizados (e chegam em OEM/CP850), entao casamos
/// por fragmentos que sobrevivem ao encoding.
fn friendly_plan_name(raw: &str, guid: &str) -> String {
    let low = raw.to_lowercase();
    if low.contains("ximo") {
        "Desempenho maximo".into()
    } else if low.contains("alto desempenho") || low.contains("high performance") {
        "Alto desempenho".into()
    } else if low.contains("equilibrado") || low.contains("balanced") {
        "Equilibrado".into()
    } else if low.contains("conomia") || low.contains("saver") {
        "Economia de energia".into()
    } else {
        plan_name(guid).to_string()
    }
}

/// MHz base via CIM - usado quando a CPUID nao expoe o clock e o brand
/// string nao traz "@ x.xxGHz" (caso dos Intel mais novos).
pub fn cpu_mhz_cim() -> Option<u32> {
    let out = ps("[int](Get-CimInstance Win32_Processor).MaxClockSpeed")?;
    out.trim().parse().ok()
}

fn win_powercfg() -> Option<String> {
    run("powercfg.exe", &["/getactivescheme"])
}

/// GUIDs conhecidos de plano de energia.
pub fn plan_name(guid: &str) -> &'static str {
    match guid {
        "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c" => "Alto desempenho",
        "e9a42b02-d5df-448d-aa00-03f14749eb61" => "Desempenho maximo (Ultimate)",
        "381b4222-f694-41f0-9685-ff5bb260df2e" => "Equilibrado",
        "a1841308-3541-4fab-bc81-f71556f20b4a" => "Economia de energia",
        _ => "Personalizado/desconhecido",
    }
}

pub fn set_power_plan(guid: &str) -> bool {
    run("powercfg.exe", &["/setactive", guid]).is_some()
}
