pub mod apic;
pub mod cpu;
pub mod exceptions;
pub mod gdt;
pub mod idt;
pub mod paging;
pub mod pic;
pub mod pit;
pub mod power;
pub mod syscall;

use bootloader_api::BootInfo;
pub use cpu::CpuInfo;
pub fn init(boot_info: &mut BootInfo) {
    gdt::init();
    idt::init();
    pic::init();
    pit::init(250);
    syscall::init();
    paging::enable_protection();
    if let Some(offset) = boot_info.physical_memory_offset.into_option() {
        paging::install_physmap(offset);
    }
    apic::detect();
    idt::enable();
}
pub fn timer_ticks() -> u64 {
    idt::timer_ticks()
}
pub fn last_keyboard_scancode() -> u8 {
    idt::last_keyboard_scancode()
}
pub fn pop_keyboard_scancode() -> Option<u8> {
    idt::pop_keyboard_scancode()
}
pub fn keyboard_has_scancode() -> bool {
    idt::keyboard_has_scancode()
}
pub fn page_fault_address() -> u64 {
    idt::page_fault_address()
}
pub fn page_fault_error() -> u64 {
    idt::page_fault_error()
}
pub fn apic_supported() -> bool {
    apic::supported()
}
pub fn x2apic_supported() -> bool {
    apic::x2apic_supported()
}
pub fn paging_report(physical_memory_offset: Option<u64>) -> paging::PagingReport {
    paging::report(physical_memory_offset)
}
pub use syscall::enter_user;
