use crate::console::Serial;
use crate::memory::FrameAllocator;
use core::fmt::Write;
use core::ptr::{copy_nonoverlapping, read_volatile, write_bytes, write_volatile};
use core::sync::atomic::{Ordering, fence};

const PCI_ADDR: u16 = 0xCF8;
const PCI_DATA: u16 = 0xCFC;
const EHCI_CLASS: u8 = 0x0C;
const EHCI_SUBCLASS: u8 = 0x03;
const EHCI_PROGIF: u8 = 0x20;
const USBCMD: usize = 0x00;
const USBINTR: usize = 0x08;
const CTRLDSSEGMENT: usize = 0x10;
const ASYNCLISTADDR: usize = 0x18;
const CONFIGFLAG: usize = 0x40;
const PORTSC: usize = 0x44;
const USBCMD_RUN: u32 = 1;
const USBCMD_RESET: u32 = 1 << 1;
const USBCMD_ASE: u32 = 1 << 5;
const PORTSC_CCS: u32 = 1;
const PORTSC_PED: u32 = 1 << 2;
const PORTSC_PR: u32 = 1 << 8;
const PORTSC_PP: u32 = 1 << 12;
const PORTSC_PSPD_MASK: u32 = 3 << 26;
const PORTSC_PSPD_HIGH: u32 = 2 << 26;
const QTD_ACTIVE: u32 = 1 << 7;
const QTD_HALTED: u32 = 1 << 6;
const QTD_BUFERR: u32 = 1 << 5;
const QTD_BABBLE: u32 = 1 << 4;
const QTD_XACTERR: u32 = 1 << 3;
const QTD_MISSED: u32 = 1 << 2;
const QTD_IOC: u32 = 1 << 15;
const QTD_TOGGLE: u32 = 1 << 31;
const QTD_PID_OUT: u32 = 0;
const QTD_PID_IN: u32 = 1 << 8;
const QTD_PID_SETUP: u32 = 2 << 8;
const LINK_TERMINATE: u32 = 1;
const LINK_QH: u32 = 2;
const USB_REQ_GET_DESCRIPTOR: u8 = 6;
const USB_REQ_SET_ADDRESS: u8 = 5;
const USB_REQ_SET_CONFIGURATION: u8 = 9;
const USB_DT_DEVICE: u8 = 1;
const USB_DT_CONFIG: u8 = 2;
const USB_CLASS_GET_STATUS: u8 = 0;
const USB_CLASS_SET_FEATURE: u8 = 3;
const HUB_PORT_RESET: u16 = 4;
const USB_VID_INTEL: u16 = 0x8087;
const USB_PID_INTEL_IRH: u16 = 0x8000;
const USB_VID_REALTEK: u16 = 0x0BDA;
const USB_PID_RTL8188FTV: u16 = 0xF179;
const PAGE: usize = 4096;
const QH_OFF: usize = 0x000;
const QTD_SETUP_OFF: usize = 0x040;
const QTD_DATA_OFF: usize = 0x080;
const QTD_STATUS_OFF: usize = 0x0C0;
const SETUP_BUF_OFF: usize = 0x200;
const DATA_BUF_OFF: usize = 0x300;

#[derive(Clone, Copy)]
pub struct UsbDevice {
    pub address: u8,
    pub vendor: u16,
    pub product: u16,
    pub max_packet: u16,
    pub configuration: u8,
    pub bulk_in: u8,
    pub bulk_out: [u8; 2],
    pub bulk_packet: u16,
}

pub struct UsbBus {
    op: *mut u8,
    qh_phys: u64,
    page_phys: u64,
    page_virt: *mut u8,
    root_ports: u8,
    next_address: u8,
    pub wifi: Option<UsbDevice>,
}

unsafe impl Send for UsbBus {}
unsafe impl Sync for UsbBus {}

fn pci_read32(bus: u8, dev: u8, fun: u8, off: u8) -> u32 {
    let a = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((fun as u32) << 8)
        | ((off as u32) & 0xFC);
    unsafe {
        out32(PCI_ADDR, a);
        in32(PCI_DATA)
    }
}

fn pci_write16(bus: u8, dev: u8, fun: u8, off: u8, value: u16) {
    let aligned = off & 0xFC;
    let old = pci_read32(bus, dev, fun, aligned);
    let shift = ((off & 2) * 8) as u32;
    let merged = (old & !(0xFFFFu32 << shift)) | ((value as u32) << shift);
    let a = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((fun as u32) << 8)
        | aligned as u32;
    unsafe {
        out32(PCI_ADDR, a);
        out32(PCI_DATA, merged);
    }
}

