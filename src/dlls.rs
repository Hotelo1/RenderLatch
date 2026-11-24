// Instalacao de DLLs de runtime (runtime/ -> pasta do jogo) com backup e undo.
//
// Regra de seguranca: so substitui arquivo que JA existe no jogo. Adicionar
// nvngx_dlssnr.dll num jogo que nao a carrega nao habilita DLSS 5 - so ocupa
// espaco e confunde o diagnostico.

use crate::ansi::*;
use crate::json::J;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Info {
    pub path: PathBuf,
    pub name: String,
    pub ver: String,
    pub sha: String,
    pub sig: String,
}

fn has(args: &[String], f: &str) -> bool {
    args.iter().any(|a| a == f)
}

fn val<'a>(args: &'a [String], f: &str) -> Option<&'a str> {
    let i = args.iter().position(|a| a == f)?;
    args.get(i + 1).map(|s| s.as_str())
}

const BACKUP_DIR: &str = "_dlss5forge";

/// Consulta versao, SHA-256 e assinatura em um unico spawn do PowerShell.
fn inspect(paths: &[PathBuf]) -> Vec<Info> {
    if paths.is_empty() {
        return Vec::new();
    }
    let list = paths
        .iter()
        .map(|p| format!("'{}'", p.display().to_string().replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(",");
    let script = format!(
        "$paths=@({list}); foreach($p in $paths){{ if(Test-Path -LiteralPath $p){{ \
         $i=Get-Item -LiteralPath $p; \
         $s=Get-AuthenticodeSignature -LiteralPath $p; \
         $v=$i.VersionInfo.FileVersion; if(-not $v){{$v=''}}; \
         '{{0}}|{{1}}|{{2}}|{{3}}' -f $p,$v,((Get-FileHash -LiteralPath $p -Algorithm SHA256).Hash.ToLower()),$s.Status }} }}"
    );
    let out = match crate::win::ps(&script) {
        Some(o) => o,
        None => return Vec::new(),
    };
    out.lines()
        .filter_map(|l| {
            let p: Vec<&str> = l.split('|').collect();
            if p.len() < 4 {
                return None;
            }
            let path = PathBuf::from(p[0]);
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            Some(Info {
                path,
                name,
                ver: p[1].to_string(),
                sha: p[2].to_string(),
                sig: p[3].to_string(),
            })
        })
        .collect()
}

fn runtime_dir() -> Option<PathBuf> {
    if let Some(runtime) = crate::distribution::runtime_dir() {
        return Some(runtime);
    }
    let mut cands = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            cands.push(d.join("runtime"));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        cands.push(cwd.join("runtime"));
    }
    cands.into_iter().find(|p| p.is_dir())
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

/// Procura DLLs de DLSS ate 3 niveis abaixo (jogos costumam ter subpasta de engine).
fn scan(root: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth == 0 || out.len() >= 8 {
        return;
    }
    let rd = match fs::read_dir(root) {
        Ok(r) => r,
        Err(_) => return,
    };
    let mut found_here = false;
    let mut subdirs: Vec<PathBuf> = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            let skip = p
                .file_name()
                .map(|n| {
                    let n = n.to_string_lossy().to_lowercase();
                    n == BACKUP_DIR || n == "runtime" || n == "cache"
                })
                .unwrap_or(true);
            if !skip {
                subdirs.push(p);
            }
        } else if let Some(n) = p.file_name().map(|n| n.to_string_lossy().to_lowercase()) {
            if n.starts_with("nvngx_dlss") && n.ends_with(".dll") {
                found_here = true;
            }
        }
    }
    if found_here {
        out.push(root.to_path_buf());
        return;
    }
    for d in subdirs {
        scan(&d, depth - 1, out);
        if out.len() >= 8 {
            break;
        }
    }
}

fn stamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn newest_backup(game: &Path) -> Option<PathBuf> {
    let base = game.join(BACKUP_DIR);
    let mut dirs: Vec<PathBuf> = fs::read_dir(&base)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    dirs.pop()
}

