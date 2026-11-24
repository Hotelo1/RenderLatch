// O "cerebro": transforma hardware detectado em veredito + ajustes por maquina.

use crate::json::J;
use crate::machine::Machine;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Ok,
    Info,
    Warn,
    Crit,
}

pub struct Finding {
    pub level: Level,
    pub title: String,
    pub detail: String,
    pub fix: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tier {
    Uhd,
    Qhd,
    Fhd,
    Marginal,
    Inviavel,
    Incompativel,
    Desconhecido,
}

impl Tier {
    pub fn label(self) -> &'static str {
        match self {
            Tier::Uhd => "4K (DLSS 5 em qualidade alta)",
            Tier::Qhd => "1440p",
            Tier::Fhd => "1080p",
            Tier::Marginal => "1080p com ressalvas",
            Tier::Inviavel => "inviavel neste hardware",
            Tier::Incompativel => "hardware sem suporte a DLSS 5",
            Tier::Desconhecido => "indeterminado",
        }
    }

    fn rank(self) -> i32 {
        match self {
            Tier::Uhd => 4,
            Tier::Qhd => 3,
            Tier::Fhd => 2,
            Tier::Marginal => 1,
            Tier::Inviavel => 0,
            _ => -1,
        }
    }

    fn from_rank(r: i32) -> Tier {
        match r {
            4 => Tier::Uhd,
            3 => Tier::Qhd,
            2 => Tier::Fhd,
            1 => Tier::Marginal,
            _ => Tier::Inviavel,
        }
    }
}

pub struct Settings {
    pub output: String,
    pub internal: String,
    pub scale: f32,
    pub preset: String,
    pub frame_gen: String,
    pub reflex: String,
    pub vsync: String,
    pub vram_budget_mb: u32,
    pub process_priority: String,
    pub affinity_note: String,
}

pub struct Plan {
    pub tier: Tier,
    pub findings: Vec<Finding>,
    pub settings: Settings,
}

/// Driver minimo conhecido com a linha de suporte a DLSS 5.
const MIN_DRIVER_MAJOR: u32 = 580;

fn scale_for(tier: Tier) -> f32 {
    match tier {
        Tier::Uhd => 0.50,
        Tier::Qhd => 0.58,
        Tier::Fhd => 0.667,
        _ => 0.50,
    }
}

fn preset_for(scale: f32) -> &'static str {
    if scale >= 0.66 {
        "Qualidade"
    } else if scale >= 0.55 {
        "Balanceado"
    } else if scale >= 0.44 {
        "Desempenho"
    } else {
        "Ultra desempenho"
    }
}

