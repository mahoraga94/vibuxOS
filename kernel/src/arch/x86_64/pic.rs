use core::arch::asm;

const PIC1_COMMAND: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_COMMAND: u16 = 0xa0;
const PIC2_DATA: u16 = 0xa1;
const PIC_EOI: u8 = 0x20;

#[inline(always)]
unsafe fn outb(port: u16, value: u8) {
    asm!("out dx, al", in("dx") port, in("al") value, options(nostack, preserves_flags));
}

#[inline(always)]
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    asm!("in al, dx", in("dx") port, lateout("al") value, options(nostack, preserves_flags));
    value
}

#[inline(always)]
unsafe fn io_wait() {
    outb(0x80, 0);
}

pub fn init() {
    unsafe {
        let master_mask = inb(PIC1_DATA);
        let slave_mask = inb(PIC2_DATA);

        outb(PIC1_COMMAND, 0x11);
        io_wait();
        outb(PIC2_COMMAND, 0x11);
        io_wait();

        outb(PIC1_DATA, 32);
        io_wait();
        outb(PIC2_DATA, 40);
        io_wait();

        outb(PIC1_DATA, 4);
        io_wait();
        outb(PIC2_DATA, 2);
        io_wait();

        outb(PIC1_DATA, 0x01);
        io_wait();
        outb(PIC2_DATA, 0x01);
        io_wait();

        let _ = master_mask;
        let _ = slave_mask;

        outb(PIC1_DATA, 0xfc);
        outb(PIC2_DATA, 0xff);
    }
}

pub fn end_of_interrupt(vector: u8) {
    unsafe {
        if vector >= 40 {
            outb(PIC2_COMMAND, PIC_EOI);
        }
        if vector >= 32 && vector < 48 {
            outb(PIC1_COMMAND, PIC_EOI);
        }
    }
}

pub fn mask_all() {
    unsafe {
        outb(PIC1_DATA, 0xff);
        outb(PIC2_DATA, 0xff);
    }
}

pub fn masks() -> (u8, u8) {
    unsafe { (inb(PIC1_DATA), inb(PIC2_DATA)) }
}
