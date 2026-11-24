// Comandos da CLI.

use crate::ansi::*;
use crate::json::J;
use crate::machine::Machine;
use crate::plan::Plan;
use crate::report;
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const VER: &str = env!("CARGO_PKG_VERSION");

pub fn dispatch(args: &[String]) -> i32 {
    let cmd = args.first().map(|s| s.as_str()).unwrap_or("gui");
    let rest = if args.is_empty() { &args[0..0] } else { &args[1..] };

    // Duplo clique no exe: abre a GUI (que fica aberta).
    if args.is_empty() {
        return gui();
    }

    match cmd {
        "detect" | "d" => detect(rest),
        "plan" | "p" => plan(rest),
        "doctor" | "check" => doctor(rest),
        "apply" => apply(rest),
        "launch" | "run" => launch(rest),
        "dlls" => dlls(rest),
        "dlss5" | "dlss" => crate::dlss5::cmd(rest),
        "gui" => gui(),
        "menu" => menu(),
        "build" => build(rest),
        "selftest" => selftest(),
        "version" | "-V" | "--version" => {
            println!("RenderLatch {VER}");
            0
        }
        "help" | "-h" | "--help" | _ => {
            help();
            if cmd == "help" || cmd.starts_with('-') { 0 } else { 2 }
        }
    }
}

fn has(args: &[String], f: &str) -> bool {
    args.iter().any(|a| a == f)
}

fn val<'a>(args: &'a [String], f: &str) -> Option<&'a str> {
    let i = args.iter().position(|a| a == f)?;
    args.get(i + 1).map(|s| s.as_str())
}

fn localappdata() -> PathBuf {
    std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("DLSS5Forge")
}

fn help() {
    report::header(VER);
    println!("  {}", bold("Compila no PC do cliente. Otimiza para aquele PC."));
    println!();
    println!("  {}", bold("USO"));
    println!("    forge <comando> [opcoes]");
    println!();
    println!("  {}", bold("COMANDOS"));
    println!("    {}  detecta CPU/GPU/link/RAM/Windows e mostra o retrato", pad("detect", 16));
    println!("    {}  veredito DLSS 5 + ajustes para esta maquina", pad("plan", 16));
    println!("    {}  so o que esta errado, com o que fazer", pad("doctor", 16));
    println!("    {}  grava perfil + launcher para um jogo", pad("apply --game X.exe", 16));
    println!("    {}  abre o jogo com energia/afinidade/prioridade", pad("launch --exe X.exe", 16));
    println!("    {}  inventario de DLLs DLSS/Streamline do jogo", pad("dlls --game DIR", 16));
    println!("    {}  aplica o runtime no jogo (backup + --undo)", pad("dlls --install", 16));
    println!("    {}  aplica a DLSS 5 num jogo (backup + --undo)", pad("dlss5 --game DIR", 16));
    println!("    {}  abre a interface grafica", pad("gui", 16));
    println!("    {}  abre o menu interativo do terminal", pad("menu", 16));
    println!("    {}  recompila nativo (opcional: --pgo)", pad("build", 16));
    println!("    {}  mostra com que flags ESTE binario nasceu", pad("selftest", 16));
    println!();
    println!("  {}", bold("FLAGS"));
    println!("    --json       saida legivel por script");
    println!("    --save       grava em %LOCALAPPDATA%\\DLSS5Forge");
    println!("    --undo       desfaz o que 'apply' mexeu no registro");
    println!();
    println!(
        "  {}",
        dim("DLSS 5 e proprietaria da NVIDIA: o forge NAO reimplementa o modelo.")
    );
    println!(
        "  {}",
        dim("Ele compila nativo, valida o que o PC aguenta e remove o overhead em volta.")
    );
    println!();
}