impl Plan {
    pub fn build(m: &Machine) -> Plan {
        let mut f: Vec<Finding> = Vec::new();
        let mut tier = Tier::Desconhecido;
        let gpu = m.gpu.as_ref();

        // ---- 1. Suporte ----
        match gpu {
            None => {
                f.push(Finding {
                    level: Level::Crit,
                    title: "Nenhuma GPU NVIDIA encontrada".into(),
                    detail: "A DLSS 5 so roda em GeForce RTX 50 Series.".into(),
                    fix: Some("Confirme o driver NVIDIA instalado e rode 'forge detect' de novo.".into()),
                });
            }
            Some(g) => {
                if !g.is_rtx50() {
                    tier = Tier::Incompativel;
                    f.push(Finding {
                        level: Level::Crit,
                        title: format!("{} nao suporta DLSS 5", g.name),
                        detail: "A DLSS 5 e exclusiva de GeForce RTX 50 Series (Blackwell). \
                                 Em placas anteriores o maximo e DLSS 4.5 (Super Resolucao + Frame Gen)."
                            .into(),
                        fix: Some(
                            "Neste PC, use DLSS 4.5/FSR/XeSS via camada aberta. Nao force o \
                             pipeline DLSS 5: ele nao tem fallback."
                                .into(),
                        ),
                    });
                } else {
                    // ---- 2. VRAM ----
                    // Tolerancia: uma placa "de 8 GB" reporta ~8151 MB.
                    tier = match g.vram_mb {
                        0 => Tier::Desconhecido,
                        v if v >= 15_000 => Tier::Uhd,
                        v if v >= 11_000 => Tier::Qhd,
                        v if v >= 7_500 => Tier::Fhd,
                        v if v >= 5_500 => Tier::Marginal,
                        _ => Tier::Inviavel,
                    };

                    if g.vram_mb > 0 && g.vram_mb < 12 * 1024 {
                        f.push(Finding {
                            level: if g.vram_mb < 7_500 { Level::Crit } else { Level::Warn },
                            title: format!("VRAM curta: {} MB", g.vram_mb),
                            detail: "A DLSS 5 e um estagio neural generativo: consome VRAM \
                                     proporcional a resolucao de saida. Abaixo de 12 GB, 4K nao \
                                     fecha o orcamento de frame."
                                .into(),
                            fix: Some(format!(
                                "Trave a saida em {} e mantenha texturas em Alto (nao Ultra). \
                                 Orcamento seguro: ~{} MB de VRAM so para o jogo.",
                                if g.vram_mb < 8*1024 {"1080p"} else {"1080p/1440p"},
                                (g.vram_mb as f32 * 0.85) as u32
                            )),
                        });
                    }

                    // ---- 3. Link PCIe ----
                    let bw = g.bandwidth_gbs();
                    if g.pcie_gen > 0 && g.pcie_width > 0 {
                        let max_bw = {
                            let per = match g.pcie_gen_max {
                                1 => 0.25,
                                2 => 0.5,
                                3 => 0.985,
                                4 => 1.969,
                                5 => 3.938,
                                _ => 0.0,
                            };
                            per * g.pcie_width_max as f32
                        };
                        if bw < 8.0 {
                            let before = tier;
                            if tier.rank() > 1 {
                                tier = Tier::from_rank(tier.rank() - 1);
                            }
                            f.push(Finding {
                                level: Level::Crit,
                                title: format!(
                                    "Link PCIe estrangulado: Gen{} x{} (~{:.1} GB/s)",
                                    g.pcie_gen, g.pcie_width, bw
                                ),
                                detail: format!(
                                    "A placa suporta Gen{} x{} (~{:.1} GB/s). O estagio neural \
                                     da DLSS 5 troca buffers grandes por frame; com esse link o \
                                     custo sobe e aparecem engasgos. Veredito caiu de {} para {}.",
                                    g.pcie_gen_max, g.pcie_width_max, max_bw,
                                    before.label(), tier.label()
                                ),
                                fix: Some(
                                    "Use o slot x16 ligado direto na CPU (nao o x4 do chipset). \
                                     Confira se a placa esta no slot primario e se a BIOS nao \
                                     esta dividindo lanes com M.2/SATA.".into(),
                                ),
                            });
                        } else if g.pcie_gen < g.pcie_gen_max && g.pcie_gen_max > 0 {
                            f.push(Finding {
                                level: Level::Warn,
                                title: format!(
                                    "Link negociado abaixo do maximo (Gen{} x{} de Gen{} x{})",
                                    g.pcie_gen, g.pcie_width, g.pcie_gen_max, g.pcie_width_max
                                ),
                                detail: "Sinal de slot/placa-mae limitando a GPU.".into(),
                                fix: Some("Verifique slot primario, riser e configuracao de lanes na BIOS.".into()),
                            });
                        }
                    }

                    // ---- 4. Resizable BAR ----
                    if g.smi_ok {
                        if g.rebar {
                            f.push(Finding {
                                level: Level::Ok,
                                title: format!("Resizable BAR ativo (BAR1 {} MB)", g.bar1_mb),
                                detail: "Bom: reduz round-trips de memoria no pipeline neural.".into(),
                                fix: None,
                            });
                        } else {
                            f.push(Finding {
                                level: Level::Crit,
                                title: format!("Resizable BAR desligado (BAR1 {} MB)", g.bar1_mb),
                                detail: "Em RTX 50 isso custa varios % e aumenta stutter, \
                                         principalmente perto do limite de VRAM."
                                    .into(),
                                fix: Some(
                                    "1) BIOS: habilite 'Above 4G Decoding' e 'Re-Size BAR Support'. \
                                     2) Em placas antigas (Xeon/Chipset antigo) use ReBarUEFI + \
                                     UEFIPatch. 3) Confirme com 'forge doctor' (BAR1 deve ser ~= VRAM)."
                                        .into(),
                                ),
                            });
                        }
                    }

                    // ---- 5. Driver ----
                    if g.driver_major() > 0 && g.driver_major() < MIN_DRIVER_MAJOR {
                        f.push(Finding {
                            level: Level::Warn,
                            title: format!("Driver {} antigo para DLSS 5", g.driver),
                            detail: format!("A linha {} e a base de suporte a DLSS 5.", MIN_DRIVER_MAJOR),
                            fix: Some("Atualize o driver pelo site da NVIDIA (instalacao limpa).".into()),
                        });
                    }
                }
            }
        }

        // ---- 6. CPU ----
        if !m.cpu.has("AVX2") {
            f.push(Finding {
                level: Level::Info,
                title: "CPU sem AVX2".into(),
                detail: format!(
                    "ISA detectada: {}. O forge e recompilado so com o que esta CPU tem; \
                     builds 'genericos' com AVX2 quebrariam aqui.",
                    m.cpu.isa.join(", ")
                ),
                fix: Some("Rode 'build.ps1' nesta maquina (nunca copie o .exe de outro PC).".into()),
            });
        }

        if m.cpu.base_mhz > 0 && m.cpu.base_mhz < 2400 && !m.cpu.hybrid {
            f.push(Finding {
                level: Level::Warn,
                title: format!("CPU de {} MHz - risco de gargalo", m.cpu.base_mhz),
                detail: "A DLSS 5 aumenta o trabalho de GPU, mas o jogo continua limitado por \
                         thread principal. Clock baixo = frametime pior mesmo com GPU sobrando."
                    .into(),
                fix: Some(
                    "Trave o jogo em 1 thread principal por CCD/no NUMA, feche overlays e \
                     deixe o plano de energia em Alto desempenho."
                        .into(),
                ),
            });
        }

        if m.cpu.physical > 0 && m.cpu.physical <= 4 {
            f.push(Finding {
                level: Level::Warn,
                title: format!("Apenas {} nucleos fisicos", m.cpu.physical),
                detail: "Jogos modernos assumem 6-8 nucleos.".into(),
                fix: Some("Reduza sombras/geometria e evite Frame Gen (ela consome CPU extra).".into()),
            });
        }

        if m.cpu.xeon {
            f.push(Finding {
                level: Level::Info,
                title: "Plataforma Xeon (servidor)".into(),
                detail: "Turbo baixo, latencia de memoria alta e, se for dual-socket, dois nos NUMA. \
                         Isso pesa no frametime mais do que o numero de nucleos sugere."
                    .into(),
                fix: Some(if m.cpu.numa_nodes > 1 {
                    "Como o jogo roda no no NUMA 0: 'forge launch' aplica essa afinidade automaticamente."
                        .into()
                } else {
                    "Sem segundo socket: foque em RAM em dual/quad channel e XMP/velocidade.".into()
                }),
            });
        }

        // ---- 7. RAM ----
        if m.mem.total_mb > 0 && m.mem.total_mb < 12_000 {
            let gb = (m.mem.total_mb as f32 / 1024.0).round() as u32;
            f.push(Finding {
                level: Level::Warn,
                title: format!("Apenas ~{gb} GB de RAM"),
                detail: "Abaixo de 12 GB o Windows paga pagefile no meio do jogo e o 1% low \
                         despenca, justamente onde a DLSS 5 precisa de frametime estavel."
                    .into(),
                fix: Some("16 GB e o minimo pratico; 32 GB remove o problema.".into()),
            });
        } else if m.mem.total_mb < 30_000 {
            f.push(Finding {
                level: Level::Info,
                title: format!("~{} GB de RAM", (m.mem.total_mb as f32 / 1024.0).round() as u32),
                detail: "Suficiente, mas feche navegador/overlay: o estagio neural da DLSS 5 \
                         compete por memoria com o resto do sistema."
                    .into(),
                fix: None,
            });
        }

        // ---- 8. Windows ----
        if m.os.hags == Some(false) {
            f.push(Finding {
                level: Level::Warn,
                title: "Agendamento de GPU acelerado por hardware (HAGS) desligado".into(),
                detail: "Frame Gen e o pipeline neural se beneficiam do HAGS.".into(),
                fix: Some("Configuracoes > Sistema > Video > Configuracoes de graficos > \
                           'Agendamento de GPU acelerado por hardware' = Ativado (reinicia)."
                    .into()),
            });
        }

        if m.os.vbs == Some(true) || m.os.hvci == Some(true) {
            f.push(Finding {
                level: Level::Warn,
                title: "VBS/HVCI ativos".into(),
                detail: "Virtualizacao + integridade de codigo custam de 3% a 12% em jogos, \
                         sobretudo com CPU antiga."
                    .into(),
                fix: Some("Seguranca do Windows > Isolamento do nucleo > desative 'Integridade de \
                           memoria'. Depois: 'bcdedit /set hypervisorlaunchtype off' (se nao usar \
                           WSL/Hyper-V)."
                    .into()),
            });
        }

        if m.os.game_mode == Some(false) {
            f.push(Finding {
                level: Level::Info,
                title: "Modo de jogo desligado".into(),
                detail: "Reduz preempcao de overlays e servicos durante o jogo.".into(),
                fix: Some("Configuracoes > Jogos > Modo de jogo = Ativado.".into()),
            });
        }

        let plano_ok = m.os.power_plan.to_lowercase().contains("desempenho")
            && !m.os.power_plan.to_lowercase().contains("equilibrado");
        if !plano_ok {
            f.push(Finding {
                level: Level::Info,
                title: format!("Plano de energia: {}", m.os.power_plan),
                detail: "Em Equilibrado o Windows pode baixar clock no meio do frame.".into(),
                fix: Some("'forge launch' troca para Alto desempenho e restaura ao sair.".into()),
            });
        }

        // ---- Settings ----
        let (ow, oh) = match gpu.and_then(|g| g.res) {
            Some((w, h, _)) => (w, h),
            None => (1920, 1080),
        };
        let scale = scale_for(tier);
        let iw = ((ow as f32 * scale) as u32 / 2) * 2;
        let ih = ((oh as f32 * scale) as u32 / 2) * 2;

        let frame_gen = match tier {
            Tier::Uhd | Tier::Qhd => "2x (4x so com base acima de 60 fps)".to_string(),
            Tier::Fhd => "2x se a base passar de 45 fps; senao desligado".to_string(),
            _ => "desligado ate validar o frametime".to_string(),
        };

        let vram = gpu.map(|g| g.vram_mb).unwrap_or(0);
        let budget = if vram > 0 { (vram as f32 * 0.85) as u32 } else { 0 };

        let affinity_note = if m.cpu.numa_nodes > 1 {
            format!("fixar no no NUMA 0 ({} nucleos logicos)", m.cpu.logical / m.cpu.numa_nodes.max(1))
        } else {
            "todos os nucleos".to_string()
        };

        Plan {
            tier,
            findings: f,
            settings: Settings {
                output: format!("{ow}x{oh}"),
                internal: format!("{iw}x{ih}"),
                scale,
                preset: preset_for(scale).to_string(),
                frame_gen,
                reflex: "On + Boost".into(),
                vsync: "Off (use G-Sync/VRR + limite de fps 3 abaixo do refresh)".into(),
                vram_budget_mb: budget,
                process_priority: "Above Normal".into(),
                affinity_note,
            },
        }
    }

