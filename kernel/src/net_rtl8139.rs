use crate::console::Serial;
use crate::memory::FrameAllocator;
use core::fmt::Write;
use core::hint::spin_loop;
use core::ptr::{addr_of_mut, copy_nonoverlapping, read_volatile};
use core::sync::atomic::{AtomicBool, Ordering};

const PCI_ADDR: u16 = 0xCF8;
const PCI_DATA: u16 = 0xCFC;

const RTL_VENDOR: u16 = 0x10EC;
const RTL_DEVICE: u16 = 0x8139;

const REG_MAC: u16 = 0x00;
const REG_TX_STATUS: u16 = 0x10;
const REG_TX_ADDR: u16 = 0x20;
const REG_RX_START: u16 = 0x30;
const REG_CMD: u16 = 0x37;
const REG_CAPR: u16 = 0x38;
const REG_IMR: u16 = 0x3C;
const REG_ISR: u16 = 0x3E;
const REG_TX_CONFIG: u16 = 0x40;
const REG_RX_CONFIG: u16 = 0x44;
const REG_CONFIG1: u16 = 0x52;

const CMD_RESET: u8 = 0x10;
const CMD_RX_ENABLE: u8 = 0x08;
const CMD_TX_ENABLE: u8 = 0x04;

const ISR_ROK: u16 = 1 << 0;

const TX_OK: u32 = 1 << 15;
const TX_OWN: u32 = 1 << 13;

const RX_RING: usize = 8192;
const RX_EXTRA: usize = 2048;
const RX_BYTES: usize = RX_RING + RX_EXTRA;

const MAX_FRAME: usize = 1536;
const ETH_HEADER: usize = 14;
const IP_HEADER: usize = 20;

const LOCAL_IP: [u8; 4] = [10, 0, 2, 15];
const NETMASK: [u8; 4] = [255, 255, 255, 0];
const GATEWAY: [u8; 4] = [10, 0, 2, 2];

#[derive(Clone, Copy)]
struct PciNic {
    io_base: u16,
}

struct Rtl8139 {
    io: u16,
    mac: [u8; 6],
    ip: [u8; 4],
    mask: [u8; 4],
    gateway: [u8; 4],

    rx_phys: u64,
    rx_virt: *mut u8,

    tx_phys: [u64; 4],
    tx_virt: [*mut u8; 4],

    rx_offset: u16,
    tx_index: usize,
    sequence: u16,
}

static mut DEVICE: core::mem::MaybeUninit<Rtl8139> = core::mem::MaybeUninit::uninit();

static READY: AtomicBool = AtomicBool::new(false);

fn pci_read32(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    let address = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((function as u32) << 8)
        | ((offset as u32) & 0xFC);

    unsafe {
        asm_outl(PCI_ADDR, address);
        asm_inl(PCI_DATA)
    }
}

fn pci_read16(bus: u8, device: u8, function: u8, offset: u8) -> u16 {
    let value = pci_read32(bus, device, function, offset & 0xFC);
    let shift = ((offset & 2) as u32) * 8;
    ((value >> shift) & 0xFFFF) as u16
}

fn pci_write16(bus: u8, device: u8, function: u8, offset: u8, value: u16) {
    let aligned = offset & 0xFC;
    let mut current = pci_read32(bus, device, function, aligned);
    let shift = ((offset & 2) as u32) * 8;
    let mask = 0xFFFFu32 << shift;

    current = (current & !mask) | ((value as u32) << shift);

    let address = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((function as u32) << 8)
        | aligned as u32;

    unsafe {
        asm_outl(PCI_ADDR, address);
        asm_outl(PCI_DATA, current);
    }
}

fn find_nic() -> Option<PciNic> {
    for device in 0..32u8 {
        for function in 0..8u8 {
            let id = pci_read32(0, device, function, 0);

            if id == 0xFFFF_FFFF {
                continue;
            }

            let vendor = (id & 0xFFFF) as u16;
            let device_id = (id >> 16) as u16;

            if vendor == RTL_VENDOR && device_id == RTL_DEVICE {
                let bar0 = pci_read32(0, device, function, 0x10);

                if bar0 & 1 == 0 {
                    continue;
                }

                let io_base = (bar0 & 0xFFFF_FFFC) as u16;

                let command = pci_read16(0, device, function, 0x04);

                pci_write16(0, device, function, 0x04, command | 0x0005);

                return Some(PciNic { io_base });
            }
        }
    }

    None
}