fn find_ehci() -> Option<(u64, u8, u8, u8)> {
    for bus in 0u16..=255 {
        for dev in 0u8..32 {
            for fun in 0u8..8 {
                let b = bus as u8;
                let id = pci_read32(b, dev, fun, 0);
                if id == 0xFFFF_FFFF {
                    continue;
                }
                let class = pci_read32(b, dev, fun, 8);
                if (class >> 24) as u8 != EHCI_CLASS
                    || (class >> 16) as u8 != EHCI_SUBCLASS
                    || (class >> 8) as u8 != EHCI_PROGIF
                {
                    continue;
                }
                let bar = pci_read32(b, dev, fun, 0x10);
                if bar & 1 != 0 || bar == 0 || bar == 0xFFFF_FFFF {
                    continue;
                }
                let mut base = (bar & 0xFFFF_FFF0) as u64;
                if ((bar >> 1) & 3) == 2 {
                    base |= (pci_read32(b, dev, fun, 0x14) as u64) << 32;
                }
                let command = (pci_read32(b, dev, fun, 4) & 0xFFFF) as u16;
                pci_write16(b, dev, fun, 4, command | 0x0006);
                return Some((base, b, dev, fun));
            }
        }
    }
    None
}

impl UsbBus {
    pub fn new(
        allocator: &mut FrameAllocator<'_>,
        physical_offset: u64,
        out: &mut Serial,
    ) -> Option<Self> {
        let (base, bus, dev, fun) = find_ehci()?;
        writeln!(
            out,
            "USB EHCI          : {:02X}:{:02X}.{} @ 0x{:016X}",
            bus, dev, fun, base
        )
        .ok()?;
        let frame = allocator.allocate()?;
        let page_phys = frame.addr();
        if page_phys >= 0x1_0000_0000 {
            writeln!(out, "USB DMA           : frame above 4GiB").ok();
            return None;
        }
        let page_virt = physical_offset.checked_add(page_phys)? as *mut u8;
        unsafe {
            write_bytes(page_virt, 0, PAGE);
        }
        let cap_len = read_mmio8(base, 0) as usize;
        let op = (base + cap_len as u64) as *mut u8;
        let root_ports = (read_mmio32(base, 4) as u8 & 0x0F).max(1);
        let mut usb = Self {
            op,
            qh_phys: page_phys,
            page_phys,
            page_virt,
            root_ports,
            next_address: 1,
            wifi: None,
        };
        usb.controller_reset();
        usb.reg_write32(USBINTR, 0);
        usb.reg_write32(CTRLDSSEGMENT, 0);
        usb.setup_qh_endpoint(0, 0, 64);
        usb.reg_write32(ASYNCLISTADDR, usb.qh_phys as u32);
        usb.reg_write32(CONFIGFLAG, 1);
        usb.reg_write32(USBCMD, usb.reg32(USBCMD) | USBCMD_ASE | USBCMD_RUN);
        fence(Ordering::SeqCst);
        writeln!(out, "USB root ports    : {}", usb.root_ports).ok();
        usb.enumerate_hierarchy(out);
        Some(usb)
    }

    fn controller_reset(&mut self) {
        let cmd = self.reg32(USBCMD) & !(USBCMD_RUN | USBCMD_ASE);
        self.reg_write32(USBCMD, cmd);
        for _ in 0..2_000_000 {
            if self.reg32(USBCMD) & (USBCMD_RUN | USBCMD_ASE) == 0 {
                break;
            }
            core::hint::spin_loop();
        }
        self.reg_write32(USBCMD, cmd | USBCMD_RESET);
        for _ in 0..2_000_000 {
            if self.reg32(USBCMD) & USBCMD_RESET == 0 {
                break;
            }
            core::hint::spin_loop();
        }
    }

    fn enumerate_hierarchy(&mut self, out: &mut Serial) {
        for root in 0..self.root_ports {
            if !self.reset_root_port(root) {
                continue;
            }
            let Some(root_dev) = self.enumerate_device(0, out) else {
                continue;
            };
            if root_dev.vendor == USB_VID_REALTEK && root_dev.product == USB_PID_RTL8188FTV {
                self.wifi = Some(root_dev);
                return;
            }
            if root_dev.vendor == USB_VID_INTEL
                && root_dev.product == USB_PID_INTEL_IRH
                && self.hub_reset_port(root_dev.address, 5)
            {
                if let Some(child) = self.enumerate_device(0, out) {
                    if child.vendor == USB_VID_REALTEK && child.product == USB_PID_RTL8188FTV {
                        self.wifi = Some(child);
                        return;
                    }
                }
            }
        }
    }

