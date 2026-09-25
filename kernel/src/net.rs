use crate::console::Serial;
use crate::memory::FrameAllocator;
use core::fmt::Write;

#[path = "net_rtl8139.rs"]
mod rtl8139;

use crate::{usb, wifi};

pub fn init(allocator: &mut FrameAllocator<'_>, physical_offset: u64, out: &mut Serial) -> bool {
    writeln!(out).ok();
    writeln!(out, "NETWORK PROBE").ok();
    writeln!(out, "--------------------------------").ok();

    match usb::UsbBus::new(allocator, physical_offset, out) {
        Some(mut bus) => {
            if wifi::init(&mut bus, allocator, out) {
                writeln!(out, "network backend    : RTL8188FTV").ok();
                return true;
            }
            writeln!(out, "RTL8188FTV        : not initialized").ok();
        }
        None => {
            writeln!(out, "USB EHCI          : unavailable").ok();
        }
    }

    writeln!(out, "network fallback   : RTL8139").ok();
    rtl8139::init(allocator, physical_offset, out)
}

pub fn status<W: Write>(out: &mut W) {
    if wifi::ready() {
        wifi::status(out);
    } else {
        rtl8139::status(out);
    }
}

pub fn ping<W: Write>(text: &[u8], out: &mut W) {
    if wifi::ready() {
        wifi::ping(text, out);
    } else {
        rtl8139::ping(text, out);
    }
}
