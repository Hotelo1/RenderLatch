// Impressao legivel no console.

use crate::ansi::*;
use crate::machine::Machine;
use crate::plan::{Level, Plan};

pub fn header(ver: &str) {
    let line = "─".repeat(58);
    println!();
    println!("  {}  {}", bold(&inv(" RENDERLATCH ")), dim(&format!("v{ver}")));
    println!("  {}", dim(&line));
}

pub fn kv(label: &str, value: &str) {
    println!("  {} {}", dim(&pad(label, 8)), value);
}

pub fn machine(m: &Machine) {
    let c = &m.cpu;
    let cp = format!(
        "{}  {}C/{}T @ {} MHz  {}",
        bold(&c.brand),
        c.physical,
        c.logical,
        c.base_mhz,
        dim(&format!("[{}]", c.isa.join(" ")))
    );
    kv("Host", &m.host);
    kv("CPU", &cp);

    match &m.gpu {
        Some(g) => {
            let link = if g.pcie_gen > 0 {
                format!(
                    "PCIe Gen{} x{}  {:.1} GB/s{}",
                    g.pcie_gen,
                    g.pcie_width,
                    g.bandwidth_gbs(),
                    if g.pcie_gen_max > g.pcie_gen {
                        format!("  {} (slot aceita Gen{})", yel("subutilizado"), g.pcie_gen_max)
                    } else {
                        String::new()
                    }
                )
            } else {
                "link nao reportado".into()
            };
            kv("GPU", &format!("{}  {} MB  driver {}", bold(&g.name), g.vram_mb, g.driver));
            kv("Link", &link);
            kv(
                "ReBAR",
                &match (g.smi_ok, g.rebar) {
                    (false, _) => dim("nao verificado").to_string(),
                    (true, true) => grn(&format!("ativo (BAR1 {} MB)", g.bar1_mb)),
                    (true, false) => red(&format!("desligado (BAR1 {} MB)", g.bar1_mb)),
                },
            );
            kv("Tela", &g.res_label());
        }
        None => kv("GPU", &red("nenhuma GPU NVIDIA detectada")),
    }

    kv(
        "RAM",
        &format!("{} MB  (livre {} MB)", m.mem.total_mb, m.mem.free_mb),
    );
    kv("Windows", &m.os.version);
    let flag = |v: Option<bool>, on: &str, off: &str| -> String {
        match v {
            Some(true) => grn(on),
            Some(false) => yel(off),
            None => dim("?").to_string(),
        }
    };
    kv(
        "Estado",
        &format!(
            "energia: {}  |  HAGS: {}  |  VBS/HVCI: {}  |  modo jogo: {}",
            m.os.power_plan,
            flag(m.os.hags, "ligado", "desligado"),
            flag(
                Some(m.os.vbs == Some(true) || m.os.hvci == Some(true)),
                "ligado",
                "desligado"
            ),
            flag(m.os.game_mode, "ligado", "desligado")
        ),
    );
    println!();
}

pub fn plan(p: &Plan) {
    println!("  {} {}", bold("VEREDITO:"), verdict(p));
    println!();

    for f in &p.findings {
        let icon = match f.level {
            Level::Ok => grn("[ok]"),
            Level::Info => cya("[i ]"),
            Level::Warn => yel("[! ]"),
            Level::Crit => red("[X ]"),
        };
        println!("  {icon} {}", bold(&f.title));
        for line in wrap(&f.detail, 72) {
            println!("       {}", dim(&line));
        }
        if let Some(fix) = &f.fix {
            for (i, line) in wrap(fix, 70).into_iter().enumerate() {
                if i == 0 {
                    println!("       {} {}", grn("→"), line);
                } else {
                    println!("         {line}");
                }
            }
        }
        println!();
    }

    settings(p);
}

fn verdict(p: &Plan) -> String {
    let label = p.tier.label();
    match p.tier {
        crate::plan::Tier::Uhd | crate::plan::Tier::Qhd => grn(&bold(label)),
        crate::plan::Tier::Fhd => yel(&bold(label)),
        crate::plan::Tier::Marginal => yel(&bold(label)),
        _ => red(&bold(label)),
    }
}

fn settings(p: &Plan) {
    let s = &p.settings;
    let line = "─".repeat(58);
    println!("  {}", dim(&line));
    println!("  {}", bold("AJUSTES PARA ESTA MAQUINA"));
    println!("  {}", dim(&line));
    let rows = [
        ("Saida", s.output.clone()),
        ("Interna", format!("{}  ({})", s.internal, s.preset)),
        ("Frame Gen", s.frame_gen.clone()),
        ("Reflex", s.reflex.clone()),
        ("VSync", s.vsync.clone()),
        ("VRAM", if s.vram_budget_mb > 0 { format!("orcamento ~{} MB", s.vram_budget_mb) } else { "n/d".into() }),
        ("Processo", format!("prioridade {}  |  {}", s.process_priority, s.affinity_note)),
    ];
    for (k, v) in rows {
        println!("  {} {}", dim(&pad(k, 9)), v);
    }
    println!("  {}", dim(&line));
    println!(
        "  {}",
        dim("Gerado a partir do hardware real. Rode 'forge plan --save' para gravar o perfil.")
    );
    println!();
}

pub fn wrap(s: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        if cur.is_empty() {
            cur.push_str(word);
        } else if cur.chars().count() + 1 + word.chars().count() <= width {
            cur.push(' ');
            cur.push_str(word);
        } else {
            out.push(std::mem::take(&mut cur));
            cur.push_str(word);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}
