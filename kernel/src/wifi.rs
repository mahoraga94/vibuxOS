use crate::console::Serial;
use crate::memory::FrameAllocator;
use crate::usb::UsbBus;
use core::fmt::Write;
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};

const VID: u16 = 0x0BDA;
const PID: u16 = 0xF179;
const USB_READ: u8 = 0xC0;
const USB_WRITE: u8 = 0x40;
const USB_CMD_REQ: u8 = 0x05;
const REG_MACID: u16 = 0x0610;
const REG_SYS_FUNC: u16 = 0x0002;
const REG_SYS_CLK: u16 = 0x0008;
const REG_CR: u16 = 0x0100;
const REG_TRXDMA_CTRL: u16 = 0x010C;
const REG_TXPAUSE: u16 = 0x0522;
const FW_MAX: usize = 256 * 1024;

static READY: AtomicBool = AtomicBool::new(false);
static USB_ADDR: AtomicU8 = AtomicU8::new(0);
static BULK_IN: AtomicU8 = AtomicU8::new(0);
static BULK_OUT0: AtomicU8 = AtomicU8::new(0);
static BULK_OUT1: AtomicU8 = AtomicU8::new(0);
static BULK_MPS: AtomicUsize = AtomicUsize::new(0);
static FW_LEN: AtomicUsize = AtomicUsize::new(0);
static mut MAC: [u8; 6] = [0; 6];

static FIRMWARE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/rtl8188fufw.bin"));