impl Rtl8139 {
    fn new(nic: PciNic, allocator: &mut FrameAllocator<'_>, physical_offset: u64) -> Option<Self> {
        let rx_a = allocator.allocate()?;
        let rx_b = allocator.allocate()?;
        let rx_c = allocator.allocate()?;

        if rx_b.addr() != rx_a.addr() + 4096 || rx_c.addr() != rx_b.addr() + 4096 {
            return None;
        }

        let rx_phys = rx_a.addr();

        let mut tx_phys = [0u64; 4];
        let mut tx_virt = [core::ptr::null_mut(); 4];

        for i in 0..4 {
            let frame = allocator.allocate()?;
            tx_phys[i] = frame.addr();

            tx_virt[i] = physical_offset.checked_add(frame.addr())? as *mut u8;
        }

        let rx_virt = physical_offset.checked_add(rx_phys)? as *mut u8;

        let mut mac = [0u8; 6];

        for i in 0..6 {
            mac[i] = unsafe { io_inb(nic.io_base + REG_MAC + i as u16) };
        }

        Some(Self {
            io: nic.io_base,
            mac,
            ip: LOCAL_IP,
            mask: NETMASK,
            gateway: GATEWAY,
            rx_phys,
            rx_virt,
            tx_phys,
            tx_virt,
            rx_offset: 0,
            tx_index: 0,
            sequence: 1,
        })
    }

    fn initialize(&mut self) -> bool {
        unsafe {
            io_outb(self.io + REG_CONFIG1, 0);

            io_outb(self.io + REG_CMD, CMD_RESET);
        }

        for _ in 0..2_000_000usize {
            let status = unsafe { io_inb(self.io + REG_CMD) };

            if status & CMD_RESET == 0 {
                break;
            }

            spin_loop();
        }

        let status = unsafe { io_inb(self.io + REG_CMD) };

        if status & CMD_RESET != 0 {
            return false;
        }

        unsafe {
            io_outl(self.io + REG_RX_START, self.rx_phys as u32);

            io_outw(self.io + REG_ISR, 0xFFFF);

            io_outw(self.io + REG_IMR, 0);

            io_outl(self.io + REG_TX_CONFIG, 0x0300_0700);

            io_outl(self.io + REG_RX_CONFIG, 0x0000_008F);

            io_outw(self.io + REG_CAPR, 0);

            io_outb(self.io + REG_CMD, CMD_RX_ENABLE | CMD_TX_ENABLE);
        }

        true
    }

    fn read_rx_u16(&self, offset: usize) -> u16 {
        unsafe {
            let a = read_volatile(self.rx_virt.add(offset));
            let b = read_volatile(self.rx_virt.add(offset + 1));

            u16::from_le_bytes([a, b])
        }
    }

    fn receive(&mut self, out: &mut [u8; MAX_FRAME]) -> Option<usize> {
        let cmd = unsafe { io_inb(self.io + REG_CMD) };

        if cmd & 0x01 != 0 {
            return None;
        }

        let isr = unsafe { io_inw(self.io + REG_ISR) };

        if isr != 0 {
            unsafe {
                io_outw(self.io + REG_ISR, isr);
            }
        }

        let offset = self.rx_offset as usize;

        let status = self.read_rx_u16(offset);
        let length = self.read_rx_u16(offset + 2) as usize;

        if status & 1 == 0 || length < 4 || length > 2048 {
            self.rx_offset = 0;

            unsafe {
                io_outw(self.io + REG_CAPR, 0xFFF0);
            }

            return None;
        }

        let data_len = length - 4;

        if data_len > MAX_FRAME {
            return None;
        }

        unsafe {
            copy_nonoverlapping(self.rx_virt.add(offset + 4), out.as_mut_ptr(), data_len);
        }

        let advance = (length + 4 + 3) & !3;

        self.rx_offset = ((self.rx_offset as usize + advance) % RX_RING) as u16;

        let capr = self.rx_offset.wrapping_sub(16);

        unsafe {
            io_outw(self.io + REG_CAPR, capr);
        }

        Some(data_len)
    }