pub fn install(args: &[String]) -> i32 {
    let game_arg = match val(args, "--game") {
        Some(g) => g.to_string(),
        None => {
            println!("{} use: forge dlls --install --game <pasta ou jogo.exe>", red("erro:"));
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

    // ---------------------------------------------------------------- undo
    if has(args, "--undo") {
        let bk = match newest_backup(&game) {
            Some(b) => b,
            None => {
                println!("{} nenhum backup em {}", red("erro:"), game.join(BACKUP_DIR).display());
                return 2;
            }
        };
        let mut restored = 0;
        let mut removed = 0;
        for e in fs::read_dir(&bk).into_iter().flatten().flatten() {
            let src = e.path();
            if src.extension().map(|x| x.eq_ignore_ascii_case("json")).unwrap_or(false) {
                continue;
            }
            let name = match src.file_name() {
                Some(n) => n.to_os_string(),
                None => continue,
            };
            let dest = game.join(&name);
            if fs::copy(&src, &dest).is_ok() {
                restored += 1;
            }
        }
        if let Ok(manifest) = fs::read_to_string(bk.join("backup.json")) {
            for line in manifest.lines() {
                if line.contains("\"criado\":true") {
                    if let Some(pos) = line.find("\"arquivo\":\"") {
                        let rest = &line[pos + 11..];
                        if let Some(end) = rest.find('"') {
                            let f = game.join(&rest[..end]);
                            if fs::remove_file(&f).is_ok() {
                                removed += 1;
                            }
                        }
                    }
                }
            }
        }
        println!();
        println!("  {} {} restaurado(s), {} removido(s)", grn("undo:"), restored, removed);
        println!("  {}", dim(&format!("backup mantido em {}", bk.display())));
        println!();
        return 0;
    }

    let src_dir = match val(args, "--from").map(PathBuf::from).or_else(runtime_dir) {
        Some(d) => d,
        None => {
            println!("{} pasta runtime/ nao encontrada.", red("erro:"));
            println!(
                "  {}",
                dim("Rode: .\\tools\\fetch-runtime.ps1   (ou use --from <pasta das DLLs>)")
            );
            return 2;
        }
    };

    let streamline = has(args, "--streamline");
    let dry = has(args, "--dry-run");
    let yes = has(args, "--yes");

    // ---------------------------------------------------- inventario destino
    let mut dest_existing: Vec<PathBuf> = fs::read_dir(&game)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .map(|n| {
                    let n = n.to_string_lossy().to_lowercase();
                    n.starts_with("nvngx_dlss") || (streamline && n.starts_with("sl."))
                })
                .unwrap_or(false)
        })
        .filter(|p| p.extension().map(|x| x.eq_ignore_ascii_case("dll")).unwrap_or(false))
        .collect();
    dest_existing.sort();

    crate::report::header(env!("CARGO_PKG_VERSION"));
    println!("  {} {}", bold("origem "), src_dir.display());
    println!("  {} {}", bold("jogo   "), game.display());
    println!();

    if dest_existing.is_empty() {
        println!("  {} este jogo nao tem DLL de DLSS nesta pasta.", yel("nada a fazer:"));
        println!();
        println!(
            "  {}",
            dim("Copiar DLL nao habilita DLSS num jogo que nao a carrega.")
        );
        let mut found = Vec::new();
        scan(&game, 3, &mut found);
        if found.is_empty() {
            println!(
                "  {}",
                dim("Nenhuma nvngx_dlss*.dll foi encontrada ate 3 niveis abaixo.")
            );
            println!(
                "  {}",
                dim("Aponte para a pasta do executavel (ex: ...\\Binaries\\Win64).")
            );
        } else {
            println!("  {}", bold("Pastas com DLL de DLSS encontradas - rode de novo com uma delas:"));
            for f in found {
                println!("    {}", f.display());
            }
        }
        println!();
        return 1;
    }

    // ------------------------------------------------------------- planejar
    let mut plan: Vec<(PathBuf, PathBuf)> = Vec::new(); // (origem, destino)
    let mut faltando: Vec<String> = Vec::new();
    for dest in &dest_existing {
        let name = dest.file_name().unwrap().to_string_lossy().to_string();
        let src = src_dir.join(&name);
        if src.is_file() {
            plan.push((src, dest.clone()));
        } else {
            faltando.push(name);
        }
    }

    // Se o jogo tem o modelo DLSS 5, o plugin tem que ir junto.
    let has_nr = dest_existing
        .iter()
        .any(|p| p.file_name().unwrap().to_string_lossy().eq_ignore_ascii_case("nvngx_dlssnr.dll"));
    if has_nr {
        let plugin = game.join("sl.dlss_nr.dll");
        let srcp = src_dir.join("sl.dlss_nr.dll");
        if plugin.exists() && srcp.is_file() && !plan.iter().any(|(_, d)| *d == plugin) {
            plan.push((srcp, plugin));
        }
    }

    if plan.is_empty() {
        println!("  {} nenhuma DLL correspondente em {}", yel("nada a fazer:"), src_dir.display());
        println!();
        return 1;
    }

    let src_infos = inspect(&plan.iter().map(|(s, _)| s.clone()).collect::<Vec<_>>());
    let dst_infos = inspect(&plan.iter().map(|(_, d)| d.clone()).collect::<Vec<_>>());

    // --------------------------------------------------- validar assinatura
    let bad: Vec<&Info> = src_infos.iter().filter(|i| i.sig != "Valid").collect();
    if !bad.is_empty() && !has(args, "--allow-unsigned") {
        println!("  {} DLL de origem sem assinatura valida:", red("recusado:"));
        for b in &bad {
            println!("    {} ({})", b.name, b.sig);
        }
        println!();
        println!(
            "  {}",
            dim("Use --allow-unsigned apenas se souber exatamente a procedencia.")
        );
        return 2;
    }

    // ------------------------------------------------------------- tabela
    println!(
        "  {} {} {} {} {}",
        pad("arquivo", 24),
        pad("jogo", 14),
        pad("novo", 14),
        pad("MB", 5),
        "acao"
    );
    let mut acoes: Vec<(&str, bool)> = Vec::new();
    for (src, dest) in plan.iter() {
        let si = src_infos.iter().find(|x| x.path == *src);
        let di = dst_infos.iter().find(|x| x.path == *dest);
        let new_ver = si.map(|x| x.ver.clone()).unwrap_or_default();
        let old_ver = di.map(|x| x.ver.clone()).unwrap_or_else(|| "?".into());
        let mb = (fs::metadata(src).map(|m| m.len()).unwrap_or(0) as f64) / (1024.0 * 1024.0);
        let same = match (si, di) {
            (Some(a), Some(b)) => a.sha == b.sha,
            _ => false,
        };
        let acao = if same { "ja igual" } else { "substituir" };
        acoes.push((acao, same));
        println!(
            "  {} {} {} {:>5.1} {}",
            pad(src.file_name().unwrap().to_string_lossy(), 24),
            pad(if old_ver.is_empty() { "?" } else { &old_ver }, 14),
            pad(if new_ver.is_empty() { "?" } else { &new_ver }, 14),
            mb,
            if same { dim(acao) } else { cya(acao) }
        );
    }

    for f in &faltando {
        println!("  {} {} {} {}", pad(f, 24), pad("-", 14), pad("-", 14), dim("nao existe no runtime"));
    }
    println!();

    let doable = acoes.iter().filter(|(_, same)| !*same).count();
    if doable == 0 {
        println!("  {} tudo ja esta na versao do runtime.", grn("ok:"));
        println!();
        return 0;
    }

    println!(
        "  {}",
        yel("Atencao: mexer em DLL pode ser bloqueado por anti-cheat ou verificacao de integridade.")
    );
    println!(
        "  {}",
        dim("O jogo fica offline/banido em casos raros. Backup e automatico; undo existe.")
    );
    println!();

    if dry || !yes {
        if !yes {
            println!("  {} {}", bold("nada foi alterado."), "Confirme com --yes:");
            println!(
                "    {}",
                dim(&format!(
                    "forge dlls --install --game \"{}\"{} --yes",
                    game.display(),
                    if streamline { " --streamline" } else { "" }
                ))
            );
        } else {
            println!("  {}", bold("dry-run: nada foi alterado."));
        }
        println!();
        return 0;
    }

    // ------------------------------------------------------------- backup
    let bk = game.join(BACKUP_DIR).join(format!("backup-{}", stamp()));
    if let Err(e) = fs::create_dir_all(&bk) {
        println!("{} nao foi possivel criar backup: {e}", red("erro:"));
        println!("  {}", dim("Pasta protegida? rode o terminal como administrador."));
        return 2;
    }

    let mut manifest: Vec<String> = Vec::new();
    let mut ok = 0;
    let mut erros = 0;
    for (src, dest) in &plan {
        let name = dest.file_name().unwrap().to_string_lossy().to_string();
        let existed = dest.exists();
        if existed {
            if fs::copy(dest, bk.join(&name)).is_err() {
                println!("  {} backup falhou: {}", red("X"), name);
                erros += 1;
                continue;
            }
        }
        match fs::copy(src, dest) {
            Ok(_) => {
                ok += 1;
                let old_ver = dst_infos.iter().find(|x| x.path == *dest).map(|x| x.ver.clone()).unwrap_or_default();
                let old_sha = dst_infos.iter().find(|x| x.path == *dest).map(|x| x.sha.clone()).unwrap_or_default();
                let new_ver = src_infos.iter().find(|x| x.path == *src).map(|x| x.ver.clone()).unwrap_or_default();
                let new_sha = src_infos.iter().find(|x| x.path == *src).map(|x| x.sha.clone()).unwrap_or_default();
                manifest.push(
                    J::obj()
                        .kstr("arquivo", &name)
                        .kbool("criado", !existed)
                        .kstr("versao_antiga", &old_ver)
                        .kstr("versao_nova", &new_ver)
                        .kstr("sha_antigo", &old_sha)
                        .kstr("sha_novo", &new_sha)
                        .done(),
                );
            }
            Err(e) => {
                println!("  {} copia falhou ({}): {}", red("X"), e, name);
                erros += 1;
            }
        }
    }

    let _ = fs::write(
        bk.join("backup.json"),
        format!("[\n{}\n]\n", manifest.join(",\n")),
    );

    println!(
        "  {} {} arquivo(s) atualizado(s), {} erro(s)",
        if erros == 0 { grn("pronto:") } else { yel("parcial:") },
        ok,
        erros
    );
    println!("  {} {}", dim("backup "), bk.display());
    println!();
    println!("  {}", bold("desfazer:"));
    println!(
        "    {}",
        dim(&format!(
            "forge dlls --install --game \"{}\" --undo",
            game.display()
        ))
    );
    println!();
    if erros == 0 {
        0
    } else {
        1
    }
}
