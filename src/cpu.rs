// Deteccao de CPU via CPUID puro (sem WMI, sem spawn de processo).

use core::arch::x86_64::__cpuid_count;

pub struct Cpu {
    pub brand: String,
    pub vendor: String,
    pub family: u32,
    pub model: u32,
    pub stepping: u32,
    pub logical: u32,
    pub physical: u32,
    pub base_mhz: u32,
    pub isa: Vec<String>,
    pub numa_nodes: u32,
    pub hybrid: bool,
    pub xeon: bool,
}

fn cpuid(leaf: u32, sub: u32) -> [u32; 4] {
    // SAFETY: __cpuid_count e seguro em qualquer x86_64; o CPU pode nao
    // suportar a leaf, mas nesse caso ela devolve zeros/valores definidos.
    let r = __cpuid_count(leaf, sub);
    [r.eax, r.ebx, r.ecx, r.edx]
}

fn brand_string() -> String {
    let mut bytes = Vec::with_capacity(48);
    for leaf in 0x8000_0002..=0x8000_0004u32 {
        let [a, b, c, d] = cpuid(leaf, 0);
        for reg in [a, b, c, d] {
            bytes.extend_from_slice(&reg.to_le_bytes());
        }
    }
    let s: String = bytes
        .iter()
        .filter(|&&b| b != 0)
        .map(|&b| b as char)
        .collect();
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn vendor_string() -> String {
    let [_, b, c, d] = cpuid(0, 0);
    let mut bytes = Vec::with_capacity(12);
    for reg in [b, c, d] {
        bytes.extend_from_slice(&reg.to_le_bytes());
    }
    bytes
        .iter()
        .filter(|&&b| b != 0)
        .map(|&b| b as char)
        .collect()
}

fn max_leaf() -> u32 {
    cpuid(0, 0)[0]
}

fn parse_mhz(brand: &str) -> u32 {
    // "Intel(R) Xeon(R) CPU E5-2650 0 @ 2.00GHz"
    if let Some(pos) = brand.find("GHz") {
        let head = &brand[..pos];
        let num: String = head
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        if let Ok(v) = num.parse::<f32>() {
            return (v * 1000.0) as u32;
        }
    }
    if let Some(pos) = brand.find("MHz") {
        let head = &brand[..pos];
        let num: String = head
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        if let Ok(v) = num.parse::<f32>() {
            return v as u32;
        }
    }
    0
}

fn threads_per_core() -> u32 {
    // Leaf 0x1F (V2 topology) com fallback para 0x0B, depois 1.
    let leaf = if max_leaf() >= 0x1F { 0x1F } else { 0x0B };
    let [eax, _, ecx, _] = cpuid(leaf, 0);
    let kind = (ecx >> 8) & 0xFF;
    let shift = eax & 0x1F;
    if kind == 1 && shift > 0 && shift < 31 {
        1u32 << shift
    } else {
        1
    }
}

pub fn detect(numa_nodes: u32) -> Cpu {
    let brand = brand_string();
    let vendor = vendor_string();
    let [eax1, _, ecx1, _] = cpuid(1, 0);
    let stepping = eax1 & 0xF;
    let mut model = (eax1 >> 4) & 0xF;
    let mut family = (eax1 >> 8) & 0xF;
    let ext_model = (eax1 >> 16) & 0xF;
    let ext_family = (eax1 >> 20) & 0xFF;
    if family == 0xF {
        family += ext_family;
    }
    if family == 0x6 || family == 0xF {
        model += ext_model << 4;
    }

    let mut isa: Vec<String> = vec!["SSE2".into()];
    if ecx1 & (1 << 20) != 0 {
        isa.push("SSE4.2".into());
    }
    if ecx1 & (1 << 19) != 0 {
        isa.push("SSE4.1".into());
    }
    if ecx1 & (1 << 25) != 0 {
        isa.push("AES".into());
    }
    if ecx1 & (1 << 28) != 0 {
        isa.push("AVX".into());
    }
    if ecx1 & (1 << 12) != 0 {
        isa.push("FMA3".into());
    }

    let mut hybrid = false;
    if max_leaf() >= 7 {
        let [_, ebx7, _, edx7] = cpuid(7, 0);
        if ebx7 & (1 << 5) != 0 {
            isa.push("AVX2".into());
        }
        if ebx7 & (1 << 3) != 0 {
            isa.push("BMI1".into());
        }
        if ebx7 & (1 << 8) != 0 {
            isa.push("BMI2".into());
        }
        if ebx7 & (1 << 16) != 0 {
            isa.push("AVX512F".into());
        }
        hybrid = edx7 & (1 << 15) != 0;
    }

    let logical = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1);
    let tpc = threads_per_core().min(logical);
    let physical = if tpc > 1 { logical / tpc } else { logical };

    let base_mhz = if max_leaf() >= 0x16 {
        let [e, ..] = cpuid(0x16, 0);
        if e > 0 {
            e
        } else {
            parse_mhz(&brand)
        }
    } else {
        parse_mhz(&brand)
    };

    let xeon = brand.to_uppercase().contains("XEON");

    Cpu {
        brand,
        vendor,
        family,
        model,
        stepping,
        logical,
        physical,
        base_mhz,
        isa,
        numa_nodes,
        hybrid,
        xeon,
    }
}

impl Cpu {
    pub fn has(&self, f: &str) -> bool {
        self.isa.iter().any(|i| i.eq_ignore_ascii_case(f))
    }
}
