use core::arch::asm;
pub fn reboot() -> ! {
    unsafe {
        asm!("cli");
    }
    unsafe {
        asm!("out dx, al", in("dx") 0x64u16, in("al") 0xFEu8);
        asm!("out dx, al", in("dx") 0xCF9u16, in("al") 0x02u8);
        asm!("out dx, al", in("dx") 0xCF9u16, in("al") 0x06u8);
        loop {
            asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}
pub fn shutdown() -> ! {
    unsafe {
        asm!("cli");
    }
    unsafe {
        asm!("out dx, ax", in("dx") 0x604u16, in("ax") 0x2000u16);
        asm!("out dx, ax", in("dx") 0xB004u16, in("ax") 0x2000u16);
        loop {
            asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}