/// Abre a GUI (tools/gui.ps1) do lado do forge.exe.
fn gui() -> i32 {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    if crate::distribution::PORTABLE {
        let result = (|| {
            let script = crate::distribution::prepare_ui()?;
            let exe = std::env::current_exe()?;
            let runtime = crate::distribution::runtime_dir().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "Local application data directory is unavailable")
            })?;
            Command::new(powershell_path())
                .args(["-NoProfile", "-STA", "-ExecutionPolicy", "Bypass", "-File"])
                .arg(&script)
                .arg("-ExecutablePath").arg(exe)
                .arg("-RuntimeRoot").arg(runtime)
                .arg("-Standalone")
                .current_dir(script.parent().unwrap())
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
        })();
        if let Err(error) = result {
            crate::distribution::launch_error(&format!("Could not open RenderLatch.\n\n{error}"));
            return 2;
        }
        return 0;
    }
    let mut cands = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(p) = exe.parent() {
            cands.push(p.join("tools").join("gui.ps1"));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        cands.push(cwd.join("tools").join("gui.ps1"));
    }
    let script = match cands.into_iter().find(|p| p.is_file()) {
        Some(s) => s,
        None => {
            println!("{} tools\\gui.ps1 nao encontrado ao lado do forge.exe", red("erro:"));
            println!("  {}", dim("Use os comandos diretos: forge detect, forge dlss5 --game \"...\""));
            return 2;
        }
    };
    // Libera o executável principal para que a GUI possa recompilá-lo.
    match Command::new("powershell.exe")
        .args(["-NoProfile", "-STA", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
    {
        Ok(_) => 0,
        Err(e) => {
            println!("{} nao consegui abrir o PowerShell/GUI: {e}", red("erro:"));
            2
        }
    }
}

fn powershell_path() -> PathBuf {
    std::env::var_os("SystemRoot")
        .map(|root| PathBuf::from(root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe"))
        .unwrap_or_else(|| PathBuf::from("powershell.exe"))
}

fn menu() -> i32 {
    let mut cands = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(p) = exe.parent() {
            cands.push(p.join("tools").join("menu.ps1"));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        cands.push(cwd.join("tools").join("menu.ps1"));
    }
    let script = match cands.into_iter().find(|p| p.is_file()) {
        Some(s) => s,
        None => {
            println!("{} tools\\menu.ps1 nao encontrado ao lado do forge.exe", red("erro:"));
            println!("  {}", dim("Use os comandos diretos: forge doctor, forge dlss5 --game \"...\""));
            return 2;
        }
    };
    match Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .status()
    {
        Ok(s) => s.code().unwrap_or(0),
        Err(e) => {
            println!("{} nao consegui abrir o PowerShell: {e}", red("erro:"));
            2
        }
    }
}

fn machine_json(m: &Machine) -> String {
    let g = m.gpu.as_ref();
    let gpu_j = match g {
        Some(g) => J::obj()
            .kstr("nome", &g.name)
            .knum("vram_mb", g.vram_mb as f64)
            .kstr("driver", &g.driver)
            .knum("pcie_gen", g.pcie_gen as f64)
            .knum("pcie_lanes", g.pcie_width as f64)
            .knum("pcie_gen_max", g.pcie_gen_max as f64)
            .knum("pcie_lanes_max", g.pcie_width_max as f64)
            .knum("link_gbs", g.bandwidth_gbs() as f64)
            .knum("bar1_mb", g.bar1_mb as f64)
            .kbool("rebar", g.rebar)
            .kbool("smi", g.smi_ok)
            .done(),
        None => "null".to_string(),
    };
    J::obj()
        .knum("schema", 1.0)
        .kstr("host", &m.host)
        .kv(
            "cpu",
            J::obj()
                .kstr("marca", &m.cpu.brand)
                .kstr("fabricante", &m.cpu.vendor)
                .knum("familia", m.cpu.family as f64)
                .knum("modelo", m.cpu.model as f64)
                .knum("stepping", m.cpu.stepping as f64)
                .knum("logicos", m.cpu.logical as f64)
                .knum("fisicos", m.cpu.physical as f64)
                .knum("mhz_base", m.cpu.base_mhz as f64)
                .kstr("isa", &m.cpu.isa.join(","))
                .knum("nos_numa", m.cpu.numa_nodes as f64)
                .kbool("xeon", m.cpu.xeon)
                .end(),
        )
        .kv("gpu", J::raw(gpu_j))
        .kv(
            "memoria",
            J::obj()
                .knum("total_mb", m.mem.total_mb as f64)
                .knum("livre_mb", m.mem.free_mb as f64)
                .end(),
        )
        .kv(
            "windows",
            J::obj()
                .kstr("versao", &m.os.version)
                .kstr("energia", &m.os.power_plan)
                .kbool("hags", m.os.hags.unwrap_or(false))
                .kbool("vbs", m.os.vbs.unwrap_or(false))
                .kbool("hvci", m.os.hvci.unwrap_or(false))
                .kbool("modo_jogo", m.os.game_mode.unwrap_or(false))
                .end(),
        )
        .kv("binario", J::raw(build_info()))
        .done()
}

fn build_info() -> String {
    J::obj()
        .kstr("versao", VER)
        .kstr("flags", env!("FORGE_RUSTFLAGS"))
        .kstr("target", env!("FORGE_TARGET"))
        .kstr("perfil", env!("FORGE_PROFILE"))
        .done()
}

fn detect(args: &[String]) -> i32 {
    let m = Machine::detect();
    if has(args, "--json") {
        println!("{}", machine_json(&m));
        return 0;
    }
    report::header(VER);
    report::machine(&m);
    if has(args, "--save") {
        let dir = localappdata().join("machines");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join(format!("{}.json", m.slug()));
        let _ = fs::write(&path, machine_json(&m) + "\n");
        println!("  {} {}", grn("perfil salvo:"), path.display());
        println!();
    }
    0
}

fn plan(args: &[String]) -> i32 {
    let m = Machine::detect();
    let p = Plan::build(&m);

    if has(args, "--json") {
        println!("{}", p.json(&m));
        return 0;
    }

    report::header(VER);
    report::machine(&m);
    report::plan(&p);

    if has(args, "--save") {
        let dir = localappdata().join("profiles");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join(format!("{}.json", m.slug()));
        let _ = fs::write(&path, p.json(&m) + "\n");
        println!("  {} {}", grn("perfil salvo:"), path.display());
        println!();
    }
    0
}

fn doctor(_args: &[String]) -> i32 {
    let m = Machine::detect();
    let p = Plan::build(&m);
    report::header(VER);

    println!("  {}", bold("CHECKLIST"));
    let mut fails = 0;
    let mut checks: Vec<(bool, String, String)> = Vec::new();

    let g = m.gpu.as_ref();
    checks.push((
        g.as_ref().map(|g| g.is_rtx50()).unwrap_or(false),
        "GPU GeForce RTX 50 Series (exigencia da DLSS 5)".into(),
        g.map(|g| g.name.clone()).unwrap_or_else(|| "nao encontrada".into()),
    ));
    checks.push((
        g.as_ref().map(|g| g.driver_major() >= 580).unwrap_or(false),
        "Driver da linha com suporte a DLSS 5 (580+)".into(),
        g.as_ref().map(|g| g.driver.clone()).unwrap_or_default(),
    ));
    checks.push((
        g.as_ref().map(|g| g.rebar).unwrap_or(false),
        "Resizable BAR ativo".into(),
        g.as_ref()
            .map(|g| format!("BAR1 {} MB", g.bar1_mb))
            .unwrap_or_default(),
    ));
    checks.push((
        g.as_ref().map(|g| g.bandwidth_gbs() >= 8.0).unwrap_or(false),
        "Link PCIe com pelo menos ~8 GB/s".into(),
        g.as_ref()
            .map(|g| format!("Gen{} x{} = {:.1} GB/s", g.pcie_gen, g.pcie_width, g.bandwidth_gbs()))
            .unwrap_or_default(),
    ));
    checks.push((
        m.cpu.has("AVX2") || m.cpu.base_mhz >= 2600,
        "CPU capaz de sustentar o frametime".into(),
        format!("{}C/{}T @ {} MHz", m.cpu.physical, m.cpu.logical, m.cpu.base_mhz),
    ));
    checks.push((
        m.mem.total_mb >= 15_000,
        "16 GB de RAM ou mais".into(),
        format!("{} MB", m.mem.total_mb),
    ));
    checks.push((
        m.os.hags == Some(true),
        "HAGS ligado".into(),
        match m.os.hags {
            Some(true) => "ligado".into(),
            Some(false) => "desligado".into(),
            None => "?".into(),
        },
    ));
    checks.push((
        !(m.os.vbs == Some(true) || m.os.hvci == Some(true)),
        "VBS/HVCI desligados".into(),
        match (m.os.vbs, m.os.hvci) {
            (Some(true), _) | (_, Some(true)) => "ativos".into(),
            _ => "desligados".into(),
        },
    ));

    for (ok, label, value) in &checks {
        if !ok {
            fails += 1;
        }
        let icon = if *ok { grn("[ok]") } else { red("[X ]") };
        println!("  {icon} {}  {}", pad(label, 44), dim(value));
    }
    println!();

    let problems: Vec<_> = p
        .findings
        .iter()
        .filter(|f| matches!(f.level, crate::plan::Level::Warn | crate::plan::Level::Crit))
        .collect();

    if problems.is_empty() {
        println!("  {} {}", grn("Tudo certo."), "Nenhum bloqueio encontrado.");
    } else {
        println!("  {}", bold(&format!("{} PENDENCIA(S)", problems.len())));
        for f in &problems {
            println!("  {} {}", yel("•"), bold(&f.title));
            if let Some(fix) = &f.fix {
                for line in report::wrap(fix, 68) {
                    println!("    {}", dim(&line));
                }
            }
        }
    }
    println!();
    println!("  {}", dim(&format!("veredito DLSS 5: {}", p.tier.label())));
    println!();
    if fails >= 3 {
        1
    } else {
        0
    }
}

fn resolve_exe(path: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(path);
    if p.is_file() {
        return Ok(p);
    }
    if p.is_dir() {
        let mut found: Vec<PathBuf> = fs::read_dir(&p)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|f| f.extension().map(|x| x.eq_ignore_ascii_case("exe")).unwrap_or(false))
            .collect();
        found.sort_by_key(|f| fs::metadata(f).map(|m| m.len()).unwrap_or(0));
        found.reverse();
        if found.len() == 1 {
            return Ok(found.remove(0));
        }
        if found.is_empty() {
            return Err("nenhum .exe nesta pasta".into());
        }
        let list: Vec<String> = found
            .iter()
            .take(6)
            .map(|f| format!("  {}", f.display()))
            .collect();
        return Err(format!("pasta com varios .exe, informe qual:\n{}", list.join("\n")));
    }
    Err(format!("caminho nao existe: {path}"))
}

/// Remove o prefixo verbatim (\\?\) que o canonicalize adiciona no Windows.
fn clean_path(p: PathBuf) -> PathBuf {
    let s = p.display().to_string();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        p
    }
}

fn apply(args: &[String]) -> i32 {
    let game = match val(args, "--game") {
        Some(g) => g.to_string(),
        None => {
            println!("{} use: forge apply --game <jogo.exe>", red("erro:"));
            return 2;
        }
    };
    let exe = match resolve_exe(&game) {
        Ok(p) => clean_path(p.canonicalize().unwrap_or(p)),
        Err(e) => {
            println!("{} {e}", red("erro:"));
            return 2;
        }
    };
    let exe_s = exe.display().to_string();
    let undo = has(args, "--undo");

    let enc_key = r"HKCU\Software\Microsoft\DirectX\UserGpuPreferences";
    let lay_key = r"HKCU\Software\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers";

    if undo {
        crate::win::run("reg.exe", &["delete", enc_key, "/v", &exe_s, "/f"]);
        crate::win::run("reg.exe", &["delete", lay_key, "/v", &exe_s, "/f"]);
        println!("  {} preferencia de GPU e otimizacoes de tela removidas para", grn("[ok]"));
        println!("       {}", exe_s);
        return 0;
    }

    let m = Machine::detect();
    let p = Plan::build(&m);

    // 1. Preferencia de GPU de alto desempenho para este .exe
    crate::win::run(
        "reg.exe",
        &["add", enc_key, "/v", &exe_s, "/t", "REG_SZ", "/d", "GpuPreference=2;", "/f"],
    );
    // 2. Desliga otimizacoes de tela cheia (input lag / alt-tab)
    crate::win::run(
        "reg.exe",
        &[
            "add",
            lay_key,
            "/v",
            &exe_s,
            "/t",
            "REG_SZ",
            "/d",
            "~ DISABLEDXMAXIMIZEDWINDOWEDMODE",
            "/f",
        ],
    );

    // 3. Perfil + launcher
    let dir = localappdata().join("profiles");
    let _ = fs::create_dir_all(&dir);
    let stem = exe
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "jogo".into());
    let profile_path = dir.join(format!("{stem}.json"));
    let _ = fs::write(&profile_path, p.json(&m) + "\n");

    let self_exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("forge.exe"));
    let cmd_path = dir.join(format!("jogar-{stem}.cmd"));
    let cmd_body = format!(
        "@echo off\r\ntitle DLSS5 Forge - {stem}\r\n\"{}\" launch --exe \"{}\" -- %*\r\n",
        self_exe.display(),
        exe_s
    );
    let _ = fs::write(&cmd_path, cmd_body);

    report::header(VER);
    println!("  {}", bold(&format!("Configurado: {stem}")));
    println!("  {} {}", dim("alvo     "), exe_s);
    println!("  {} {}", dim("veredito "), p.tier.label());
    println!("  {} {}", dim("perfil   "), profile_path.display());
    println!("  {} {}", dim("launcher "), cmd_path.display());
    println!();
    println!("  {}", bold("Aplicado no Windows:"));
    println!("    {} GPU de alto desempenho para este .exe", grn("ok"));
    println!("    {} otimizacoes de tela cheia desativadas", grn("ok"));
    println!();
    println!(
        "  {} {}",
        bold("Atencao:"),
        "nada foi injetado no jogo. Anti-cheat nao e afetado."
    );
    println!(
        "  {}",
        dim("Se o jogo tiver verificacao de integridade, nada aqui quebra os arquivos dele.")
    );
    println!();
    0
}

