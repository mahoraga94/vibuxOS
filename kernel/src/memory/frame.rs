use bootloader_api::info::{MemoryRegion, MemoryRegionKind};

use super::address::{PAGE_SIZE, align_up};

const RESERVED_LOW_MEMORY: u64 = 1024 * 1024;

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PhysFrame(u64);

impl PhysFrame {
    #[inline(always)]
    pub const fn addr(self) -> u64 {
        self.0
    }
}

pub struct FrameAllocator<'a> {
    regions: &'a [MemoryRegion],
    region_index: usize,
    next_frame: u64,
}

impl<'a> FrameAllocator<'a> {
    #[inline(always)]
    pub fn new(regions: &'a [MemoryRegion]) -> Self {
        Self {
            regions,
            region_index: 0,
            next_frame: 0,
        }
    }

    #[inline(always)]
    pub fn allocate(&mut self) -> Option<PhysFrame> {
        loop {
            if self.region_index >= self.regions.len() {
                return None;
            }

            let region = self.regions[self.region_index];

            if region.kind != MemoryRegionKind::Usable {
                self.region_index += 1;
                self.next_frame = 0;
                continue;
            }

            let start = region.start.max(RESERVED_LOW_MEMORY);

            let start = align_up(start)?;

            if self.next_frame == 0 || self.next_frame < start {
                self.next_frame = start;
            }

            if self.next_frame.checked_add(PAGE_SIZE)? <= region.end {
                let frame = PhysFrame(self.next_frame);

                self.next_frame = self.next_frame.checked_add(PAGE_SIZE)?;

                return Some(frame);
            }

            self.region_index += 1;
            self.next_frame = 0;
        }
    }
}
