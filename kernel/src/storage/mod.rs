pub mod btrfs;
pub mod checksum;
pub mod nvme;
pub mod pci;
pub mod vfs;

use crate::console::Serial;
use crate::memory::FrameAllocator;
use core::fmt::Write;

pub fn probe(
    allocator: &mut FrameAllocator<'_>,
    physical_memory_offset: u64,
    out: &mut Serial,
) -> Option<nvme::StorageReport> {
    writeln!(out).ok();
    writeln!(out, "STORAGE PROBE").ok();
    writeln!(out, "--------------------------------").ok();

    let devices = pci::find_all_nvme();
    let mut first = None;
    let mut idx = 0;

    for dev_opt in devices {
        if let Some(pci_dev) = dev_opt {
            if idx == 0 {
                if let Some(report) =
                    nvme::Nvme::probe(pci_dev, physical_memory_offset, allocator, out)
                {
                    first = Some(report);
                    idx += 1;
                }
            } else {
                if let Some((report, nvme_dev)) =
                    nvme::Nvme::probe_secondary(pci_dev, physical_memory_offset, allocator, out)
                {
                    let name: &[u8] = match idx {
                        1 => b"nvme1",
                        2 => b"nvme2",
                        3 => b"nvme3",
                        _ => b"disk",
                    };
                    let _ = vfs::mount_readonly(
                        nvme_dev,
                        report.namespace_bytes,
                        report.lba_bytes,
                        name,
                        out,
                    );
                    idx += 1;
                }
            }
        }
    }

    if first.is_none() {
        writeln!(out, "NVMe : NOT FOUND").ok();
        writeln!(out, "storage : OFFLINE").ok();
        return None;
    }

    if vfs::mounted() {
        let _ = vfs::make_dir(b"/mnt", vfs::root_inode(), 0, 0);
    }

    first
}
