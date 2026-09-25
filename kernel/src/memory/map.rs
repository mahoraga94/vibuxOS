use bootloader_api::info::{MemoryRegion, MemoryRegionKind};
use core::fmt::Write;

use super::address::{GIB, MIB, frame_count};

const DMA_LIMIT: u64 = 16 * MIB;
const DMA32_LIMIT: u64 = 4 * GIB;

#[derive(Clone, Copy)]
pub struct MemoryMapReport {
    pub region_count: usize,
    pub reported_bytes: u64,
    pub usable_bytes: u64,
    pub reserved_bytes: u64,
    pub highest_physical_address: u64,
    pub usable_frames: u64,
    pub dma_bytes: u64,
    pub dma32_bytes: u64,
    pub normal_bytes: u64,
}

impl MemoryMapReport {
    pub fn from_regions(regions: &[MemoryRegion]) -> Self {
        let mut r = Self {
            region_count: 0,
            reported_bytes: 0,
            usable_bytes: 0,
            reserved_bytes: 0,
            highest_physical_address: 0,
            usable_frames: 0,
            dma_bytes: 0,
            dma32_bytes: 0,
            normal_bytes: 0,
        };

        for region in regions {
            let length = region.end.saturating_sub(region.start);

            r.region_count += 1;
            r.reported_bytes = r.reported_bytes.saturating_add(length);

            r.highest_physical_address = r.highest_physical_address.max(region.end);

            match region.kind {
                MemoryRegionKind::Usable => {
                    r.usable_bytes = r.usable_bytes.saturating_add(length);

                    r.usable_frames = r
                        .usable_frames
                        .saturating_add(frame_count(region.start, region.end));

                    r.dma_bytes = r.dma_bytes.saturating_add(intersection(
                        region.start,
                        region.end,
                        0,
                        DMA_LIMIT,
                    ));

                    r.dma32_bytes = r.dma32_bytes.saturating_add(intersection(
                        region.start,
                        region.end,
                        DMA_LIMIT,
                        DMA32_LIMIT,
                    ));

                    r.normal_bytes = r.normal_bytes.saturating_add(intersection(
                        region.start,
                        region.end,
                        DMA32_LIMIT,
                        u64::MAX,
                    ));
                }

                _ => {
                    r.reserved_bytes = r.reserved_bytes.saturating_add(length);
                }
            }
        }

        r
    }

    pub fn report<W: Write>(&self, out: &mut W) {
        writeln!(out).ok();
        writeln!(out, "PHYSICAL MEMORY MAP").ok();
        writeln!(out, "----------------------------------------").ok();

        writeln!(out, "regions           : {}", self.region_count).ok();

        writeln!(
            out,
            "map high-water    : {} MiB",
            self.highest_physical_address / MIB
        )
        .ok();

        writeln!(out, "reported          : {} MiB", self.reported_bytes / MIB).ok();

        writeln!(out, "usable            : {} MiB", self.usable_bytes / MIB).ok();

        writeln!(out, "reserved          : {} MiB", self.reserved_bytes / MIB).ok();

        writeln!(out, "usable 4KiB frames: {}", self.usable_frames).ok();
    }

    pub fn dump_regions<W: Write>(&self, regions: &[MemoryRegion], out: &mut W) {
        for (index, region) in regions.iter().enumerate() {
            write!(out, "MEM[{index:02}] {:<12}", kind_name(region.kind),).ok();

            write!(out, "  0x{:016X}-0x{:016X}", region.start, region.end,).ok();

            writeln!(
                out,
                "  {:>8} KiB",
                region.end.saturating_sub(region.start) / 1024,
            )
            .ok();
        }
    }
}

fn kind_name(kind: MemoryRegionKind) -> &'static str {
    match kind {
        MemoryRegionKind::Usable => "USABLE",

        MemoryRegionKind::Bootloader => "BOOTLOADER",

        MemoryRegionKind::UnknownUefi(_) => "UEFI-UNKNOWN",

        MemoryRegionKind::UnknownBios(_) => "BIOS-UNKNOWN",

        _ => "OTHER",
    }
}

#[inline(always)]
fn intersection(start: u64, end: u64, low: u64, high: u64) -> u64 {
    end.min(high).saturating_sub(start.max(low))
}
