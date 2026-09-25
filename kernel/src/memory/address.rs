pub const PAGE_SIZE: u64 = 4096;
pub const LARGE_PAGE_SIZE: u64 = 2 * 1024 * 1024;
pub const HUGE_PAGE_SIZE: u64 = 1024 * 1024 * 1024;

pub const MIB: u64 = 1024 * 1024;
pub const GIB: u64 = 1024 * MIB;

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysAddr(pub u64);

impl PhysAddr {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }

    pub const fn align_down(self) -> Self {
        Self(self.0 & !(PAGE_SIZE - 1))
    }

    pub const fn is_aligned(self) -> bool {
        self.0 & (PAGE_SIZE - 1) == 0
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VirtAddr(pub u64);

impl VirtAddr {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }

    pub const fn page_offset(self) -> u64 {
        self.0 & (PAGE_SIZE - 1)
    }

    pub const fn align_down(self) -> Self {
        Self(self.0 & !(PAGE_SIZE - 1))
    }
}

#[inline(always)]
pub const fn align_up(value: u64) -> Option<u64> {
    match value.checked_add(PAGE_SIZE - 1) {
        Some(value) => Some(value & !(PAGE_SIZE - 1)),
        None => None,
    }
}

#[inline(always)]
pub const fn frame_count(start: u64, end: u64) -> u64 {
    let aligned_start = match align_up(start) {
        Some(value) => value,
        None => return 0,
    };

    if aligned_start >= end {
        return 0;
    }

    (end - aligned_start) / PAGE_SIZE
}
