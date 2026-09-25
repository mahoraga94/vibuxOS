use super::error::Error;

pub const ELF64: u8 = 2;
pub const LITTLE_ENDIAN: u8 = 1;
pub const ET_EXEC: u16 = 2;
pub const ET_DYN: u16 = 3;
pub const EM_X86_64: u16 = 0x003E;
pub const PT_LOAD: u32 = 1;
pub const PT_DYNAMIC: u32 = 2;
pub const PT_INTERP: u32 = 3;
pub const PT_PHDR: u32 = 6;
pub const PT_TLS: u32 = 7;
pub const PT_GNU_EH_FRAME: u32 = 0x6474_E550;
pub const PT_GNU_STACK: u32 = 0x6474_E551;
pub const PT_GNU_RELRO: u32 = 0x6474_E552;
pub const PF_X: u32 = 1;
pub const PF_W: u32 = 2;
pub const PF_R: u32 = 4;

#[derive(Clone, Copy)]
pub struct Header {
    pub kind: u16,
    pub machine: u16,
    pub version: u32,
    pub entry: u64,
    pub phoff: u64,
    pub phnum: u16,
    pub phentsize: u16,
}

#[derive(Clone, Copy)]
pub struct ProgramHeader {
    pub kind: u32,
    pub flags: u32,
    pub offset: u64,
    pub vaddr: u64,
    pub paddr: u64,
    pub filesz: u64,
    pub memsz: u64,
    pub align: u64,
}

pub struct Image<'a> {
    pub bytes: &'a [u8],
    pub header: Header,
}

#[inline(always)]
fn read_u16(data: &[u8], at: usize) -> Option<u16> {
    let bytes: [u8; 2] = data.get(at..at + 2)?.try_into().ok()?;
    Some(u16::from_le_bytes(bytes))
}

#[inline(always)]
fn read_u32(data: &[u8], at: usize) -> Option<u32> {
    let bytes: [u8; 4] = data.get(at..at + 4)?.try_into().ok()?;
    Some(u32::from_le_bytes(bytes))
}

#[inline(always)]
pub fn read_u64(data: &[u8], at: usize) -> Option<u64> {
    let bytes: [u8; 8] = data.get(at..at + 8)?.try_into().ok()?;
    Some(u64::from_le_bytes(bytes))
}

