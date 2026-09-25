use crate::console::Serial;
use crate::memory::FrameAllocator;

static mut PROCESS_ALLOCATOR: Option<*mut FrameAllocator<'static>> = None;
static mut PROCESS_PHYSICAL_OFFSET: u64 = 0;

pub fn install_process_allocator(allocator: &mut FrameAllocator<'_>, physical_offset: u64) {
    unsafe {
        PROCESS_ALLOCATOR = Some(core::mem::transmute::<
            *mut FrameAllocator<'_>,
            *mut FrameAllocator<'static>,
        >(allocator as *mut FrameAllocator<'_>));
        PROCESS_PHYSICAL_OFFSET = physical_offset;
    }
}

pub fn process_physical_offset() -> u64 {
    unsafe { PROCESS_PHYSICAL_OFFSET }
}

pub unsafe fn current_frame_allocator() -> &'static mut FrameAllocator<'static> {
    unsafe {
        match PROCESS_ALLOCATOR {
            Some(ptr) => &mut *ptr,
            None => panic!("Vibux process allocator not installed"),
        }
    }
}

pub mod dynamic;
pub mod error;
pub mod format;
pub mod loader;
pub mod runner;
mod runtime;
pub(crate) mod store;

#[inline(always)]
pub fn bash_bytes() -> &'static [u8] {
    store::bash()
}

#[inline(always)]
pub fn sh_bytes() -> &'static [u8] {
    store::sh()
}

pub fn with_process_allocator<R, F>(function: F) -> Option<R>
where
    F: FnOnce(&mut FrameAllocator<'static>, u64) -> R,
{
    unsafe {
        match PROCESS_ALLOCATOR {
            Some(ptr) => Some(function(&mut *ptr, PROCESS_PHYSICAL_OFFSET)),
            None => None,
        }
    }
}

pub fn run_bash(allocator: &mut FrameAllocator<'_>, physical_offset: u64, serial: &mut Serial) {
    install_process_allocator(allocator, physical_offset);
    runner::run_bash(allocator, physical_offset, serial);
}

pub fn run_bash_again(serial: &mut Serial) -> i32 {
    unsafe {
        match PROCESS_ALLOCATOR {
            Some(ptr) => {
                runner::run_bash(&mut *ptr, PROCESS_PHYSICAL_OFFSET, serial);
            }
            None => return -1,
        }
    }
    crate::arch::x86_64::syscall::exit_code()
}