    fn transmit(&mut self, frame: &[u8]) -> bool {
        if frame.len() < 60 || frame.len() > MAX_FRAME {
            return false;
        }

        let index = self.tx_index;
        let tx = self.tx_virt[index];

        unsafe {
            copy_nonoverlapping(frame.as_ptr(), tx, frame.len());
        }

        unsafe {
            io_outl(
                self.io + REG_TX_ADDR + (index as u16 * 4),
                self.tx_phys[index] as u32,
            );
        }

        for _ in 0..1_000_000usize {
            let status = unsafe { io_inl(self.io + REG_TX_STATUS + (index as u16 * 4)) };

            if status & TX_OWN != 0 {
                break;
            }

            spin_loop();
        }

        unsafe {
            io_outl(
                self.io + REG_TX_STATUS + (index as u16 * 4),
                frame.len() as u32,
            );
        }

        for _ in 0..2_000_000usize {
            let status = unsafe { io_inl(self.io + REG_TX_STATUS + (index as u16 * 4)) };

            if status & TX_OWN != 0 {
                self.tx_index = (index + 1) & 3;
                return status & TX_OK != 0;
            }

            spin_loop();
        }

        false
    }

    fn poll_for_arp(&mut self, wanted: [u8; 4]) -> Option<[u8; 6]> {
        let mut frame = [0u8; MAX_FRAME];

        for _ in 0..8_000_000usize {
            if let Some(len) = self.receive(&mut frame) {
                if len >= 42
                    && u16::from_be_bytes([frame[12], frame[13]]) == 0x0806
                    && u16::from_be_bytes([frame[20], frame[21]]) == 2
                    && frame[28..32] == wanted
                {
                    let mut mac = [0u8; 6];
                    mac.copy_from_slice(&frame[22..28]);
                    return Some(mac);
                }
            } else {
                spin_loop();
            }
        }

        None
    }

    fn arp(&mut self, ip: [u8; 4]) -> bool {
        let mut frame = [0u8; 60];

        for byte in &mut frame {
            *byte = 0;
        }

        for i in 0..6 {
            frame[i] = 0xFF;
            frame[6 + i] = self.mac[i];
        }

        frame[12] = 0x08;
        frame[13] = 0x06;

        frame[14..16].copy_from_slice(&1u16.to_be_bytes());

        frame[16..18].copy_from_slice(&0x0800u16.to_be_bytes());

        frame[18] = 6;
        frame[19] = 4;

        frame[20..22].copy_from_slice(&1u16.to_be_bytes());

        frame[22..28].copy_from_slice(&self.mac);

        frame[28..32].copy_from_slice(&self.ip);

        frame[32..38].fill(0);

        frame[38..42].copy_from_slice(&ip);

        self.transmit(&frame)
    }