impl<'a> Image<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, Error> {
        if bytes.len() < 64 {
            return Err(if bytes.is_empty() {
                Error::Empty
            } else {
                Error::TooSmall
            });
        }
        if &bytes[0..4] != b"\x7FELF" {
            return Err(Error::BadMagic);
        }
        if bytes[4] != ELF64 {
            return Err(Error::NotElf64);
        }
        if bytes[5] != LITTLE_ENDIAN {
            return Err(Error::NotLittleEndian);
        }
        if bytes[6] != 1 {
            return Err(Error::BadIdentVersion);
        }

        let header = Header {
            kind: read_u16(bytes, 16).ok_or(Error::BadHeader("e_type"))?,
            machine: read_u16(bytes, 18).ok_or(Error::BadHeader("e_machine"))?,
            version: read_u32(bytes, 20).ok_or(Error::BadHeader("e_version"))?,
            entry: read_u64(bytes, 24).ok_or(Error::BadHeader("e_entry"))?,
            phoff: read_u64(bytes, 32).ok_or(Error::BadHeader("e_phoff"))?,
            phentsize: read_u16(bytes, 54).ok_or(Error::BadHeader("e_phentsize"))?,
            phnum: read_u16(bytes, 56).ok_or(Error::BadHeader("e_phnum"))?,
        };

        if header.machine != EM_X86_64 {
            return Err(Error::UnsupportedMachine(header.machine));
        }
        if header.kind != ET_EXEC && header.kind != ET_DYN {
            return Err(Error::UnsupportedType(header.kind));
        }
        if header.version != 1 {
            return Err(Error::BadHeader("e_version is not EV_CURRENT"));
        }
        if header.phentsize != 56 {
            return Err(Error::BadPhdrSize(header.phentsize));
        }

        let phoff = usize::try_from(header.phoff).map_err(|_| Error::PhdrOutOfBounds)?;
        let bytes_needed = usize::from(header.phnum)
            .checked_mul(56)
            .ok_or(Error::PhdrOutOfBounds)?;
        let end = phoff
            .checked_add(bytes_needed)
            .ok_or(Error::PhdrOutOfBounds)?;
        if end > bytes.len() {
            return Err(Error::PhdrOutOfBounds);
        }

        Ok(Self { bytes, header })
    }

    #[inline(always)]
    pub fn phdr(&self, index: usize) -> Result<ProgramHeader, Error> {
        if index >= usize::from(self.header.phnum) {
            return Err(Error::PhdrOutOfBounds);
        }
        let base = usize::try_from(self.header.phoff)
            .map_err(|_| Error::PhdrOutOfBounds)?
            .checked_add(index.checked_mul(56).ok_or(Error::PhdrOutOfBounds)?)
            .ok_or(Error::PhdrOutOfBounds)?;
        if base.checked_add(56).ok_or(Error::PhdrOutOfBounds)? > self.bytes.len() {
            return Err(Error::PhdrOutOfBounds);
        }

        Ok(ProgramHeader {
            kind: read_u32(self.bytes, base).ok_or(Error::PhdrOutOfBounds)?,
            flags: read_u32(self.bytes, base + 4).ok_or(Error::PhdrOutOfBounds)?,
            offset: read_u64(self.bytes, base + 8).ok_or(Error::PhdrOutOfBounds)?,
            vaddr: read_u64(self.bytes, base + 16).ok_or(Error::PhdrOutOfBounds)?,
            paddr: read_u64(self.bytes, base + 24).ok_or(Error::PhdrOutOfBounds)?,
            filesz: read_u64(self.bytes, base + 32).ok_or(Error::PhdrOutOfBounds)?,
            memsz: read_u64(self.bytes, base + 40).ok_or(Error::PhdrOutOfBounds)?,
            align: read_u64(self.bytes, base + 48).ok_or(Error::PhdrOutOfBounds)?,
        })
    }

    #[inline(always)]
    pub fn bytes_at(&self, offset: u64, length: u64) -> Result<&'a [u8], Error> {
        let start = usize::try_from(offset).map_err(|_| Error::SegmentOutOfBounds)?;
        let size = usize::try_from(length).map_err(|_| Error::SegmentOutOfBounds)?;
        let end = start.checked_add(size).ok_or(Error::SegmentOutOfBounds)?;
        if end > self.bytes.len() {
            return Err(Error::SegmentOutOfBounds);
        }
        Ok(&self.bytes[start..end])
    }

    pub fn interpreter(&self) -> Result<Option<&'a [u8]>, Error> {
        for index in 0..usize::from(self.header.phnum) {
            let ph = self.phdr(index)?;
            if ph.kind != PT_INTERP {
                continue;
            }
            let data = self.bytes_at(ph.offset, ph.filesz)?;
            if data.is_empty() || data[data.len() - 1] != 0 {
                return Err(Error::InvalidInterpreter);
            }
            return Ok(Some(&data[..data.len() - 1]));
        }
        Ok(None)
    }

    pub fn dynamic(&self) -> Result<Option<ProgramHeader>, Error> {
        for index in 0..usize::from(self.header.phnum) {
            let ph = self.phdr(index)?;
            if ph.kind == PT_DYNAMIC {
                return Ok(Some(ph));
            }
        }
        Ok(None)
    }

    pub fn address_to_file(&self, address: u64) -> Option<u64> {
        for index in 0..usize::from(self.header.phnum) {
            let ph = self.phdr(index).ok()?;
            if ph.kind != PT_LOAD {
                continue;
            }
            let end = ph.vaddr.checked_add(ph.filesz)?;
            if address >= ph.vaddr && address < end {
                return ph.offset.checked_add(address - ph.vaddr);
            }
        }
        None
    }
}
