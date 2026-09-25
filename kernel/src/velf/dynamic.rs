use super::error::Error;
use super::format::{Image, read_u64};

pub const DT_NULL: u64 = 0;
pub const DT_NEEDED: u64 = 1;
pub const DT_STRTAB: u64 = 5;
pub const DT_RELA: u64 = 7;
pub const DT_RELASZ: u64 = 8;
pub const DT_RELAENT: u64 = 9;
pub const DT_STRSZ: u64 = 10;
pub const DT_JMPREL: u64 = 23;
pub const DT_PLTRELSZ: u64 = 2;

const MAX_NEEDED: usize = 32;

#[derive(Clone, Copy)]
pub struct Info {
    pub needed: [u64; MAX_NEEDED],
    pub needed_count: usize,
    pub strtab: u64,
    pub strsz: u64,
    pub rela: u64,
    pub relasz: u64,
    pub relaent: u64,
    pub jmprel: u64,
    pub pltrelsz: u64,
}

impl Info {
    pub const fn empty() -> Self {
        Self {
            needed: [0; MAX_NEEDED],
            needed_count: 0,
            strtab: 0,
            strsz: 0,
            rela: 0,
            relasz: 0,
            relaent: 0,
            jmprel: 0,
            pltrelsz: 0,
        }
    }
}

pub fn parse(image: &Image<'_>) -> Result<Option<Info>, Error> {
    let Some(ph) = image.dynamic()? else {
        return Ok(None);
    };
    if ph.filesz == 0 || ph.filesz % 16 != 0 {
        return Err(Error::InvalidDynamic);
    }

    let data = image.bytes_at(ph.offset, ph.filesz)?;
    let mut info = Info::empty();
    let mut at = 0usize;

    while at + 16 <= data.len() {
        let tag = read_u64(data, at).ok_or(Error::InvalidDynamic)?;
        let value = read_u64(data, at + 8).ok_or(Error::InvalidDynamic)?;
        at += 16;
        if tag == DT_NULL {
            break;
        }

        match tag {
            DT_NEEDED => {
                if info.needed_count >= MAX_NEEDED {
                    return Err(Error::InvalidDynamic);
                }
                info.needed[info.needed_count] = value;
                info.needed_count += 1;
            }
            DT_STRTAB => info.strtab = value,
            DT_STRSZ => info.strsz = value,
            DT_RELA => info.rela = value,
            DT_RELASZ => info.relasz = value,
            DT_RELAENT => info.relaent = value,
            DT_JMPREL => info.jmprel = value,
            DT_PLTRELSZ => info.pltrelsz = value,
            _ => {}
        }
    }

    if info.needed_count != 0 && (info.strtab == 0 || info.strsz == 0) {
        return Err(Error::InvalidDynamic);
    }
    if info.relasz != 0 && info.relaent == 0 {
        return Err(Error::InvalidDynamic);
    }

    Ok(Some(info))
}

pub fn needed_name<'a>(image: &'a Image<'a>, info: &Info, index: usize) -> Result<&'a [u8], Error> {
    if index >= info.needed_count {
        return Err(Error::InvalidStringTable);
    }
    let string_offset = info.needed[index];
    if string_offset >= info.strsz {
        return Err(Error::InvalidStringTable);
    }

    let virtual_address = info
        .strtab
        .checked_add(string_offset)
        .ok_or(Error::InvalidStringTable)?;
    let file_offset = image
        .address_to_file(virtual_address)
        .ok_or(Error::InvalidStringTable)?;
    let remaining = info.strsz - string_offset;
    let data = image.bytes_at(file_offset, remaining)?;

    let end = data
        .iter()
        .position(|&b| b == 0)
        .ok_or(Error::InvalidStringTable)?;
    Ok(&data[..end])
}