    fn ping_ip(&mut self, target: [u8; 4]) -> bool {
        let next_hop = if same_subnet(target, self.ip, self.mask) {
            target
        } else {
            self.gateway
        };

        if !self.arp(next_hop) {
            return false;
        }

        let gateway_mac = self.poll_for_arp(next_hop);

        let Some(dst_mac) = gateway_mac else {
            return false;
        };

        let mut frame = [0u8; 60];

        for byte in &mut frame {
            *byte = 0;
        }

        frame[0..6].copy_from_slice(&dst_mac);

        frame[6..12].copy_from_slice(&self.mac);

        frame[12..14].copy_from_slice(&0x0800u16.to_be_bytes());

        frame[14] = 0x45;
        frame[15] = 0;

        let total_len = 20u16 + 8;

        frame[16..18].copy_from_slice(&total_len.to_be_bytes());

        frame[18..20].copy_from_slice(&self.sequence.to_be_bytes());

        frame[20..22].copy_from_slice(&0x4000u16.to_be_bytes());

        frame[22] = 64;
        frame[23] = 1;

        frame[24] = 0;
        frame[25] = 0;

        frame[26..30].copy_from_slice(&self.ip);

        frame[30..34].copy_from_slice(&target);

        let ip_sum = checksum16(&frame[14..34]);

        frame[24..26].copy_from_slice(&ip_sum.to_be_bytes());

        frame[34] = 8;
        frame[35] = 0;
        frame[36] = 0;
        frame[37] = 0;

        frame[38..40].copy_from_slice(&0x4256u16.to_be_bytes());

        frame[40..42].copy_from_slice(&self.sequence.to_be_bytes());

        let icmp_sum = checksum16(&frame[34..42]);

        frame[36..38].copy_from_slice(&icmp_sum.to_be_bytes());

        let sequence = self.sequence;

        self.sequence = self.sequence.wrapping_add(1).max(1);

        if !self.transmit(&frame) {
            return false;
        }

        let mut rx = [0u8; MAX_FRAME];

        for _ in 0..8_000_000usize {
            if let Some(len) = self.receive(&mut rx) {
                if len < 42 {
                    continue;
                }

                if u16::from_be_bytes([rx[12], rx[13]]) != 0x0800 {
                    continue;
                }

                if rx[23] != 1 {
                    continue;
                }

                if rx[26..30] != target {
                    continue;
                }

                if rx[30..34] != self.ip {
                    continue;
                }

                let ihl = ((rx[14] & 0x0F) as usize) * 4;

                if ihl < 20 || len < 14 + ihl + 8 {
                    continue;
                }

                let icmp = 14 + ihl;

                if rx[icmp] != 0 || rx[icmp + 1] != 0 {
                    continue;
                }

                let id = u16::from_be_bytes([rx[icmp + 4], rx[icmp + 5]]);

                let seq = u16::from_be_bytes([rx[icmp + 6], rx[icmp + 7]]);

                if id == 0x4256 && seq == sequence {
                    return true;
                }
            } else {
                spin_loop();
            }
        }

        false
    }
}

pub fn init(allocator: &mut FrameAllocator<'_>, physical_offset: u64, out: &mut Serial) -> bool {
    if READY.load(Ordering::Acquire) {
        return true;
    }

    let Some(pci) = find_nic() else {
        writeln!(out, "network           : RTL8139 NOT FOUND").ok();
        return false;
    };

    writeln!(out, "RTL8139           : I/O 0x{:04X}", pci.io_base).ok();

    let Some(mut device) = Rtl8139::new(pci, allocator, physical_offset) else {
        writeln!(out, "network buffers    : allocation failed").ok();
        return false;
    };

    if !device.initialize() {
        writeln!(out, "RTL8139 init       : FAILED").ok();
        return false;
    }

    writeln!(out, "RTL8139            : ONLINE").ok();

    writeln!(
        out,
        "MAC                : {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        device.mac[0], device.mac[1], device.mac[2], device.mac[3], device.mac[4], device.mac[5]
    )
    .ok();

    writeln!(
        out,
        "IPv4               : {}.{}.{}.{}/24",
        device.ip[0], device.ip[1], device.ip[2], device.ip[3]
    )
    .ok();

    unsafe {
        addr_of_mut!(DEVICE).write(core::mem::MaybeUninit::new(device));
    }

    READY.store(true, Ordering::Release);
    true
}

fn with_device<R>(f: impl FnOnce(&mut Rtl8139) -> R) -> Option<R> {
    if !READY.load(Ordering::Acquire) {
        return None;
    }

    unsafe {
        let ptr = addr_of_mut!(DEVICE).cast::<Rtl8139>();

        Some(f(&mut *ptr))
    }
}

pub fn status<W: Write>(out: &mut W) {
    let _ = with_device(|device| {
        writeln!(out, "device=RTL8139").ok();

        writeln!(
            out,
            "mac={:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            device.mac[0],
            device.mac[1],
            device.mac[2],
            device.mac[3],
            device.mac[4],
            device.mac[5]
        )
        .ok();

        writeln!(
            out,
            "ip={}.{}.{}.{}/24",
            device.ip[0], device.ip[1], device.ip[2], device.ip[3]
        )
        .ok();

        writeln!(
            out,
            "gateway={}.{}.{}.{}",
            device.gateway[0], device.gateway[1], device.gateway[2], device.gateway[3]
        )
        .ok();
    });
}