fn launch(args: &[String]) -> i32 {
    let exe = match val(args, "--exe") {
        Some(e) => e.to_string(),
        None => {
            println!("{} use: forge launch --exe <jogo.exe> [-- args do jogo]", red("erro:"));
            return 2;
        }
    };
    let game_args: Vec<String> = match args.iter().position(|a| a == "--") {
        Some(i) => args[i + 1..].to_vec(),
        None => Vec::new(),
    };

    let m = Machine::detect();
    let p = Plan::build(&m);

    // Plano de energia
    let high = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
    let old = m.os.power_plan_guid.clone();
    let old_name = m.os.power_plan.clone();
    let changed = !old.starts_with(high) && crate::win::set_power_plan(high);

    // Afinidade NUMA (herdada pelo processo filho)
    let mut affinity_applied = false;
    if m.cpu.numa_nodes > 1 {
        let mask = crate::win::numa_mask(0);
        affinity_applied = crate::win::set_affinity(mask);
    }
    // Prioridade (herdada pelo filho)
    crate::win::set_priority(crate::win::ABOVE_NORMAL_PRIORITY_CLASS);

    report::header(VER);
    println!("  {} {}", bold("Iniciando:"), exe);
    println!(
        "  {} energia: {}  |  prioridade: Above Normal  |  afinidade: {}",
        dim("setup"),
        if changed { grn("Alto desempenho (temporario)") } else { m.os.power_plan.clone() },
        if affinity_applied { "no NUMA 0".to_string() } else { p.settings.affinity_note.clone() }
    );
    println!();

    let started = Command::new(&exe)
        .args(&game_args)
        .current_dir(
            Path::new(&exe)
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from(".")),
        )
        .spawn();

    let code = match started {
        Ok(mut child) => {
            let st = child.wait();
            st.ok().and_then(|s| s.code()).unwrap_or(0)
        }
        Err(e) => {
            println!("{} nao foi possivel iniciar: {e}", red("erro:"));
            if changed {
                crate::win::set_power_plan(&old);
            }
            return 2;
        }
    };

    if changed {
        crate::win::set_power_plan(&old);
    }
    println!();
    println!(
        "  {} jogo encerrado (codigo {}); energia restaurada para {}",
        grn("fim"),
        code,
        old_name
    );
    println!();
    code
}

