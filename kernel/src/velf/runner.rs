use super::error::Error;
use super::format::Image;
use super::loader::{Loaded, PAGE_SIZE, load_at};
use super::store;
use crate::arch::x86_64::paging::{PageFlags, PageTableManager};
use crate::console::Serial;
use crate::memory::FrameAllocator;
use core::arch::asm;
use core::fmt::Write;

pub const MAIN_BASE: u64 = 0x0000_4000_0000_0000;
pub const RTLD_BASE: u64 = 0x0000_5000_0000_0000;
pub const MMAP_BASE: u64 = 0x0000_6000_0000_0000;
pub const HEAP_BASE: u64 = 0x0000_7000_0000_0000;
const USER_LIMIT: u64 = 0x0000_8000_0000_0000;
pub const STACK_TOP: u64 = 0x0000_7F00_0000_0000;
const STACK_PAGES: usize = 64;
const STACK_SIZE: u64 = STACK_PAGES as u64 * PAGE_SIZE;

#[inline(always)]
fn stack_bottom() -> u64 {
    STACK_TOP - STACK_SIZE
}

#[inline(always)]
fn stack_guard() -> u64 {
    stack_bottom() - PAGE_SIZE
}

fn image_range(image: &Image<'_>, bias: u64) -> Option<(u64, u64)> {
    let mut start = u64::MAX;
    let mut end = 0u64;
    for index in 0..usize::from(image.header.phnum) {
        let segment = image.phdr(index).ok()?;
        if segment.kind != super::format::PT_LOAD {
            continue;
        }
        let segment_start = segment.vaddr.checked_add(bias)? & !(PAGE_SIZE - 1);
        let segment_end = segment
            .vaddr
            .checked_add(segment.memsz)?
            .checked_add(bias)?
            .checked_add(PAGE_SIZE - 1)?
            & !(PAGE_SIZE - 1);
        start = start.min(segment_start);
        end = end.max(segment_end);
    }
    if start < end {
        Some((start, end))
    } else {
        None
    }
}

struct StackBuilder<'a> {
    frames: &'a [u64],
    physical_offset: u64,
    sp: u64,
    bottom: u64,
}

impl<'a> StackBuilder<'a> {
    fn new(frames: &'a [u64], physical_offset: u64) -> Self {
        Self {
            frames,
            physical_offset,
            sp: STACK_TOP,
            bottom: stack_bottom(),
        }
    }

    #[inline(always)]
    fn ptr_at(&self, user_addr: u64) -> *mut u8 {
        let offset_in_stack = user_addr - self.bottom;
        let page_idx = (offset_in_stack / PAGE_SIZE) as usize;
        let page_offset = (offset_in_stack % PAGE_SIZE) as usize;
        let phys = self.frames[page_idx];
        (self.physical_offset + phys + page_offset as u64) as *mut u8
    }

    fn reserve(&mut self, size: usize, alignment: u64) -> Option<u64> {
        if alignment == 0 || !alignment.is_power_of_two() {
            return None;
        }
        let raw = self.sp.checked_sub(size as u64)?;
        let aligned = raw & !(alignment - 1);
        if aligned < self.bottom {
            return None;
        }
        self.sp = aligned;
        Some(self.sp)
    }

    fn put_bytes(&mut self, bytes: &[u8]) -> Option<u64> {
        let address = self.reserve(bytes.len() + 1, 1)?;
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), self.ptr_at(address), bytes.len());
            self.ptr_at(address).add(bytes.len()).write(0);
        }
        Some(address)
    }

    fn put_u64(&mut self, value: u64) -> Option<u64> {
        let address = self.reserve(8, 8)?;
        unsafe {
            self.ptr_at(address).cast::<u64>().write(value);
        }
        Some(address)
    }

    fn align_before_block(&mut self, block_size: usize) -> Option<()> {
        let future = self.sp.checked_sub(block_size as u64)?;
        let padding = future & 0xF;
        if padding != 0 {
            self.sp = self.sp.checked_sub(padding)?;
        }
        if self.sp < self.bottom {
            return None;
        }
        Some(())
    }

    fn push_block(&mut self, entries: &[u64]) -> Option<u64> {
        let total_bytes = entries.len().checked_mul(8)?;
        self.align_before_block(total_bytes)?;
        for &val in entries.iter().rev() {
            self.put_u64(val)?;
        }
        Some(self.sp)
    }
}

