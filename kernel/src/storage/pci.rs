use core::arch::asm;
#[derive(Clone, Copy)]
pub struct PciDevice {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor: u16,
    pub device_id: u16,
    pub bar0: u64,
}
const BUS_SCAN: (u16, u16) = (0, 255);
pub fn find_all_nvme() -> [Option<PciDevice>; 8] {
    let mut out: [Option<PciDevice>; 8] = [None, None, None, None, None, None, None, None];
    let mut count = 0;
    let (bus_start, bus_end) = BUS_SCAN;
    for bus in bus_start..=bus_end {
        let bus = bus as u8;
        for device in 0u8..32 {
            for function in 0u8..8 {
                if count >= 8 {
                    return out;
                }
                let vendor = read16(bus, device, function, 0x00);
                if vendor == 0xFFFF {
                    continue;
                }
                let class = read8(bus, device, function, 0x0B);
                let subclass = read8(bus, device, function, 0x0A);
                let prog_if = read8(bus, device, function, 0x09);
                if class != 0x01 || subclass != 0x08 || prog_if != 0x02 {
                    continue;
                }
                let device_id = read16(bus, device, function, 0x02);
                let bar_low = read32(bus, device, function, 0x10);
                if bar_low == 0 || bar_low == 0xFFFF_FFFF {
                    continue;
                }
                let bar_type = (bar_low >> 1) & 0x3;
                let bar0 = if bar_type == 0x2 {
                    let high = read32(bus, device, function, 0x14);
                    (bar_low as u64 & 0xFFFF_FFF0) | ((high as u64) << 32)
                } else {
                    (bar_low as u64) & 0xFFFF_FFF0
                };
                enable_bus_master(bus, device, function);
                out[count] = Some(PciDevice {
                    bus,
                    device,
                    function,
                    vendor,
                    device_id,
                    bar0,
                });
                count += 1;
            }
        }
    }
    out
}
pub fn find_nvme() -> Option<PciDevice> {
    find_all_nvme()[0]
}
#[inline(always)]
fn enable_bus_master(b: u8, d: u8, f: u8) {
    let c = read16(b, d, f, 0x04);
    write16(b, d, f, 0x04, c | 0x0006);
}
#[inline(always)]
fn read8(b: u8, d: u8, f: u8, o: u8) -> u8 {
    let v = read32(b, d, f, o & 0xFC);
    let s = (o & 3) * 8;
    (v >> s) as u8
}
#[inline(always)]
fn read16(b: u8, d: u8, f: u8, o: u8) -> u16 {
    let v = read32(b, d, f, o & 0xFC);
    let s = (o & 2) * 8;
    (v >> s) as u16
}
#[inline(always)]
fn read32(b: u8, d: u8, f: u8, o: u8) -> u32 {
    let a = 0x8000_0000u32
        | ((b as u32) << 16)
        | ((d as u32) << 11)
        | ((f as u32) << 8)
        | ((o as u32) & 0xFC);
    unsafe {
        out32(0xCF8, a);
        in32(0xCFC)
    }
}
#[inline(always)]
fn write16(b: u8, d: u8, f: u8, o: u8, v: u16) {
    let al = o & 0xFC;
    let old = read32(b, d, f, al);
    let sh = (o & 2) * 8;
    let m = (old & !(0xFFFFu32 << sh)) | ((v as u32) << sh);
    let a =
        0x8000_0000u32 | ((b as u32) << 16) | ((d as u32) << 11) | ((f as u32) << 8) | al as u32;
    unsafe {
        out32(0xCF8, a);
        out32(0xCFC, m);
    }
}
#[inline(always)]
unsafe fn out32(p: u16, v: u32) {
    unsafe {
        core::arch::asm!("out dx, eax", in("dx") p, in("eax") v, options(nostack,preserves_flags));
    }
}
#[inline(always)]
unsafe fn in32(p: u16) -> u32 {
    let v: u32;
    unsafe {
        core::arch::asm!("in eax, dx", in("dx") p, out("eax") v, options(nostack,preserves_flags));
    }
    v
}
