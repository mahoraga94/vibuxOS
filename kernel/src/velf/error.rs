use core::fmt::Write;

#[derive(Clone, Copy, Debug)]
pub enum Error {
    Empty,
    TooSmall,
    BadMagic,
    NotElf64,
    NotLittleEndian,
    BadIdentVersion,
    UnsupportedMachine(u16),
    UnsupportedType(u16),
    BadHeader(&'static str),
    PhdrOutOfBounds,
    BadPhdrSize(u16),
    SegmentOutOfBounds,
    FileszGreaterThanMemsz,
    BadAlignment,
    AddressOverflow,
    NonCanonical,
    OutsideUserRange,
    TooManyLoadSegments,
    OutOfMemory,
    PageAlreadyMapped(u64),
    MappingFailed,
    InvalidInterpreter,
    InvalidDynamic,
    InvalidStringTable,
}

impl Error {
    pub fn report<W: Write>(&self, out: &mut W) {
        let _ = match *self {
            Self::Empty => writeln!(out, "VELF ERROR: ELF image is empty"),
            Self::TooSmall => writeln!(out, "VELF ERROR: image is smaller than ELF64 header"),
            Self::BadMagic => writeln!(out, "VELF ERROR: ELF magic is invalid"),
            Self::NotElf64 => writeln!(out, "VELF ERROR: only ELF64 is supported"),
            Self::NotLittleEndian => {
                writeln!(out, "VELF ERROR: only little-endian ELF is supported")
            }
            Self::BadIdentVersion => {
                writeln!(out, "VELF ERROR: ELF identification version is invalid")
            }
            Self::UnsupportedMachine(m) => {
                writeln!(out, "VELF ERROR: e_machine=0x{:04X}; expected x86_64", m)
            }
            Self::UnsupportedType(k) => writeln!(out, "VELF ERROR: ELF type {} is unsupported", k),
            Self::BadHeader(r) => writeln!(out, "VELF ERROR: {}", r),
            Self::PhdrOutOfBounds => {
                writeln!(out, "VELF ERROR: program-header table is outside the image")
            }
            Self::BadPhdrSize(s) => {
                writeln!(out, "VELF ERROR: e_phentsize={} but ELF64 requires 56", s)
            }
            Self::SegmentOutOfBounds => {
                writeln!(out, "VELF ERROR: PT_LOAD file range is outside the image")
            }
            Self::FileszGreaterThanMemsz => {
                writeln!(out, "VELF ERROR: p_filesz is greater than p_memsz")
            }
            Self::BadAlignment => writeln!(out, "VELF ERROR: PT_LOAD p_align is invalid"),
            Self::AddressOverflow => {
                writeln!(out, "VELF ERROR: ELF virtual-address arithmetic overflowed")
            }
            Self::NonCanonical => {
                writeln!(out, "VELF ERROR: ELF requested a non-canonical address")
            }
            Self::OutsideUserRange => writeln!(
                out,
                "VELF ERROR: ELF segment crosses Vibux user-address limit"
            ),
            Self::TooManyLoadSegments => {
                writeln!(out, "VELF ERROR: ELF has too many PT_LOAD segments")
            }
            Self::OutOfMemory => writeln!(out, "VELF ERROR: physical frame allocator is exhausted"),
            Self::PageAlreadyMapped(a) => {
                writeln!(out, "VELF ERROR: user page 0x{:016X} is already mapped", a)
            }
            Self::MappingFailed => writeln!(out, "VELF ERROR: Vibux page-table insertion failed"),
            Self::InvalidInterpreter => writeln!(out, "VELF ERROR: PT_INTERP is malformed"),
            Self::InvalidDynamic => writeln!(out, "VELF ERROR: PT_DYNAMIC metadata is malformed"),
            Self::InvalidStringTable => {
                writeln!(out, "VELF ERROR: dynamic string-table reference is invalid")
            }
        };
    }
}
