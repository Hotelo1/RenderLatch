// Deteccao de GPU via nvidia-smi (fonte de verdade do driver) + CIM como fallback.

#[derive(Clone)]
pub struct Gpu {
    pub name: String,
    pub vram_mb: u32,
    pub driver: String,
    pub pcie_gen: u32,
    pub pcie_width: u32,
    pub pcie_gen_max: u32,
    pub pcie_width_max: u32,
    pub bar1_mb: u32,
    pub rebar: bool,
    pub res: Option<(u32, u32, u32)>, // largura, altura, Hz
    pub smi_ok: bool,
}

const QUERY: &str = "name,memory.total,driver_version,pcie.link.gen.current,pcie.link.width.current,pcie.link.gen.max,pcie.link.width.max";

fn num(s: &str) -> Option<u32> {
    let t: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    t.parse().ok()
}

fn bar1_mb() -> Option<u32> {
    let out = crate::win::run("nvidia-smi.exe", &["-q", "-d", "MEMORY"])?;
    let mut in_bar1 = false;
    for line in out.lines() {
        if line.contains("BAR1") {
            in_bar1 = true;
            continue;
        }
        if in_bar1 {
            if let Some(v) = line.split(':').nth(1) {
                let t = v.trim();
                if t.starts_with(|c: char| c.is_ascii_digit()) {
                    return num(t);
                }
            }
            if line.trim().is_empty() {
                in_bar1 = false;
            }
        }
    }
    None
}

fn resolution() -> Option<(u32, u32, u32)> {
    let script = "$v=Get-CimInstance Win32_VideoController | Where-Object {$_.Name -like '*NVIDIA*'} | Select-Object -First 1; if($v){'{0} {1} {2}' -f $v.CurrentHorizontalResolution,$v.CurrentVerticalResolution,$v.CurrentRefreshRate}";
    let out = crate::win::ps(script)?;
    let mut it = out.split_whitespace();
    let w: u32 = it.next()?.parse().ok()?;
    let h: u32 = it.next()?.parse().ok()?;
    let r: u32 = it.next().and_then(|v| v.parse().ok()).unwrap_or(60);
    if w == 0 || h == 0 {
        None
    } else {
        Some((w, h, r))
    }
}

pub fn detect_nvidia() -> Option<Gpu> {
    let out = crate::win::run(
        "nvidia-smi.exe",
        &["--query-gpu", QUERY, "--format=csv,noheader,nounits"],
    )?;
    let first = out.lines().find(|l| !l.trim().is_empty())?;
    let parts: Vec<&str> = first.split(',').map(|p| p.trim()).collect();
    if parts.len() < 7 {
        return None;
    }
    let vram = num(parts[1]).unwrap_or(0);
    let bar1 = bar1_mb().unwrap_or(0);
    // Com Resizable BAR ligado, BAR1 expoe quase toda a VRAM.
    let rebar = bar1 > 512 && (bar1 as f32) > (vram as f32) * 0.5;

    Some(Gpu {
        name: parts[0].to_string(),
        vram_mb: vram,
        driver: parts[2].to_string(),
        pcie_gen: num(parts[3]).unwrap_or(0),
        pcie_width: num(parts[4]).unwrap_or(0),
        pcie_gen_max: num(parts[5]).unwrap_or(0),
        pcie_width_max: num(parts[6]).unwrap_or(0),
        bar1_mb: bar1,
        rebar,
        res: resolution(),
        smi_ok: true,
    })
}

/// Fallback: nome pelo CIM, sem dados de PCIe.
pub fn detect_fallback() -> Option<Gpu> {
    let script = "$v=Get-CimInstance Win32_VideoController | Where-Object {$_.Name -like '*NVIDIA*'} | Select-Object -First 1; if($v){$v.Name}";
    let name = crate::win::ps(script)?.trim().to_string();
    if name.is_empty() {
        return None;
    }
    Some(Gpu {
        name,
        vram_mb: 0,
        driver: "?".into(),
        pcie_gen: 0,
        pcie_width: 0,
        pcie_gen_max: 0,
        pcie_width_max: 0,
        bar1_mb: 0,
        rebar: false,
        res: resolution(),
        smi_ok: false,
    })
}

impl Gpu {
    /// Largura de banda efetiva do link (GB/s, teorica por direcao).
    pub fn bandwidth_gbs(&self) -> f32 {
        let per_lane = match self.pcie_gen {
            1 => 0.25,
            2 => 0.5,
            3 => 0.985,
            4 => 1.969,
            5 => 3.938,
            _ => 0.0,
        };
        per_lane * self.pcie_width as f32
    }

    pub fn is_rtx50(&self) -> bool {
        self.name.contains("RTX 50")
    }

    pub fn res_label(&self) -> String {
        match self.res {
            Some((w, h, r)) => format!("{w}x{h} @ {r} Hz"),
            None => "desconhecida".into(),
        }
    }

    pub fn driver_major(&self) -> u32 {
        self.driver
            .split('.')
            .next()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    }
}