fn random_seed() -> [u8; 16] {
    let low: u32;
    let high: u32;
    unsafe {
        asm!("rdtsc", lateout("eax") low, lateout("edx") high, options(nomem, nostack, preserves_flags));
    }
    let a = ((high as u64) << 32) | low as u64;
    let b = a.rotate_left(29) ^ 0x9E37_79B9_7F4A_7C15u64;
    let mut out = [0u8; 16];
    out[..8].copy_from_slice(&a.to_ne_bytes());
    out[8..].copy_from_slice(&b.to_ne_bytes());
    out
}

fn allocate_user_stack(
    allocator: &mut FrameAllocator<'_>,
    physical_offset: u64,
) -> Result<[u64; STACK_PAGES], Error> {
    let paging = PageTableManager::new(physical_offset);
    if paging.translate(stack_guard()).is_some() {
        return Err(Error::MappingFailed);
    }

    let flags = PageFlags::USER
        .union(PageFlags::WRITABLE)
        .union(PageFlags::NO_EXECUTE);
    let mut frames = [0u64; STACK_PAGES];
    let mut page = stack_bottom();

    for i in 0..STACK_PAGES {
        let frame = allocator.allocate().ok_or(Error::OutOfMemory)?;
        let physical = frame.addr();
        frames[i] = physical;

        paging
            .map_4k(page, physical, flags, || {
                allocator.allocate().map(|x| x.addr())
            })
            .map_err(|_| Error::MappingFailed)?;

        unsafe {
            core::ptr::write_bytes(
                (physical_offset + physical) as *mut u8,
                0,
                PAGE_SIZE as usize,
            );
        }
        page += PAGE_SIZE;
    }
    Ok(frames)
}