    fn reset_root_port(&mut self, port: u8) -> bool {
        let off = PORTSC + port as usize * 4;
        let mut v = self.reg32(off);
        if v & PORTSC_CCS == 0 {
            return false;
        }
        v |= PORTSC_PP | PORTSC_PR;
        self.reg_write32(off, v);
        let end = crate::arch::x86_64::timer_ticks().saturating_add(15);
        while crate::arch::x86_64::timer_ticks() < end {
            core::hint::spin_loop();
        }
        self.reg_write32(off, self.reg32(off) & !PORTSC_PR);
        let end = crate::arch::x86_64::timer_ticks().saturating_add(5);
        while crate::arch::x86_64::timer_ticks() < end {
            core::hint::spin_loop();
        }
        let s = self.reg32(off);
        s & PORTSC_CCS != 0 && s & PORTSC_PED != 0 && s & PORTSC_PSPD_MASK == PORTSC_PSPD_HIGH
    }

    fn hub_reset_port(&mut self, address: u8, port: u8) -> bool {
        let mut status = [0u8; 4];
        let _ = self.control_in(
            address,
            0xA3,
            USB_CLASS_GET_STATUS,
            0,
            port as u16,
            &mut status,
        );
        if self
            .control_no_data(
                address,
                0x23,
                USB_CLASS_SET_FEATURE,
                HUB_PORT_RESET,
                port as u16,
            )
            .is_err()
        {
            return false;
        }
        let end = crate::arch::x86_64::timer_ticks().saturating_add(25);
        while crate::arch::x86_64::timer_ticks() < end {
            core::hint::spin_loop();
        }
        for _ in 0..8 {
            if self
                .control_in(
                    address,
                    0xA3,
                    USB_CLASS_GET_STATUS,
                    0,
                    port as u16,
                    &mut status,
                )
                .is_ok()
            {
                let v = u32::from_le_bytes(status);
                if v & 1 != 0 && v & 2 != 0 {
                    return true;
                }
            }
            let end = crate::arch::x86_64::timer_ticks().saturating_add(2);
            while crate::arch::x86_64::timer_ticks() < end {
                core::hint::spin_loop();
            }
        }
        false
    }

    fn enumerate_device(&mut self, old_address: u8, out: &mut Serial) -> Option<UsbDevice> {
        let mut d = [0u8; 18];
        self.control_in(old_address, 0x80, USB_REQ_GET_DESCRIPTOR, 0x0100, 0, &mut d)
            .ok()?;
        if d[0] < 18 || d[1] != USB_DT_DEVICE {
            return None;
        }
        let vendor = u16::from_le_bytes([d[8], d[9]]);
        let product = u16::from_le_bytes([d[10], d[11]]);
        let max_packet = d[7] as u16;
        let address = self.next_address;
        self.next_address = self.next_address.saturating_add(1);
        self.control_no_data(old_address, 0, USB_REQ_SET_ADDRESS, address as u16, 0)
            .ok()?;
        let end = crate::arch::x86_64::timer_ticks().saturating_add(2);
        while crate::arch::x86_64::timer_ticks() < end {
            core::hint::spin_loop();
        }
        let mut cfg9 = [0u8; 9];
        self.control_in(address, 0x80, USB_REQ_GET_DESCRIPTOR, 0x0200, 0, &mut cfg9)
            .ok()?;
        let total = u16::from_le_bytes([cfg9[2], cfg9[3]]) as usize;
        let len = total.clamp(9, 512);
        let mut cfg = [0u8; 512];
        self.control_in(
            address,
            0x80,
            USB_REQ_GET_DESCRIPTOR,
            0x0200,
            0,
            &mut cfg[..len],
        )
        .ok()?;
        let configuration = cfg[5];
        self.control_no_data(
            address,
            0,
            USB_REQ_SET_CONFIGURATION,
            configuration as u16,
            0,
        )
        .ok()?;
        let (bulk_in, bulk_out, bulk_packet) = parse_bulk_endpoints(&cfg[..len]);
        writeln!(
            out,
            "USB device        : addr={} {:04X}:{:04X} EPin={:02X} EPout={:02X}/{:02X}",
            address, vendor, product, bulk_in, bulk_out[0], bulk_out[1]
        )
        .ok();
        Some(UsbDevice {
            address,
            vendor,
            product,
            max_packet,
            configuration,
            bulk_in,
            bulk_out,
            bulk_packet,
        })
    }