pub fn init(usb: &mut UsbBus, _allocator: &mut FrameAllocator<'_>, out: &mut Serial) -> bool {
    let Some(device) = usb.wifi else {
        writeln!(out, "WIFI RTL8188FTV   : NOT FOUND").ok();
        return false;
    };

    if device.vendor != VID || device.product != PID {
        return false;
    }

    if device.bulk_in == 0 || device.bulk_out[0] == 0 || device.bulk_packet == 0 {
        writeln!(out, "WIFI USB          : BULK ENDPOINTS INVALID").ok();
        return false;
    }

    if FIRMWARE.is_empty() || FIRMWARE.len() > FW_MAX {
        writeln!(
            out,
            "WIFI FIRMWARE     : INVALID ({} bytes)",
            FIRMWARE.len()
        )
        .ok();
        return false;
    }

    writeln!(
        out,
        "WIFI RTL8188FTV   : FOUND {:04X}:{:04X} addr={}",
        device.vendor, device.product, device.address
    )
    .ok();
    writeln!(
        out,
        "WIFI USB EPs      : IN={:02X} OUT={:02X}/{:02X} MPS={}",
        device.bulk_in, device.bulk_out[0], device.bulk_out[1], device.bulk_packet
    )
    .ok();

    let mut mac = [0u8; 6];
    for i in 0..6 {
        let mut value = [0u8; 1];
        if usb
            .control_in(
                device.address,
                USB_READ,
                USB_CMD_REQ,
                REG_MACID + i as u16,
                0,
                &mut value,
            )
            .is_err()
        {
            writeln!(
                out,
                "WIFI REG I/O      : FAILED @ 0x{:04X}",
                REG_MACID + i as u16
            )
            .ok();
            return false;
        }
        mac[i] = value[0];
    }

    if mac == [0; 6] || mac == [0xFF; 6] {
        writeln!(out, "WIFI MAC          : INVALID").ok();
        return false;
    }

    let sys_func = match read32(usb, device.address, REG_SYS_FUNC) {
        Some(v) => v,
        None => {
            writeln!(out, "WIFI CHIP READ     : FAILED").ok();
            return false;
        }
    };
    let sys_clk = read32(usb, device.address, REG_SYS_CLK).unwrap_or(0);
    let cr = read8(usb, device.address, REG_CR).unwrap_or(0);
    let trxdma = read8(usb, device.address, REG_TRXDMA_CTRL).unwrap_or(0);
    let txpause = read8(usb, device.address, REG_TXPAUSE).unwrap_or(0);

    writeln!(
        out,
        "WIFI CHIP REGS    : SYS_FUNC={:08X} SYS_CLK={:08X} CR={:02X}",
        sys_func, sys_clk, cr
    )
    .ok();
    writeln!(
        out,
        "WIFI DMA REGS     : TRXDMA={:02X} TXPAUSE={:02X}",
        trxdma, txpause
    )
    .ok();

    unsafe {
        MAC = mac;
    }
    USB_ADDR.store(device.address, Ordering::Release);
    BULK_IN.store(device.bulk_in, Ordering::Release);
    BULK_OUT0.store(device.bulk_out[0], Ordering::Release);
    BULK_OUT1.store(device.bulk_out[1], Ordering::Release);
    BULK_MPS.store(device.bulk_packet as usize, Ordering::Release);
    FW_LEN.store(FIRMWARE.len(), Ordering::Release);
    READY.store(true, Ordering::Release);

    writeln!(
        out,
        "WIFI MAC          : {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    )
    .ok();
    writeln!(out, "WIFI FIRMWARE     : EMBEDDED {} bytes", FIRMWARE.len()).ok();
    writeln!(out, "WIFI TRANSPORT    : EHCI + RTL8188F CONTROL ONLINE").ok();
    writeln!(out, "WIFI BACKEND      : SELECTED").ok();
    true
}

pub fn ready() -> bool {
    READY.load(Ordering::Acquire)
}

pub fn mac() -> Option<[u8; 6]> {
    if !ready() {
        return None;
    }
    unsafe { Some(MAC) }
}

pub fn status<W: Write>(out: &mut W) {
    if !ready() {
        writeln!(out, "device=RTL8188FTV offline").ok();
        return;
    }

    let mac = unsafe { MAC };
    writeln!(out, "device=RTL8188FTV").ok();
    writeln!(out, "usb={:04X}:{:04X}", VID, PID).ok();
    writeln!(out, "address={}", USB_ADDR.load(Ordering::Acquire)).ok();
    writeln!(
        out,
        "mac={:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    )
    .ok();
    writeln!(out, "bulk_in=0x{:02X}", BULK_IN.load(Ordering::Acquire)).ok();
    writeln!(
        out,
        "bulk_out=0x{:02X},0x{:02X}",
        BULK_OUT0.load(Ordering::Acquire),
        BULK_OUT1.load(Ordering::Acquire)
    )
    .ok();
    writeln!(out, "bulk_mps={}", BULK_MPS.load(Ordering::Acquire)).ok();
    writeln!(out, "firmware_bytes={}", FW_LEN.load(Ordering::Acquire)).ok();
    writeln!(out, "radio=RTL8188F 2.4GHz 1T1R").ok();
    writeln!(out, "wireless_state=transport_initialized").ok();
}

pub fn ping<W: Write>(_text: &[u8], out: &mut W) {
    if ready() {
        writeln!(
            out,
            "ping: RTL8188FTV transport is online; 802.11 station/ICMP path is not initialized yet"
        )
        .ok();
    } else {
        writeln!(out, "network: offline").ok();
    }
}

fn read8(usb: &mut UsbBus, address: u8, reg: u16) -> Option<u8> {
    let mut data = [0u8; 1];
    usb.control_in(address, USB_READ, USB_CMD_REQ, reg, 0, &mut data)
        .ok()?;
    Some(data[0])
}

fn read32(usb: &mut UsbBus, address: u8, reg: u16) -> Option<u32> {
    let mut data = [0u8; 4];
    usb.control_in(address, USB_READ, USB_CMD_REQ, reg, 0, &mut data)
        .ok()?;
    Some(u32::from_le_bytes(data))
}

#[allow(dead_code)]
fn write8(usb: &mut UsbBus, address: u8, reg: u16, value: u8) -> bool {
    usb.control_out(address, USB_WRITE, USB_CMD_REQ, reg, 0, &[value])
        .is_ok()
}

#[allow(dead_code)]
fn write32(usb: &mut UsbBus, address: u8, reg: u16, value: u32) -> bool {
    usb.control_out(
        address,
        USB_WRITE,
        USB_CMD_REQ,
        reg,
        0,
        &value.to_le_bytes(),
    )
    .is_ok()
}