    pub fn json(&self, m: &Machine) -> String {
        let findings = self
            .findings
            .iter()
            .fold(J::arr(), |a, f| {
                let obj = J::obj()
                    .kstr("nivel", match f.level {
                        Level::Ok => "ok",
                        Level::Info => "info",
                        Level::Warn => "aviso",
                        Level::Crit => "critico",
                    })
                    .kstr("titulo", &f.title)
                    .kstr("detalhe", &f.detail)
                    .kstr("correcao", f.fix.as_deref().unwrap_or(""))
                    .done();
                a.push(J::raw(obj))
            })
            .done();

        let s = &self.settings;
        J::obj()
            .knum("schema", 1.0)
            .kstr("host", &m.host)
            .kstr("cpu", &m.cpu.brand)
            .kstr("cpu_isa", &m.cpu.isa.join(","))
            .kstr("gpu", &m.gpu_name())
            .knum("vram_mb", m.gpu.as_ref().map(|g| g.vram_mb).unwrap_or(0) as f64)
            .kstr("driver", &m.gpu.as_ref().map(|g| g.driver.clone()).unwrap_or_default())
            .knum(
                "pcie_gbs",
                m.gpu.as_ref().map(|g| g.bandwidth_gbs()).unwrap_or(0.0) as f64,
            )
            .kbool("rebar", m.gpu.as_ref().map(|g| g.rebar).unwrap_or(false))
            .knum("ram_mb", m.mem.total_mb as f64)
            .kstr("veredito", self.tier.label())
            .kv(
                "ajustes",
                J::obj()
                    .kstr("saida", &s.output)
                    .kstr("interna", &s.internal)
                    .knum("escala", s.scale as f64)
                    .kstr("preset", &s.preset)
                    .kstr("frame_gen", &s.frame_gen)
                    .kstr("reflex", &s.reflex)
                    .kstr("vsync", &s.vsync)
                    .knum("orcamento_vram_mb", s.vram_budget_mb as f64)
                    .kstr("prioridade", &s.process_priority)
                    .kstr("afinidade", &s.affinity_note)
                    .end(),
            )
            .kv("achados", J::raw(findings))
            .done()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::Cpu;
    use crate::gpu::Gpu;
    use crate::machine::{Machine, Mem, Windows};

    fn cpu(brand: &str, phys: u32, log: u32, mhz: u32, isa: &[&str], xeon: bool, numa: u32) -> Cpu {
        Cpu {
            brand: brand.into(),
            vendor: "GenuineIntel".into(),
            family: 6,
            model: 0,
            stepping: 0,
            logical: log,
            physical: phys,
            base_mhz: mhz,
            isa: isa.iter().map(|s| s.to_string()).collect(),
            numa_nodes: numa,
            hybrid: false,
            xeon,
        }
    }

    fn gpu(name: &str, vram: u32, gen: u32, width: u32, rebar: bool) -> Gpu {
        Gpu {
            name: name.into(),
            vram_mb: vram,
            driver: "621.24".into(),
            pcie_gen: gen,
            pcie_width: width,
            pcie_gen_max: 5,
            pcie_width_max: 8,
            bar1_mb: if rebar { vram } else { 256 },
            rebar,
            res: Some((1920, 1080, 144)),
            smi_ok: true,
        }
    }

    fn mach(c: Cpu, g: Gpu, ram: u32) -> Machine {
        Machine {
            host: "TESTE".into(),
            cpu: c,
            gpu: Some(g),
            mem: Mem {
                total_mb: ram,
                free_mb: ram / 2,
            },
            os: Windows {
                version: "Windows 11 Pro 26H2 (build 26300)".into(),
                hags: Some(false),
                vbs: Some(true),
                hvci: Some(false),
                game_mode: Some(true),
                power_plan_guid: "381b4222-f694-41f0-9685-ff5bb260df2e".into(),
                power_plan: "Equilibrado".into(),
            },
        }
    }

    fn has_title(p: &Plan, needle: &str) -> bool {
        p.findings.iter().any(|f| f.title.contains(needle))
    }

    #[test]
    fn xeon_2650_com_rtx_5050_cai_para_marginal() {
        // Cliente do caso real: Xeon E5-2650 v1, RTX 5050 8 GB, PCIe 3.0 x8.
        let c = cpu("Intel(R) Xeon(R) CPU E5-2650 0 @ 2.00GHz", 8, 16, 2000,
                    &["SSE2", "SSE4.2", "AVX"], true, 1);
        let g = gpu("NVIDIA GeForce RTX 5050", 8151, 3, 8, false);
        let p = Plan::build(&mach(c, g, 32768));

        assert_eq!(p.tier, Tier::Marginal, "8 GB + PCIe 3.0 x8 nao pode dar 1080p cheio");
        assert!(has_title(&p, "PCIe"), "precisa acusar o link estrangulado");
        assert!(has_title(&p, "Resizable BAR"), "precisa acusar ReBAR off");
        assert!(has_title(&p, "AVX2"), "precisa avisar sobre ISA antiga");
        assert!(has_title(&p, "Xeon"), "precisa avisar sobre plataforma de servidor");
        assert!(p.settings.vram_budget_mb > 6000 && p.settings.vram_budget_mb < 7200);
    }

    #[test]
    fn rtx_40_nao_suporta_dlss5() {
        let c = cpu("Intel(R) Core(TM) i7-12700K", 12, 20, 3600,
                    &["SSE2", "AVX2"], false, 1);
        let g = gpu("NVIDIA GeForce RTX 4070", 12282, 4, 16, true);
        let p = Plan::build(&mach(c, g, 32768));
        assert_eq!(p.tier, Tier::Incompativel);
        assert!(has_title(&p, "nao suporta DLSS 5"));
    }

    #[test]
    fn rtx_5080_vai_a_4k() {
        let c = cpu("AMD Ryzen 7 9800X3D 8-Core Processor", 8, 16, 4700,
                    &["SSE2", "AVX2", "AVX512F"], false, 1);
        let g = gpu("NVIDIA GeForce RTX 5080", 16303, 5, 16, true);
        let p = Plan::build(&mach(c, g, 65536));
        assert_eq!(p.tier, Tier::Uhd);
        assert_eq!(p.settings.internal, "960x540");
    }

    #[test]
    fn pcie_3_x4_e_inviavel() {
        let c = cpu("Intel(R) Xeon(R) CPU E5-2650 0 @ 2.00GHz", 8, 16, 2000,
                    &["SSE2", "AVX"], true, 1);
        let g = gpu("NVIDIA GeForce RTX 5050", 8151, 3, 4, false);
        let p = Plan::build(&mach(c, g, 16384));
        assert_eq!(p.tier, Tier::Marginal);
    }
}