    pub fn control_in(
        &mut self,
        address: u8,
        request_type: u8,
        request: u8,
        value: u16,
        index: u16,
        data: &mut [u8],
    ) -> Result<(), ()> {
        self.control_transfer(
            address,
            request_type,
            request,
            value,
            index,
            Some(data),
            true,
        )
    }

    pub fn control_out(
        &mut self,
        address: u8,
        request_type: u8,
        request: u8,
        value: u16,
        index: u16,
        data: &[u8],
    ) -> Result<(), ()> {
        if data.len() > PAGE - DATA_BUF_OFF {
            return Err(());
        }
        unsafe {
            copy_nonoverlapping(data.as_ptr(), self.page_virt.add(DATA_BUF_OFF), data.len());
        }
        let slice = unsafe {
            core::slice::from_raw_parts_mut(self.page_virt.add(DATA_BUF_OFF), data.len())
        };
        self.control_transfer(
            address,
            request_type,
            request,
            value,
            index,
            Some(slice),
            false,
        )
    }

    pub fn control_no_data(
        &mut self,
        address: u8,
        request_type: u8,
        request: u8,
        value: u16,
        index: u16,
    ) -> Result<(), ()> {
        self.control_transfer(address, request_type, request, value, index, None, false)
    }

    fn control_transfer(
        &mut self,
        address: u8,
        request_type: u8,
        request: u8,
        value: u16,
        index: u16,
        data: Option<&mut [u8]>,
        in_dir: bool,
    ) -> Result<(), ()> {
        if data
            .as_ref()
            .map_or(false, |d| d.len() > PAGE - DATA_BUF_OFF)
        {
            return Err(());
        }
        self.setup_qh_endpoint(address, 0, 64);
        unsafe {
            let s = self.page_virt.add(SETUP_BUF_OFF);
            write_volatile(s, request_type);
            write_volatile(s.add(1), request);
            write_u16(s.add(2), value);
            write_u16(s.add(4), index);
            write_u16(s.add(6), data.as_ref().map_or(0, |d| d.len() as u16));
            if let Some(d) = data.as_ref() {
                if !in_dir {
                    copy_nonoverlapping(d.as_ptr(), self.page_virt.add(DATA_BUF_OFF), d.len());
                }
            }
        }
        self.clear_qtd(QTD_SETUP_OFF);
        self.clear_qtd(QTD_DATA_OFF);
        self.clear_qtd(QTD_STATUS_OFF);
        let q1 = self.qtd_phys(QTD_SETUP_OFF);
        let q2 = self.qtd_phys(QTD_DATA_OFF);
        let q3 = self.qtd_phys(QTD_STATUS_OFF);
        self.set_qtd(
            QTD_SETUP_OFF,
            q2,
            QTD_PID_SETUP,
            8,
            0,
            self.page_phys + SETUP_BUF_OFF as u64,
        );
        let has_data = data.as_ref().map_or(false, |d| !d.is_empty());
        if has_data {
            let pid = if in_dir { QTD_PID_IN } else { QTD_PID_OUT };
            self.set_qtd(
                QTD_DATA_OFF,
                q3,
                pid,
                data.as_ref().unwrap().len(),
                QTD_TOGGLE,
                self.page_phys + DATA_BUF_OFF as u64,
            );
        }
        let status_pid = if has_data {
            if in_dir { QTD_PID_OUT } else { QTD_PID_IN }
        } else {
            QTD_PID_IN
        };
        self.set_qtd(
            QTD_STATUS_OFF,
            LINK_TERMINATE as u64,
            status_pid,
            0,
            QTD_TOGGLE | QTD_IOC,
            0,
        );
        unsafe {
            write_u32(self.page_virt.add(QH_OFF + 16), q1 as u32);
            write_u32(self.page_virt.add(QH_OFF + 20), LINK_TERMINATE);
            write_u32(self.page_virt.add(QH_OFF + 24), 0);
        }
        fence(Ordering::SeqCst);
        self.wait_qtd(QTD_STATUS_OFF)?;
        if let Some(d) = data {
            if in_dir && !d.is_empty() {
                unsafe {
                    copy_nonoverlapping(self.page_virt.add(DATA_BUF_OFF), d.as_mut_ptr(), d.len());
                }
            }
        }
        Ok(())
    }

