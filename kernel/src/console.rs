use core::arch::asm;
use core::fmt::{self, Write};

const COM1: u16 = 0x3F8;

pub struct Serial {
    initialized: bool,
}

impl Serial {
    pub const fn new() -> Self {
        Self { initialized: false }
    }

    pub fn init(&mut self) {
        unsafe {
            outb(COM1 + 1, 0x00);
            outb(COM1 + 3, 0x80);
            outb(COM1, 0x01);
            outb(COM1 + 1, 0x00);
            outb(COM1 + 3, 0x03);
            outb(COM1 + 2, 0xC7);
            outb(COM1 + 4, 0x0B);
        }

        self.initialized = true;
    }

    pub fn write_byte_public(&mut self, byte: u8) -> fmt::Result {
        if !self.initialized {
            self.init();
        }

        unsafe {
            outb(COM1, byte);
        }

        Ok(())
    }

    fn write_byte(&mut self, byte: u8) {
        let _ = self.write_byte_public(byte);
    }
}

impl Write for Serial {
    fn write_str(&mut self, string: &str) -> fmt::Result {
        for byte in string.bytes() {
            if byte == b'\n' {
                self.write_byte(b'\r');
            }

            self.write_byte(byte);
        }

        Ok(())
    }
}

#[inline(always)]
pub fn serial_read_byte() -> Option<u8> {
    unsafe {
        if inb(COM1 + 5) & 0x01 != 0 {
            Some(inb(COM1))
        } else {
            None
        }
    }
}

#[inline(always)]
pub fn serial_has_data() -> bool {
    unsafe { inb(COM1 + 5) & 0x01 != 0 }
}

#[inline(always)]
pub fn serial_write_byte(byte: u8) {
    unsafe {
        outb(COM1, byte);
    }
}

#[inline(always)]
unsafe fn inb(port: u16) -> u8 {
    let value: u8;

    asm!(
        "in al, dx",
        out("al") value,
        in("dx") port,
        options(
            nostack,
            preserves_flags
        )
    );

    value
}

#[inline(always)]
unsafe fn outb(port: u16, value: u8) {
    unsafe {
        asm!(
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(
                nostack,
                preserves_flags
            )
        );
    }
}
