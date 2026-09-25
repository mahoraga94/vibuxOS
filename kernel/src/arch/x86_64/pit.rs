use core::arch::asm;

const PIT_FREQUENCY: u32 = 1_193_182;

#[inline(always)]
unsafe fn outb(port: u16, value: u8) {
    asm!("out dx, al", in("dx") port, in("al") value, options(nostack, preserves_flags));
}

pub fn init(hz: u32) {
    let hz = hz.clamp(20, 1000);
    let divisor = (PIT_FREQUENCY / hz).clamp(1, u16::MAX as u32) as u16;
    unsafe {
        outb(0x43, 0x34);
        outb(0x40, divisor as u8);
        outb(0x40, (divisor >> 8) as u8);
    }
}
