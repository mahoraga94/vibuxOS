use super::error::Error;
use super::format::{ET_DYN, Image, PF_W, PF_X, PT_LOAD, ProgramHeader};
use crate::arch::x86_64::paging::{PageFlags, PageTableManager};
use crate::memory::FrameAllocator;
use core::ptr::{copy_nonoverlapping, write_bytes};

pub const PAGE_SIZE: u64 = 4096;
pub const USER_LIMIT: u64 = 0x0000_8000_0000_0000;

#[derive(Clone, Copy)]
pub struct Loaded {
    pub entry: u64,
    pub bias: u64,
    pub start: u64,
    pub end: u64,
    pub segments: usize,
    pub pages: usize,
}

#[inline(always)]
fn down(value: u64) -> u64 {
    value & !(PAGE_SIZE - 1)
}

#[inline(always)]
fn up(value: u64) -> Option<u64> {
    value.checked_add(PAGE_SIZE - 1).map(down)
}

#[inline(always)]
fn physical_pointer(physical_offset: u64, physical: u64) -> Result<*mut u8, Error> {
    let address = physical_offset
        .checked_add(physical)
        .ok_or(Error::AddressOverflow)?;
    Ok(address as *mut u8)
}

fn validate_segment(image: &Image<'_>, segment: ProgramHeader) -> Result<(), Error> {
    if segment.filesz > segment.memsz {
        return Err(Error::FileszGreaterThanMemsz);
    }
    if segment.align != 0 && (segment.align & (segment.align - 1)) != 0 {
        return Err(Error::BadAlignment);
    }
    image.bytes_at(segment.offset, segment.filesz)?;
    segment
        .vaddr
        .checked_add(segment.memsz)
        .ok_or(Error::AddressOverflow)?;
    Ok(())
}

fn populate(
    image: &Image<'_>,
    segment: ProgramHeader,
    bias: u64,
    virtual_page: u64,
    physical_offset: u64,
    physical: u64,
) -> Result<(), Error> {
    let start = segment
        .vaddr
        .checked_add(bias)
        .ok_or(Error::AddressOverflow)?;
    let file_end = start
        .checked_add(segment.filesz)
        .ok_or(Error::AddressOverflow)?;
    let page_end = virtual_page
        .checked_add(PAGE_SIZE)
        .ok_or(Error::AddressOverflow)?;

    let copy_start = core::cmp::max(start, virtual_page);
    let copy_end = core::cmp::min(file_end, page_end);
    let destination = physical_pointer(physical_offset, physical)?;

    unsafe {
        write_bytes(destination, 0, PAGE_SIZE as usize);
    }
    if copy_start >= copy_end {
        return Ok(());
    }

    let source_offset = segment
        .offset
        .checked_add(copy_start - start)
        .ok_or(Error::AddressOverflow)?;
    let length = copy_end - copy_start;
    let source = image.bytes_at(source_offset, length)?;
    let destination_offset =
        usize::try_from(copy_start - virtual_page).map_err(|_| Error::AddressOverflow)?;

    unsafe {
        copy_nonoverlapping(
            source.as_ptr(),
            destination.add(destination_offset),
            source.len(),
        );
    }
    Ok(())
}

pub fn load_at(
    image: &Image<'_>,
    allocator: &mut FrameAllocator<'_>,
    physical_offset: u64,
    bias: u64,
) -> Result<Loaded, Error> {
    let mut segments = 0usize;
    let mut start = u64::MAX;
    let mut end = 0u64;

    for index in 0..usize::from(image.header.phnum) {
        let segment = image.phdr(index)?;
        if segment.kind != PT_LOAD {
            continue;
        }
        if segments >= 32 {
            return Err(Error::TooManyLoadSegments);
        }
        validate_segment(image, segment)?;

        let mapped_start = segment
            .vaddr
            .checked_add(bias)
            .map(down)
            .ok_or(Error::AddressOverflow)?;
        let raw_end = segment
            .vaddr
            .checked_add(segment.memsz)
            .and_then(|x| x.checked_add(bias))
            .ok_or(Error::AddressOverflow)?;
        let mapped_end = up(raw_end).ok_or(Error::AddressOverflow)?;

        if mapped_start >= USER_LIMIT || mapped_end > USER_LIMIT {
            return Err(Error::OutsideUserRange);
        }
        start = core::cmp::min(start, mapped_start);
        end = core::cmp::max(end, mapped_end);
        segments += 1;
    }
    if segments == 0 {
        return Err(Error::BadHeader("ELF contains no PT_LOAD segments"));
    }

    let paging = PageTableManager::new(physical_offset);
    let mut mapped_pages = 0usize;

    for index in 0..usize::from(image.header.phnum) {
        let segment = image.phdr(index)?;
        if segment.kind != PT_LOAD {
            continue;
        }

        let mapped_start = segment
            .vaddr
            .checked_add(bias)
            .map(down)
            .ok_or(Error::AddressOverflow)?;
        let raw_end = segment
            .vaddr
            .checked_add(segment.memsz)
            .and_then(|x| x.checked_add(bias))
            .ok_or(Error::AddressOverflow)?;
        let mapped_end = up(raw_end).ok_or(Error::AddressOverflow)?;

        let mut page = mapped_start;
        while page < mapped_end {
            if paging.translate(page).is_some() {
                return Err(Error::PageAlreadyMapped(page));
            }
            let frame = allocator.allocate().ok_or(Error::OutOfMemory)?;
            let physical = frame.addr();

            let mut flags = PageFlags::USER;
            if segment.flags & PF_W != 0 {
                flags = flags.union(PageFlags::WRITABLE);
            }
            if segment.flags & PF_X == 0 {
                flags = flags.union(PageFlags::NO_EXECUTE);
            }

            paging
                .map_4k(page, physical, flags, || {
                    allocator.allocate().map(|x| x.addr())
                })
                .map_err(|_| Error::MappingFailed)?;

            populate(image, segment, bias, page, physical_offset, physical)?;
            mapped_pages += 1;
            page = page.checked_add(PAGE_SIZE).ok_or(Error::AddressOverflow)?;
        }
    }

    let entry = image
        .header
        .entry
        .checked_add(bias)
        .ok_or(Error::AddressOverflow)?;
    Ok(Loaded {
        entry,
        bias,
        start,
        end,
        segments,
        pages: mapped_pages,
    })
}

pub fn load(
    image: &Image<'_>,
    allocator: &mut FrameAllocator<'_>,
    physical_offset: u64,
) -> Result<Loaded, Error> {
    let bias = if image.header.kind == ET_DYN {
        0x0000_4000_0000_0000
    } else {
        0
    };
    load_at(image, allocator, physical_offset, bias)
}
