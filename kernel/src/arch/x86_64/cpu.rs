use core::arch::x86_64::__cpuid;
use core::fmt::Write;

#[derive(Clone, Copy)]
pub struct CpuInfo {
    pub vendor: [u8; 12],
    pub brand: [u8; 48],
    pub family: u32,
    pub model: u32,
    pub stepping: u32,
    pub max_basic_leaf: u32,
    pub max_extended_leaf: u32,
    pub sse: bool,
    pub sse2: bool,
    pub sse42: bool,
    pub avx: bool,
    pub avx2: bool,
    pub aes: bool,
    pub pclmulqdq: bool,
    pub popcnt: bool,
    pub bmi1: bool,
    pub bmi2: bool,
    pub erms: bool,
    pub rdseed: bool,
    pub adx: bool,
    pub fsgsbase: bool,
}

impl CpuInfo {
    pub fn detect() -> Self {
        let leaf0 = unsafe { __cpuid(0) };

        let max_basic_leaf = leaf0.eax;

        let mut vendor = [0u8; 12];

        vendor[0..4].copy_from_slice(&leaf0.ebx.to_le_bytes());

        vendor[4..8].copy_from_slice(&leaf0.edx.to_le_bytes());

        vendor[8..12].copy_from_slice(&leaf0.ecx.to_le_bytes());

        let max_extended_leaf = unsafe { __cpuid(0x80000000).eax };

        let mut brand = [0u8; 48];

        if max_extended_leaf >= 0x80000004 {
            for i in 0..3u32 {
                let r = unsafe { __cpuid(0x80000002 + i) };

                let base = (i as usize) * 16;

                brand[base..base + 4].copy_from_slice(&r.eax.to_le_bytes());

                brand[base + 4..base + 8].copy_from_slice(&r.ebx.to_le_bytes());

                brand[base + 8..base + 12].copy_from_slice(&r.ecx.to_le_bytes());

                brand[base + 12..base + 16].copy_from_slice(&r.edx.to_le_bytes());
            }
        }

        let leaf1 = unsafe { __cpuid(1) };

        let base_family = (leaf1.eax >> 8) & 0xF;

        let base_model = (leaf1.eax >> 4) & 0xF;

        let extended_family = (leaf1.eax >> 20) & 0xFF;

        let extended_model = (leaf1.eax >> 16) & 0xF;

        let family = if base_family == 0xF {
            base_family + extended_family
        } else {
            base_family
        };

        let model = if base_family == 0x6 || base_family == 0xF {
            base_model | (extended_model << 4)
        } else {
            base_model
        };

        let leaf7 = if max_basic_leaf >= 7 {
            unsafe { __cpuid(7) }
        } else {
            unsafe { __cpuid(0) }
        };

        Self {
            vendor,
            brand,

            family,
            model,
            stepping: leaf1.eax & 0xF,

            max_basic_leaf,
            max_extended_leaf,

            sse: leaf1.edx & (1 << 25) != 0,
            sse2: leaf1.edx & (1 << 26) != 0,
            sse42: leaf1.ecx & (1 << 20) != 0,

            avx: leaf1.ecx & (1 << 28) != 0,
            avx2: leaf7.ebx & (1 << 5) != 0,

            aes: leaf1.ecx & (1 << 25) != 0,
            pclmulqdq: leaf1.ecx & (1 << 1) != 0,
            popcnt: leaf1.ecx & (1 << 23) != 0,

            bmi1: leaf7.ebx & (1 << 3) != 0,
            bmi2: leaf7.ebx & (1 << 8) != 0,
            erms: leaf7.ebx & (1 << 9) != 0,
            rdseed: leaf7.ebx & (1 << 18) != 0,
            adx: leaf7.ebx & (1 << 19) != 0,
            fsgsbase: leaf7.ebx & (1 << 0) != 0,
        }
    }

    pub fn microarchitecture(&self) -> &'static str {
        if self.family == 6 && self.model == 60 {
            "HASWELL"
        } else if self.family == 6 {
            "INTEL-FAMILY-6"
        } else {
            "X86-64"
        }
    }

    pub fn report<W: Write>(&self, out: &mut W) {
        writeln!(out).ok();
        writeln!(out, "CPU DETECTION").ok();
        writeln!(out, "----------------------------------------").ok();

        write!(out, "vendor            : ").ok();

        for &b in &self.vendor {
            if b != 0 {
                write!(out, "{}", b as char).ok();
            }
        }

        writeln!(out).ok();

        write!(out, "brand             : ").ok();

        for &b in &self.brand {
            if b != 0 {
                write!(out, "{}", b as char).ok();
            }
        }

        writeln!(out).ok();

        writeln!(out, "microarchitecture : {}", self.microarchitecture()).ok();

        writeln!(out, "family/model      : {}/{}", self.family, self.model).ok();

        writeln!(out, "stepping          : {}", self.stepping).ok();

        writeln!(out, "CPUID basic max   : 0x{:08X}", self.max_basic_leaf).ok();

        writeln!(out, "CPUID extended max: 0x{:08X}", self.max_extended_leaf).ok();

        write!(out, "features          : ").ok();

        feature(out, "SSE", self.sse);
        feature(out, "SSE2", self.sse2);
        feature(out, "SSE4.2", self.sse42);
        feature(out, "AVX", self.avx);
        feature(out, "AVX2", self.avx2);
        feature(out, "AES", self.aes);
        feature(out, "PCLMULQDQ", self.pclmulqdq);
        feature(out, "POPCNT", self.popcnt);
        feature(out, "BMI1", self.bmi1);
        feature(out, "BMI2", self.bmi2);
        feature(out, "ERMS", self.erms);
        feature(out, "RDSEED", self.rdseed);
        feature(out, "ADX", self.adx);
        feature(out, "FSGSBASE", self.fsgsbase);

        writeln!(out).ok();
    }
}

fn feature<W: Write>(out: &mut W, name: &str, enabled: bool) {
    if enabled {
        write!(out, "{} ", name).ok();
    }
}
