use core::arch::asm;
use core::arch::x86_64::__cpuid;

const IA32_APIC_BASE: u32 = 0x1b;
const APIC_ENABLE: u64 = 1 << 11;
const X2APIC_ENABLE: u64 = 1 << 10;

static mut APIC_SUPPORTED: bool = false;
static mut X2APIC_SUPPORTED: bool = false;
static mut APIC_ENABLED: bool = false;
static mut APIC_BASE: u64 = 0;

#[inline(always)]
unsafe fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    asm!(
        "rdmsr",
        in("ecx") msr,
        lateout("eax") low,
        lateout("edx") high,
        options(nostack, preserves_flags),
    );
    ((high as u64) << 32) | low as u64
}

pub fn detect() {
    let leaf1 = __cpuid(1);
    let supported = leaf1.edx & (1 << 9) != 0;
    let x2 = leaf1.ecx & (1 << 21) != 0;
    let base = unsafe { rdmsr(IA32_APIC_BASE) };

    unsafe {
        APIC_SUPPORTED = supported;
        X2APIC_SUPPORTED = x2;
        APIC_ENABLED = base & APIC_ENABLE != 0;
        APIC_BASE = base & 0x000f_ffff_ffff_f000;
    }
}

pub fn supported() -> bool {
    unsafe { APIC_SUPPORTED }
}

pub fn x2apic_supported() -> bool {
    unsafe { X2APIC_SUPPORTED }
}

pub fn enabled() -> bool {
    unsafe { APIC_ENABLED }
}

pub fn base() -> u64 {
    unsafe { APIC_BASE }
}

pub fn x2apic_enabled() -> bool {
    unsafe {
        rdmsr(IA32_APIC_BASE) & (APIC_ENABLE | X2APIC_ENABLE) == (APIC_ENABLE | X2APIC_ENABLE)
    }
}