pub fn exec_program(
    allocator: &mut FrameAllocator<'_>,
    physical_offset: u64,
    serial: &mut Serial,
    path: &[u8],
    argv: &[&[u8]],
    envp: &[&[u8]],
) -> Result<(), u64> {
    let Some(binary) = store::find(path) else {
        return Err(2);
    };
    let main_image = Image::parse(binary.bytes).map_err(|_| 8u64)?;

    let interp_path = match main_image.interpreter().map_err(|_| 8u64)? {
        Some(value) => value,
        None => b"",
    };

    let rtld_binary = if interp_path.is_empty() {
        None
    } else {
        store::find(interp_path)
    };
    if !interp_path.is_empty() && rtld_binary.is_none() {
        return Err(2);
    }

    let rtld_image = match rtld_binary.map(|x| x.bytes) {
        Some(bytes) => Some(Image::parse(bytes).map_err(|_| 8u64)?),
        None => None,
    };

    let Some((main_start, main_end)) = image_range(&main_image, MAIN_BASE) else {
        return Err(8);
    };
    let (rtld_start, rtld_end) = match rtld_image {
        Some(ref image) => image_range(image, RTLD_BASE).ok_or(8u64)?,
        None => (0, 0),
    };

    crate::arch::x86_64::syscall::reset_process_for_exec(
        main_start,
        main_end,
        rtld_start,
        rtld_end,
        stack_bottom(),
        STACK_TOP,
        0x0000_5FFF_0000_0000,
    );

    let main = load_at(&main_image, allocator, physical_offset, MAIN_BASE).map_err(|_| 12u64)?;
    let rtld = match rtld_image {
        Some(image) => load_at(&image, allocator, physical_offset, RTLD_BASE).map_err(|_| 12u64)?,
        None => main,
    };

    let frames = allocate_user_stack(allocator, physical_offset).map_err(|_| 12u64)?;
    let mut stack = StackBuilder::new(&frames, physical_offset);

    let execfn = stack.put_bytes(path).ok_or(12u64)?;
    let platform = stack.put_bytes(b"x86_64").ok_or(12u64)?;
    let random = random_seed();
    let random_addr = stack.reserve(16, 16).ok_or(12u64)?;
    unsafe {
        core::ptr::copy_nonoverlapping(random.as_ptr(), stack.ptr_at(random_addr), 16);
    }

    let mut argv_ptrs = [0u64; 32];
    let mut env_ptrs = [0u64; 32];
    let argc = core::cmp::min(argv.len(), argv_ptrs.len());
    for i in 0..argc {
        argv_ptrs[i] = stack.put_bytes(argv[i]).ok_or(12u64)?;
    }

    let envc = core::cmp::min(envp.len(), env_ptrs.len());
    for i in 0..envc {
        env_ptrs[i] = stack.put_bytes(envp[i]).ok_or(12u64)?;
    }

    let phdr = main.bias.checked_add(main_image.header.phoff).ok_or(8u64)?;
    let auxv = [
        (3u64, phdr),
        (4u64, main_image.header.phentsize as u64),
        (5u64, main_image.header.phnum as u64),
        (6u64, PAGE_SIZE),
        (7u64, rtld.bias),
        (9u64, main.entry),
        (11u64, 1000),
        (12u64, 1000),
        (13u64, 1000),
        (14u64, 1000),
        (15u64, platform),
        (16u64, 0),
        (17u64, 250),
        (23u64, 0),
        (25u64, random_addr),
        (26u64, 0),
        (31u64, execfn),
        (0u64, 0),
    ];

    let total = 1usize + argc + 1 + envc + 1 + auxv.len() * 2;
    let mut entries = [0u64; 128];
    let mut count = 0usize;

    entries[count] = argc as u64;
    count += 1;
    for i in 0..argc {
        entries[count] = argv_ptrs[i];
        count += 1;
    }
    entries[count] = 0;
    count += 1;
    for i in 0..envc {
        entries[count] = env_ptrs[i];
        count += 1;
    }
    entries[count] = 0;
    count += 1;
    for &(kind, value) in &auxv {
        entries[count] = kind;
        count += 1;
        entries[count] = value;
        count += 1;
    }

    if count != total {
        return Err(8);
    }
    let sp = stack.push_block(&entries[..count]).ok_or(12u64)?;

    if !crate::process::prepare_exec_tls() {
        return Err(12);
    }
    unsafe {
        crate::arch::x86_64::syscall::enter_vfork_exec(rtld.entry, sp);
    }
}