    fn wait_qtd(&mut self, off: usize) -> Result<(), ()> {
        for _ in 0..2_000_000 {
            let token = unsafe { read_volatile(self.page_virt.add(off + 8) as *const u32) };
            if token & QTD_ACTIVE == 0 {
                if token & (QTD_HALTED | QTD_BUFERR | QTD_BABBLE | QTD_XACTERR | QTD_MISSED) != 0 {
                    return Err(());
                }
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(())
    }

    fn setup_qh_endpoint(&mut self, address: u8, endpoint: u8, max_packet: u16) {
        unsafe {
            write_u32(self.page_virt.add(QH_OFF), (self.qh_phys as u32) | LINK_QH);
            write_u32(
                self.page_virt.add(QH_OFF + 4),
                address as u32
                    | ((endpoint as u32 & 7) << 8)
                    | (2 << 12)
                    | (1 << 14)
                    | ((max_packet as u32 & 0x7FF) << 16),
            );
            write_u32(self.page_virt.add(QH_OFF + 8), 0);
            write_u32(self.page_virt.add(QH_OFF + 12), 0);
        }
    }

    fn clear_qtd(&mut self, off: usize) {
        unsafe {
            write_bytes(self.page_virt.add(off), 0, 64);
        }
    }

    fn set_qtd(&mut self, off: usize, next: u64, pid: u32, len: usize, flags: u32, buffer: u64) {
        let token = pid | (((len as u32) & 0x7FFF) << 16) | QTD_ACTIVE | flags;
        unsafe {
            write_u32(self.page_virt.add(off), next as u32);
            write_u32(self.page_virt.add(off + 4), LINK_TERMINATE);
            write_u32(self.page_virt.add(off + 8), token);
            if buffer != 0 {
                write_u32(self.page_virt.add(off + 16), buffer as u32);
            }
        }
    }

    fn qtd_phys(&self, off: usize) -> u64 {
        self.page_phys + off as u64
    }

    #[inline(always)]
    fn reg32(&self, off: usize) -> u32 {
        unsafe { read_volatile(self.op.add(off) as *const u32) }
    }

    #[inline(always)]
    fn reg_write32(&self, off: usize, value: u32) {
        unsafe {
            write_volatile(self.op.add(off) as *mut u32, value);
        }
    }
}

fn parse_bulk_endpoints(config: &[u8]) -> (u8, [u8; 2], u16) {
    let mut i = 0usize;
    let mut bulk_in = 0u8;
    let mut bulk_out = [0u8; 2];
    let mut count = 0usize;
    let mut packet = 0u16;
    while i + 2 <= config.len() {
        let len = config[i] as usize;
        if len < 2 || i + len > config.len() {
            break;
        }
        if config[i + 1] == 5 && len >= 7 && config[i + 3] & 3 == 2 {
            let ep = config[i + 2];
            let mps = u16::from_le_bytes([config[i + 4], config[i + 5]]) & 0x07FF;
            packet = packet.max(mps);
            if ep & 0x80 != 0 {
                bulk_in = ep;
            } else if count < 2 {
                bulk_out[count] = ep;
                count += 1;
            }
        }
        i += len;
    }
    (bulk_in, bulk_out, packet)
}

#[inline(always)]
fn read_mmio8(base: u64, off: usize) -> u8 {
    unsafe { read_volatile((base + off as u64) as *const u8) }
}

#[inline(always)]
fn read_mmio32(base: u64, off: usize) -> u32 {
    unsafe { read_volatile((base + off as u64) as *const u32) }
}

#[inline(always)]
fn write_u16(ptr: *mut u8, value: u16) {
    unsafe {
        write_volatile(ptr as *mut u16, value.to_le());
    }
}

#[inline(always)]
fn write_u32(ptr: *mut u8, value: u32) {
    unsafe {
        write_volatile(ptr as *mut u32, value);
    }
}

#[inline(always)]
unsafe fn out32(port: u16, value: u32) {
    core::arch::asm!("out dx, eax", in("dx") port, in("eax") value, options(nostack, preserves_flags));
}

#[inline(always)]
unsafe fn in32(port: u16) -> u32 {
    let value: u32;
    core::arch::asm!("in eax, dx", in("dx") port, lateout("eax") value, options(nostack, preserves_flags));
    value
}
