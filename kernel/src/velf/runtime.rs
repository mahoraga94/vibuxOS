use crate::memory::FrameAllocator;

pub unsafe fn install(_allocator: &mut FrameAllocator<'_>, _physical_offset: u64) {}

pub fn initialized() -> bool {
    true
}

pub fn with_allocator<R, F: FnOnce(&mut FrameAllocator<'_>, u64) -> R>(_function: F) -> R {
    panic!("VELF runtime allocator bridge requires process-owned allocator")
}