pub fn run_bash(allocator: &mut FrameAllocator<'_>, physical_offset: u64, serial: &mut Serial) {
    let bash_bytes = store::bash();
    let bash_image = match Image::parse(bash_bytes) {
        Ok(image) => image,
        Err(error) => {
            error.report(serial);
            return;
        }
    };

    let rtld_path = match bash_image.interpreter() {
        Ok(Some(path)) => path,
        Ok(None) => {
            let _ = writeln!(serial, "VELF EXEC ERROR: Bash has no PT_INTERP");
            return;
        }
        Err(error) => {
            error.report(serial);
            return;
        }
    };

    let Some(rtld_binary) = store::find(rtld_path) else {
        let _ = writeln!(serial, "VELF EXEC ERROR: embedded PT_INTERP not found");
        return;
    };

    let rtld_image = match Image::parse(rtld_binary.bytes) {
        Ok(image) => image,
        Err(error) => {
            error.report(serial);
            return;
        }
    };

    let Some((main_start, main_end)) = image_range(&bash_image, MAIN_BASE) else {
        let _ = writeln!(serial, "VELF EXEC ERROR: invalid Bash PT_LOAD range");
        return;
    };
    let Some((rtld_start, rtld_end)) = image_range(&rtld_image, RTLD_BASE) else {
        let _ = writeln!(serial, "VELF EXEC ERROR: invalid RTLD PT_LOAD range");
        return;
    };

    crate::arch::x86_64::syscall::reset_process_for_exec(
        main_start,
        main_end,
        rtld_start,
        rtld_end,
        stack_bottom(),
        STACK_TOP,
        0x0000_5FFF_0000_0000,
    );

    let main = match load_at(&bash_image, allocator, physical_offset, MAIN_BASE) {
        Ok(value) => value,
        Err(error) => {
            error.report(serial);
            return;
        }
    };
    let rtld = match load_at(&rtld_image, allocator, physical_offset, RTLD_BASE) {
        Ok(value) => value,
        Err(error) => {
            error.report(serial);
            return;
        }
    };

    let frames = match allocate_user_stack(allocator, physical_offset) {
        Ok(f) => f,
        Err(error) => {
            error.report(serial);
            return;
        }
    };

    let mut stack = StackBuilder::new(&frames, physical_offset);
    let execfn = stack.put_bytes(b"/bin/bash").unwrap();
    let platform = stack.put_bytes(b"x86_64").unwrap();
    let env_home = stack.put_bytes(b"HOME=/home/boi").unwrap();
    let env_user = stack.put_bytes(b"USER=boi").unwrap();
    let env_shell = stack.put_bytes(b"SHELL=/bin/bash").unwrap();
    let env_path = stack.put_bytes(b"PATH=/bin:/usr/bin").unwrap();
    let env_pwd = stack.put_bytes(b"PWD=/home/boi").unwrap();
    let env_ld_library_path = stack.put_bytes(b"LD_LIBRARY_PATH=/lib64").unwrap();

    let random = random_seed();
    let random_addr = stack.reserve(16, 16).unwrap();
    unsafe {
        core::ptr::copy_nonoverlapping(random.as_ptr(), stack.ptr_at(random_addr), 16);
    }

    let argv0 = stack.put_bytes(b"/bin/bash").unwrap();
    let argv1 = stack.put_bytes(b"--norc").unwrap();
    let argv2 = stack.put_bytes(b"-i").unwrap();

    let phdr = main.bias.checked_add(bash_image.header.phoff).unwrap();
    let auxv = [
        (3u64, phdr),
        (4u64, bash_image.header.phentsize as u64),
        (5u64, bash_image.header.phnum as u64),
        (6u64, PAGE_SIZE),
        (7u64, rtld.bias),
        (9u64, main.entry),
        (11u64, 1000),
        (12u64, 1000),
        (13u64, 1000),
        (14u64, 1000),
        (15u64, platform),
        (16u64, 0),
        (17u64, 250),
        (23u64, 0),
        (25u64, random_addr),
        (26u64, 0),
        (31u64, execfn),
        (0u64, 0),
    ];

    let envs = [
        env_home,
        env_user,
        env_shell,
        env_path,
        env_pwd,
        env_ld_library_path,
    ];
    let total_entries = 1 + 3 + 1 + envs.len() + 1 + auxv.len() * 2;
    let mut entries = [0u64; 64];
    let mut count = 0usize;

    entries[count] = 3;
    count += 1;
    entries[count] = argv0;
    count += 1;
    entries[count] = argv1;
    count += 1;
    entries[count] = argv2;
    count += 1;
    entries[count] = 0;
    count += 1;
    for &env in &envs {
        entries[count] = env;
        count += 1;
    }
    entries[count] = 0;
    count += 1;
    for &(kind, value) in &auxv {
        entries[count] = kind;
        count += 1;
        entries[count] = value;
        count += 1;
    }

    if count != total_entries {
        return;
    }
    let sp = stack.push_block(&entries[..count]).unwrap();

    let exit_code = crate::arch::x86_64::syscall::enter_user(rtld.entry, sp);
    let _ = writeln!(serial, "VELF PROCESS EXIT: {}", exit_code);
}