fn dlls(args: &[String]) -> i32 {
    if has(args, "--install") {
        return crate::dlls::install(args);
    }
    let target = match val(args, "--game") {
        Some(g) => g.to_string(),
        None => {
            println!("{} use: forge dlls --game <pasta ou jogo.exe>", red("erro:"));
            return 2;
        }
    };
    let dir = {
        let p = PathBuf::from(&target);
        if p.is_dir() {
            p
        } else {
            p.parent().map(|x| x.to_path_buf()).unwrap_or_else(|| PathBuf::from("."))
        }
    };
    let dir_s = dir.display().to_string().replace('\'', "''");
    let script = format!(
        "$d='{dir_s}'; Get-ChildItem -LiteralPath $d -Filter *.dll -ErrorAction SilentlyContinue | \
         Where-Object {{ $_.Name -match 'dlss|^sl\\.' }} | Sort-Object Name | ForEach-Object {{ \
         $v=$_.VersionInfo.FileVersion; if(-not $v){{$v=$_.VersionInfo.ProductVersion}}; \
         $s=Get-AuthenticodeSignature -LiteralPath $_.FullName; \
         $st='nvidia'; if($s.Status -ne 'Valid'){{$st='sem'}} \
         elseif(-not ($s.SignerCertificate.Subject -match 'NVIDIA')){{$st='outro'}}; \
         '{{0}}|{{1}}|{{2}}|{{3}}' -f $_.Name,[int]($_.Length/1024),$v,$st }}"
    );

    report::header(VER);
    println!("  {} {}", bold("DLLs DLSS/Streamline em"), dir.display());
    println!();

    match crate::win::ps(&script) {
        Some(out) if !out.trim().is_empty() => {
            println!(
                "  {} {} {} {}",
                pad("arquivo", 26),
                pad("KB", 7),
                pad("versao", 14),
                "assinatura"
            );
            let mut modelo_dlss5: Option<String> = None;
            for line in out.lines().filter(|l| !l.trim().is_empty()) {
                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() < 4 {
                    continue;
                }
                let v = parts[2].trim();
                let vs = if v.is_empty() {
                    dim("sem versao")
                } else {
                    cya(v)
                };
                let sig = match parts[3].trim() {
                    "nvidia" => grn("NVIDIA"),
                    "outro" => yel("outro emissor"),
                    _ => red("nao assinada"),
                };
                println!(
                    "  {} {} {} {}",
                    pad(parts[0], 26),
                    pad(parts[1], 7),
                    pad(vs, 14),
                    sig
                );
                if parts[0].eq_ignore_ascii_case("nvngx_dlssnr.dll") {
                    modelo_dlss5 = Some(v.to_string());
                }
            }
            println!();
            match modelo_dlss5 {
                Some(v) => {
                    let preset = if v.contains("310.8") { "Preset A" } else { "Preset A + B" };
                    println!("  {} modelo DLSS 5 presente ({} | {})", grn("DLSS 5:"), v, preset);
                }
                None => {
                    println!(
                        "  {} modelo DLSS 5 ausente (nvngx_dlssnr.dll nao esta nesta pasta)",
                        yel("DLSS 5:")
                    );
                    println!(
                        "  {}",
                        dim("So jogos com DLSS 5 trazem esse arquivo. Nao existe em SDK publico.")
                    );
                }
            }
            println!();
            println!(
                "  {}",
                dim("DLL antiga/duplicada e a causa numero 1 de 'DLSS quebrada'. Uma so versao por pasta.")
            );
        }
        _ => {
            println!("  {} nenhuma DLL dlss/sl encontrada nesta pasta.", yel("aviso:"));
            println!(
                "  {}",
                dim("Jogos com DLSS 5 carregam pelo Streamline (sl.dlss_nr.dll + nvngx_dlssnr.dll).")
            );
        }
    }
    println!();
    0
}

