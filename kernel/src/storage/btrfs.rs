use super::checksum::crc32c;
use core::fmt::Write;

pub const SUPERBLOCK_OFFSET: u64 = 0x10_000;
pub const SUPERBLOCK_SIZE: usize = 4096;

const MAGIC: &[u8; 8] = b"_BHRfS_M";
const LABEL_OFFSET: usize = 0x12B;
const LABEL_BYTES: usize = 256;

#[derive(Clone, Copy)]
pub struct BtrfsInfo {
    pub checksum_valid: bool,
    pub checksum_type: u16,
    pub generation: u64,
    pub root: u64,
    pub chunk_root: u64,
    pub total_bytes: u64,
    pub bytes_used: u64,
    pub root_dir_objectid: u64,
    pub num_devices: u64,
    pub sectorsize: u32,
    pub nodesize: u32,
    pub leafsize: u32,
    pub stripesize: u32,
    pub compat_flags: u64,
    pub compat_ro_flags: u64,
    pub incompat_flags: u64,
    pub root_level: u8,
    pub chunk_root_level: u8,
    pub log_root_level: u8,
    pub label: [u8; LABEL_BYTES],
    pub label_len: usize,
}

pub fn parse(block: &[u8]) -> Option<BtrfsInfo> {
    if block.len() < SUPERBLOCK_SIZE {
        return None;
    }
    if &block[0x40..0x48] != MAGIC {
        return None;
    }

    let checksum_type = le16(block, 0xC4);
    let checksum_valid = matches!(checksum_type, 0) && {
        let expected = le32(block, 0x00);
        let actual = crc32c(&block[0x20..0x1000]);
        expected == actual
    };

    let mut label = [0u8; LABEL_BYTES];
    label.copy_from_slice(&block[LABEL_OFFSET..LABEL_OFFSET + LABEL_BYTES]);
    let mut label_len = LABEL_BYTES;
    while label_len > 0 && (label[label_len - 1] == 0 || label[label_len - 1] == b' ') {
        label_len -= 1;
    }

    Some(BtrfsInfo {
        checksum_valid,
        checksum_type,
        generation: le64(block, 0x48),
        root: le64(block, 0x50),
        chunk_root: le64(block, 0x58),
        total_bytes: le64(block, 0x70),
        bytes_used: le64(block, 0x78),
        root_dir_objectid: le64(block, 0x80),
        num_devices: le64(block, 0x88),
        sectorsize: le32(block, 0x90),
        nodesize: le32(block, 0x94),
        leafsize: le32(block, 0x98),
        stripesize: le32(block, 0x9C),
        compat_flags: le64(block, 0xAC),
        compat_ro_flags: le64(block, 0xB4),
        incompat_flags: le64(block, 0xBC),
        root_level: block[0xC6],
        chunk_root_level: block[0xC7],
        log_root_level: block[0xC8],
        label,
        label_len,
    })
}

pub fn report<W: Write>(info: &BtrfsInfo, out: &mut W) {
    writeln!(out, "BTRFS").ok();
    writeln!(out, "--------------------------------").ok();
    writeln!(
        out,
        "checksum           : {}",
        if info.checksum_valid {
            "CRC32C OK"
        } else {
            "INVALID"
        }
    )
    .ok();
    writeln!(out, "generation         : {}", info.generation).ok();
    writeln!(out, "devices            : {}", info.num_devices).ok();
    writeln!(out, "sector size        : {}", info.sectorsize).ok();
    writeln!(out, "node size          : {}", info.nodesize).ok();
    writeln!(
        out,
        "total              : {} MiB",
        info.total_bytes / 1024 / 1024
    )
    .ok();
    writeln!(
        out,
        "used               : {} MiB",
        info.bytes_used / 1024 / 1024
    )
    .ok();
    writeln!(out, "root               : 0x{:016X}", info.root).ok();
    writeln!(out, "chunk root         : 0x{:016X}", info.chunk_root).ok();
    write!(out, "label              : ").ok();
    for &byte in &info.label[..info.label_len] {
        if (0x20..0x7F).contains(&byte) {
            out.write_char(byte as char).ok();
        }
    }
    writeln!(out).ok();
}

#[inline(always)]
fn le16(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

#[inline(always)]
fn le32(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

#[inline(always)]
fn le64(data: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
        data[offset + 4],
        data[offset + 5],
        data[offset + 6],
        data[offset + 7],
    ])
}