pub fn ping<W: Write>(text: &[u8], out: &mut W) {
    let Some(target) = parse_ipv4(text) else {
        writeln!(out, "ping: invalid IPv4 address").ok();
        return;
    };

    write!(
        out,
        "PING {}.{}.{}.{}\n",
        target[0], target[1], target[2], target[3]
    )
    .ok();

    let result = with_device(|device| device.ping_ip(target));

    match result {
        Some(true) => {
            writeln!(
                out,
                "reply from {}.{}.{}.{}",
                target[0], target[1], target[2], target[3]
            )
            .ok();

            writeln!(out, "1 packets transmitted, 1 received").ok();
        }

        Some(false) => {
            writeln!(out, "request timed out").ok();

            writeln!(out, "1 packets transmitted, 0 received").ok();
        }

        None => {
            writeln!(out, "network: offline").ok();
        }
    }
}

fn parse_ipv4(input: &[u8]) -> Option<[u8; 4]> {
    let mut result = [0u8; 4];
    let mut octet = 0usize;
    let mut value = 0u16;
    let mut digits = 0usize;

    for &byte in input {
        if byte == b'.' {
            if octet >= 3 || digits == 0 || value > 255 {
                return None;
            }

            result[octet] = value as u8;

            octet += 1;
            value = 0;
            digits = 0;
        } else if byte >= b'0' && byte <= b'9' {
            if digits >= 3 {
                return None;
            }

            value = value * 10 + (byte - b'0') as u16;

            digits += 1;
        } else {
            return None;
        }
    }

    if octet != 3 || digits == 0 || value > 255 {
        return None;
    }

    result[3] = value as u8;
    Some(result)
}

fn same_subnet(a: [u8; 4], b: [u8; 4], mask: [u8; 4]) -> bool {
    for i in 0..4 {
        if (a[i] & mask[i]) != (b[i] & mask[i]) {
            return false;
        }
    }

    true
}

fn checksum16(data: &[u8]) -> u16 {
    let mut sum = 0u32;
    let mut i = 0usize;

    while i + 1 < data.len() {
        sum += u16::from_be_bytes([data[i], data[i + 1]]) as u32;
        i += 2;
    }

    if i < data.len() {
        sum += (data[i] as u32) << 8;
    }

    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }

    !(sum as u16)
}

unsafe fn asm_outl(port: u16, value: u32) {
    unsafe {
        core::arch::asm!(
            "out dx, eax",
            in("dx") port,
            in("eax") value,
            options(nostack, preserves_flags)
        );
    }
}

unsafe fn asm_inl(port: u16) -> u32 {
    let value: u32;

    unsafe {
        core::arch::asm!(
            "in eax, dx",
            in("dx") port,
            lateout("eax") value,
            options(nostack, preserves_flags)
        );
    }

    value
}

unsafe fn io_outb(port: u16, value: u8) {
    unsafe {
        core::arch::asm!(
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nostack, preserves_flags)
        );
    }
}

unsafe fn io_inb(port: u16) -> u8 {
    let value: u8;

    unsafe {
        core::arch::asm!(
            "in al, dx",
            in("dx") port,
            lateout("al") value,
            options(nostack, preserves_flags)
        );
    }

    value
}

unsafe fn io_outw(port: u16, value: u16) {
    unsafe {
        core::arch::asm!(
            "out dx, ax",
            in("dx") port,
            in("ax") value,
            options(nostack, preserves_flags)
        );
    }
}

unsafe fn io_inw(port: u16) -> u16 {
    let value: u16;

    unsafe {
        core::arch::asm!(
            "in ax, dx",
            in("dx") port,
            lateout("ax") value,
            options(nostack, preserves_flags)
        );
    }

    value
}

unsafe fn io_outl(port: u16, value: u32) {
    unsafe {
        core::arch::asm!(
            "out dx, eax",
            in("dx") port,
            in("eax") value,
            options(nostack, preserves_flags)
        );
    }
}

unsafe fn io_inl(port: u16) -> u32 {
    let value: u32;

    unsafe {
        core::arch::asm!(
            "in eax, dx",
            in("dx") port,
            lateout("eax") value,
            options(nostack, preserves_flags)
        );
    }

    value
}