fn build(args: &[String]) -> i32 {
    report::header(VER);
    let script = Path::new("build.ps1");
    if !script.exists() {
        println!("  {} build.ps1 nao encontrado no diretorio atual.", red("erro:"));
        println!("  {}", dim("Rode este comando na pasta do projeto."));
        return 2;
    }
    let mut cmd = Command::new("powershell.exe");
    cmd.args([
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        "build.ps1",
    ]);
    if has(args, "--pgo") {
        cmd.arg("-Pgo");
    }
    println!("  {} compilando nativo (target-cpu=native)...", dim("build"));
    println!();
    let st = cmd.status();
    match st {
        Ok(s) if s.success() => {
            println!();
            println!("  {}", grn("build concluido."));
            println!("  {}", dim("Rode 'forge selftest' para ver as flags usadas."));
            0
        }
        Ok(s) => {
            println!();
            println!("  {} build falhou (codigo {:?})", red("erro:"), s.code());
            1
        }
        Err(e) => {
            println!("  {} nao foi possivel executar powershell: {e}", red("erro:"));
            2
        }
    }
}

fn selftest() -> i32 {
    let m = Machine::detect();
    report::header(VER);
    println!("  {}", bold("ESTE BINARIO"));
    println!("  {} {}", dim(pad("versao", 10)), VER);
    println!("  {} {}", dim(pad("target", 10)), env!("FORGE_TARGET"));
    println!("  {} {}", dim(pad("perfil", 10)), env!("FORGE_PROFILE"));
    println!("  {} {}", dim(pad("flags", 10)), env!("FORGE_RUSTFLAGS"));
    if let Ok(p) = std::env::current_exe() {
        let kb = fs::metadata(&p).map(|x| x.len() / 1024).unwrap_or(0);
        println!("  {} {}", dim(pad("tamanho", 10)), format!("{kb} KB"));
        println!("  {} {}", dim(pad("caminho", 10)), p.display());
    }
    println!();
    println!("  {}", bold("ESTA CPU"));
    println!("  {} {}", dim(pad("modelo", 10)), m.cpu.brand);
    println!("  {} {}", dim(pad("ISA", 10)), m.cpu.isa.join(", "));
    println!(
        "  {} {}",
        dim(pad("veredito", 10)),
        if env!("FORGE_RUSTFLAGS").contains("target-cpu=native") {
            grn("compilado para ESTA CPU (nativo)")
        } else {
            yel("build generico - rode 'forge build' para otimizar")
        }
    );
    println!();
    0
}
