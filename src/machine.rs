// Junta tudo o que o PC "e" e o que o Windows esta fazendo contra o desempenho.

use crate::cpu::{self, Cpu};
use crate::gpu::{self, Gpu};

pub struct Windows {
    pub version: String,
    pub hags: Option<bool>,
    pub vbs: Option<bool>,
    pub hvci: Option<bool>,
    pub game_mode: Option<bool>,
    pub power_plan_guid: String,
    pub power_plan: String,
}

pub struct Mem {
    pub total_mb: u32,
    pub free_mb: u32,
}

pub struct Machine {
    pub host: String,
    pub cpu: Cpu,
    pub gpu: Option<Gpu>,
    pub mem: Mem,
    pub os: Windows,
}

fn dword_bool(v: Option<u32>) -> Option<bool> {
    v.map(|x| x != 0)
}

impl Machine {
    pub fn detect() -> Machine {
        let nodes = crate::win::numa_nodes();
        let mut cpu = cpu::detect(nodes);
        if cpu.base_mhz == 0 {
            if let Some(mhz) = crate::win::cpu_mhz_cim() {
                cpu.base_mhz = mhz;
            }
        }
        let gpu = gpu::detect_nvidia().or_else(gpu::detect_fallback);

        let (total_mb, free_mb) = crate::win::ram_mb().unwrap_or((0, 0));

        // HwSchMode: 2 = ligado, 1 = desligado. Ausente = padrao do Windows,
        // que e LIGADO a partir do build 22621.
        let hags = match crate::win::reg_dword(
            "HKLM",
            r"SYSTEM\CurrentControlSet\Control\GraphicsDrivers",
            "HwSchMode",
        ) {
            Some(v) => Some(v == 2),
            None if crate::win::build_number() >= 22621 => Some(true),
            None => None,
        };

        let vbs = dword_bool(crate::win::reg_dword(
            "HKLM",
            r"SYSTEM\CurrentControlSet\Control\DeviceGuard",
            "EnableVirtualizationBasedSecurity",
        ));

        let hvci = dword_bool(crate::win::reg_dword(
            "HKLM",
            r"SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\HypervisorEnforcedCodeIntegrity",
            "Enabled",
        ));

        let game_mode = dword_bool(crate::win::reg_dword(
            "HKCU",
            r"Software\Microsoft\GameBar",
            "AutoGameModeEnabled",
        ));

        let (guid, power_plan) = crate::win::active_power_plan();

        Machine {
            host: crate::win::hostname(),
            cpu,
            gpu,
            mem: Mem { total_mb, free_mb },
            os: Windows {
                version: crate::win::os_version(),
                hags,
                vbs,
                hvci,
                game_mode,
                power_plan_guid: guid,
                power_plan,
            },
        }
    }

    pub fn gpu_name(&self) -> String {
        self.gpu
            .as_ref()
            .map(|g| g.name.clone())
            .unwrap_or_else(|| "nenhuma GPU NVIDIA".into())
    }

    /// Impression digital curta para nomear arquivos de perfil.
    pub fn slug(&self) -> String {
        let g = self
            .gpu
            .as_ref()
            .map(|g| g.name.replace("NVIDIA GeForce ", ""))
            .unwrap_or_else(|| "nogpu".into());
        let c = self
            .cpu
            .brand
            .replace("Intel(R) ", "")
            .replace("Core(TM) ", "")
            .replace("CPU ", "");
        let raw = format!("{c}__{g}");
        let mut out = String::new();
        for ch in raw.chars() {
            if ch.is_ascii_alphanumeric() {
                out.push(ch.to_ascii_lowercase());
            } else if !out.ends_with('-') {
                out.push('-');
            }
        }
        out.trim_matches('-').to_string()
    }
}
