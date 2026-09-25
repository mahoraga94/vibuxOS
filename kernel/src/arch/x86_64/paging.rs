use core::arch::asm;

use core::ptr::{addr_of_mut, copy_nonoverlapping, read_volatile, write_bytes, write_volatile};
const PAGE_SIZE: u64 = 4096;
const ENTRY_ADDR_MASK: u64 = 0x000f_ffff_ffff_f000;
const CANONICAL_MASK: u64 = 0xffff_8000_0000_0000;
const USER_VA_LIMIT: u64 = 0x0000_8000_0000_0000;

const PRESENT: u64 = 1 << 0;
const WRITE: u64 = 1 << 1;
const USER: u64 = 1 << 2;
const PWT: u64 = 1 << 3;
const PCD: u64 = 1 << 4;
const HUGE: u64 = 1 << 7;
const GLOBAL: u64 = 1 << 8;
const NX: u64 = 1 << 63;

static mut PHYS_OFFSET: u64 = 0;

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageFlags(u64);

impl PageFlags {
    pub const PRESENT: Self = Self(PRESENT);
    pub const WRITABLE: Self = Self(WRITE);
    pub const USER: Self = Self(USER);
    pub const WRITE_THROUGH: Self = Self(PWT);
    pub const CACHE_DISABLE: Self = Self(PCD);
    pub const GLOBAL: Self = Self(GLOBAL);
    pub const NO_EXECUTE: Self = Self(NX);

    #[inline(always)]
    pub const fn empty() -> Self {
        Self(0)
    }

    #[inline(always)]
    pub const fn bits(self) -> u64 {
        self.0
    }

    #[inline(always)]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    #[inline(always)]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Mapping {
    pub virtual_address: u64,
    pub physical_address: u64,
    pub flags: PageFlags,
    pub page_size: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct PagingReport {
    pub cr3: u64,
    pub physical_memory_offset: Option<u64>,
    pub nx_enabled: bool,
    pub write_protect: bool,
    pub global_pages: bool,
    pub pcid: bool,
    pub long_mode: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapError {
    MissingPhysicalMemoryMap,
    NonCanonicalAddress,
    AlreadyMapped,
    HugePageConflict,
    OutOfMemory,
    InvalidPhysicalAddress,
    KernelMapping,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnmapError {
    MissingPhysicalMemoryMap,
    NonCanonicalAddress,
    NotMapped,
    HugePage,
}

#[derive(Clone, Copy)]
pub struct PageTableManager {
    physical_memory_offset: u64,
}

impl PageTableManager {
    #[inline(always)]
    pub const fn new(physical_memory_offset: u64) -> Self {
        Self {
            physical_memory_offset,
        }
    }
    #[inline(always)]
    pub fn clone_user_address_space(
        &self,
        parent_cr3: u64,
        allocator: &mut crate::memory::FrameAllocator<'_>,
    ) -> Option<u64> {
        let child_l4_frame = allocator.allocate()?;
        let child_l4 = child_l4_frame.addr();

        unsafe {
            self.zero_table(child_l4);

            let parent_l4 = self.table_mut(parent_cr3);
            let child_l4_table = self.table_mut(child_l4);

            for i in 0..512usize {
                let e4 = read_volatile(&parent_l4[i]);
                if e4 & PRESENT == 0 {
                    continue;
                }
                if e4 & USER == 0 {
                    write_volatile(&mut child_l4_table[i], e4);
                    continue;
                }

                let child_l3_frame = allocator.allocate()?;
                let child_l3 = child_l3_frame.addr();
                self.zero_table(child_l3);

                let parent_l3 = self.table_mut(e4 & ENTRY_ADDR_MASK);
                let child_l3_table = self.table_mut(child_l3);

                for j in 0..512usize {
                    let e3 = read_volatile(&parent_l3[j]);
                    if e3 & PRESENT == 0 {
                        continue;
                    }
                    if e3 & HUGE != 0 || e3 & USER == 0 {
                        write_volatile(&mut child_l3_table[j], e3);
                        continue;
                    }

                    let child_l2_frame = allocator.allocate()?;
                    let child_l2 = child_l2_frame.addr();
                    self.zero_table(child_l2);

                    let parent_l2 = self.table_mut(e3 & ENTRY_ADDR_MASK);
                    let child_l2_table = self.table_mut(child_l2);

                    for k in 0..512usize {
                        let e2 = read_volatile(&parent_l2[k]);
                        if e2 & PRESENT == 0 {
                            continue;
                        }
                        if e2 & HUGE != 0 || e2 & USER == 0 {
                            write_volatile(&mut child_l2_table[k], e2);
                            continue;
                        }

                        let child_l1_frame = allocator.allocate()?;
                        let child_l1 = child_l1_frame.addr();
                        self.zero_table(child_l1);

                        let parent_l1 = self.table_mut(e2 & ENTRY_ADDR_MASK);
                        let child_l1_table = self.table_mut(child_l1);

                        for l in 0..512usize {
                            let e1 = read_volatile(&parent_l1[l]);
                            if e1 & PRESENT == 0 {
                                continue;
                            }

                            if e1 & USER == 0 {
                                write_volatile(&mut child_l1_table[l], e1);
                                continue;
                            }

                            let parent_pa = e1 & ENTRY_ADDR_MASK;
                            let new_frame = allocator.allocate()?;
                            let new_pa = new_frame.addr();

                            let src = self.phys_to_virt(parent_pa);
                            let dst = self.phys_to_virt(new_pa);
                            copy_nonoverlapping(src, dst, PAGE_SIZE as usize);

                            let flags = e1 & !ENTRY_ADDR_MASK;
                            write_volatile(&mut child_l1_table[l], new_pa | flags);
                        }

                        let e2_flags = e2 & !ENTRY_ADDR_MASK;
                        write_volatile(&mut child_l2_table[k], child_l1 | e2_flags);
                    }

                    let e3_flags = e3 & !ENTRY_ADDR_MASK;
                    write_volatile(&mut child_l3_table[j], child_l2 | e3_flags);
                }

                let e4_flags = e4 & !ENTRY_ADDR_MASK;
                write_volatile(&mut child_l4_table[i], child_l3 | e4_flags);
            }
        }

        Some(child_l4)
    }

    #[inline(always)]
    fn phys_to_virt(&self, physical: u64) -> *mut u8 {
        (self.physical_memory_offset + physical) as *mut u8
    }

    #[inline(always)]
    unsafe fn table_mut(&self, physical: u64) -> &'static mut [u64; 512] {
        &mut *(self.phys_to_virt(physical) as *mut [u64; 512])
    }

    #[inline(always)]
    unsafe fn zero_table(&self, physical: u64) {
        write_bytes(self.phys_to_virt(physical), 0, PAGE_SIZE as usize);
    }

    #[inline(always)]
    fn indexes(virtual_address: u64) -> (usize, usize, usize, usize) {
        (
            ((virtual_address >> 39) & 0x1ff) as usize,
            ((virtual_address >> 30) & 0x1ff) as usize,
            ((virtual_address >> 21) & 0x1ff) as usize,
            ((virtual_address >> 12) & 0x1ff) as usize,
        )
    }

    #[inline(always)]
    fn canonical(virtual_address: u64) -> bool {
        let upper = virtual_address & CANONICAL_MASK;
        upper == 0 || upper == CANONICAL_MASK
    }

    #[inline(always)]
    pub fn translate(&self, virtual_address: u64) -> Option<Mapping> {
        if !Self::canonical(virtual_address) {
            return None;
        }

        let cr3 = read_cr3() & ENTRY_ADDR_MASK;
        let (p4, p3, p2, p1) = Self::indexes(virtual_address);

        unsafe {
            let l4 = self.table_mut(cr3);
            let e4 = read_volatile(&l4[p4]);
            if e4 & PRESENT == 0 {
                return None;
            }

            let l3 = self.table_mut(e4 & ENTRY_ADDR_MASK);
            let e3 = read_volatile(&l3[p3]);
            if e3 & PRESENT == 0 {
                return None;
            }

            if e3 & HUGE != 0 {
                let base = e3 & 0x000f_ffc0_0000_0000;
                return Some(Mapping {
                    virtual_address: virtual_address & !((1 << 30) - 1),
                    physical_address: base + (virtual_address & ((1 << 30) - 1)),
                    flags: PageFlags(e3 & (!ENTRY_ADDR_MASK | NX)),
                    page_size: 1 << 30,
                });
            }

            let l2 = self.table_mut(e3 & ENTRY_ADDR_MASK);
            let e2 = read_volatile(&l2[p2]);
            if e2 & PRESENT == 0 {
                return None;
            }

            if e2 & HUGE != 0 {
                let base = e2 & 0x000f_ffff_ffe0_0000;
                return Some(Mapping {
                    virtual_address: virtual_address & !((1 << 21) - 1),
                    physical_address: base + (virtual_address & ((1 << 21) - 1)),
                    flags: PageFlags(e2 & (!ENTRY_ADDR_MASK | NX)),
                    page_size: 1 << 21,
                });
            }

            let l1 = self.table_mut(e2 & ENTRY_ADDR_MASK);
            let e1 = read_volatile(&l1[p1]);
            if e1 & PRESENT == 0 {
                return None;
            }

            Some(Mapping {
                virtual_address: virtual_address & !(PAGE_SIZE - 1),
                physical_address: (e1 & ENTRY_ADDR_MASK) + (virtual_address & (PAGE_SIZE - 1)),
                flags: PageFlags(e1 & (!ENTRY_ADDR_MASK | NX)),
                page_size: PAGE_SIZE,
            })
        }
    }

    #[inline(always)]
    pub fn map_4k<F>(
        &self,
        virtual_address: u64,
        physical_address: u64,
        flags: PageFlags,
        mut allocate_frame: F,
    ) -> Result<(), MapError>
    where
        F: FnMut() -> Option<u64>,
    {
        if !Self::canonical(virtual_address) {
            return Err(MapError::NonCanonicalAddress);
        }
        if virtual_address & (PAGE_SIZE - 1) != 0 || physical_address & (PAGE_SIZE - 1) != 0 {
            return Err(MapError::InvalidPhysicalAddress);
        }

        let is_user = flags.contains(PageFlags::USER);
        let user_bit = if is_user { USER } else { 0 };
        let leaf_bits = flags.bits() | PRESENT;

        if is_user && virtual_address >= USER_VA_LIMIT {
            return Err(MapError::NonCanonicalAddress);
        }

        let cr3 = read_cr3() & ENTRY_ADDR_MASK;
        let (p4, p3, p2, p1) = Self::indexes(virtual_address);

        unsafe {
            let l4 = self.table_mut(cr3);
            let mut e4 = read_volatile(&l4[p4]);
            if e4 & PRESENT == 0 {
                let frame = allocate_frame().ok_or(MapError::OutOfMemory)? & ENTRY_ADDR_MASK;
                self.zero_table(frame);
                e4 = frame | PRESENT | WRITE | user_bit;
                write_volatile(&mut l4[p4], e4);
            } else if is_user && e4 & USER == 0 {
                return Err(MapError::KernelMapping);
            }

            let l3 = self.table_mut(e4 & ENTRY_ADDR_MASK);
            let mut e3 = read_volatile(&l3[p3]);
            if e3 & PRESENT != 0 && e3 & HUGE != 0 {
                return Err(MapError::HugePageConflict);
            }
            if e3 & PRESENT == 0 {
                let frame = allocate_frame().ok_or(MapError::OutOfMemory)? & ENTRY_ADDR_MASK;
                self.zero_table(frame);
                e3 = frame | PRESENT | WRITE | user_bit;
                write_volatile(&mut l3[p3], e3);
            } else if is_user && e3 & USER == 0 {
                e3 |= USER;
                write_volatile(&mut l3[p3], e3);
            }

            let l2 = self.table_mut(e3 & ENTRY_ADDR_MASK);
            let mut e2 = read_volatile(&l2[p2]);
            if e2 & PRESENT != 0 && e2 & HUGE != 0 {
                return Err(MapError::HugePageConflict);
            }
            if e2 & PRESENT == 0 {
                let frame = allocate_frame().ok_or(MapError::OutOfMemory)? & ENTRY_ADDR_MASK;
                self.zero_table(frame);
                e2 = frame | PRESENT | WRITE | user_bit;
                write_volatile(&mut l2[p2], e2);
            } else if is_user && e2 & USER == 0 {
                e2 |= USER;
                write_volatile(&mut l2[p2], e2);
            }

            let l1 = self.table_mut(e2 & ENTRY_ADDR_MASK);
            if read_volatile(&l1[p1]) & PRESENT != 0 {
                return Err(MapError::AlreadyMapped);
            }

            write_volatile(
                &mut l1[p1],
                (physical_address & ENTRY_ADDR_MASK) | leaf_bits,
            );
            invlpg(virtual_address);
        }

        Ok(())
    }

    #[inline(always)]
    pub fn unmap_4k(&self, virtual_address: u64) -> Result<u64, UnmapError> {
        if !Self::canonical(virtual_address) {
            return Err(UnmapError::NonCanonicalAddress);
        }
        if virtual_address & (PAGE_SIZE - 1) != 0 {
            return Err(UnmapError::NotMapped);
        }

        let cr3 = read_cr3() & ENTRY_ADDR_MASK;
        let (p4, p3, p2, p1) = Self::indexes(virtual_address);

        unsafe {
            let l4 = self.table_mut(cr3);
            let e4 = read_volatile(&l4[p4]);
            if e4 & PRESENT == 0 {
                return Err(UnmapError::NotMapped);
            }
            let l3 = self.table_mut(e4 & ENTRY_ADDR_MASK);
            let e3 = read_volatile(&l3[p3]);
            if e3 & PRESENT == 0 || e3 & HUGE != 0 {
                return Err(if e3 & HUGE != 0 {
                    UnmapError::HugePage
                } else {
                    UnmapError::NotMapped
                });
            }
            let l2 = self.table_mut(e3 & ENTRY_ADDR_MASK);
            let e2 = read_volatile(&l2[p2]);
            if e2 & PRESENT == 0 || e2 & HUGE != 0 {
                return Err(if e2 & HUGE != 0 {
                    UnmapError::HugePage
                } else {
                    UnmapError::NotMapped
                });
            }
            let l1 = self.table_mut(e2 & ENTRY_ADDR_MASK);
            let old = read_volatile(&l1[p1]);
            if old & PRESENT == 0 {
                return Err(UnmapError::NotMapped);
            }
            write_volatile(&mut l1[p1], 0);
            invlpg(virtual_address);
            Ok(old & ENTRY_ADDR_MASK)
        }
    }
}

#[inline(always)]
pub fn read_cr3() -> u64 {
    let value: u64;
    unsafe {
        asm!("mov %cr3, {0}", out(reg) value, options(att_syntax, nomem, nostack, preserves_flags));
    }
    value
}

#[inline(always)]
fn read_cr0() -> u64 {
    let value: u64;
    unsafe {
        asm!("mov %cr0, {0}", out(reg) value, options(att_syntax, nomem, nostack, preserves_flags));
    }
    value
}

#[inline(always)]
fn write_cr0(value: u64) {
    unsafe {
        asm!("mov {0}, %cr0", in(reg) value, options(att_syntax, nostack, preserves_flags));
    }
}

#[inline(always)]
fn read_cr4() -> u64 {
    let value: u64;
    unsafe {
        asm!("mov %cr4, {0}", out(reg) value, options(att_syntax, nomem, nostack, preserves_flags));
    }
    value
}

#[inline(always)]
fn read_efer() -> u64 {
    let (low, high): (u32, u32);
    unsafe {
        asm!(
            "rdmsr",
            in("ecx") 0xc0000080u32,
            lateout("eax") low,
            lateout("edx") high,
            options(nostack, preserves_flags),
        );
    }
    ((high as u64) << 32) | low as u64
}

#[inline(always)]
fn write_efer(value: u64) {
    unsafe {
        asm!(
            "wrmsr",
            in("ecx") 0xc0000080u32,
            in("eax") value as u32,
            in("edx") (value >> 32) as u32,
            options(nostack, preserves_flags),
        );
    }
}

#[inline(always)]
fn invlpg(address: u64) {
    unsafe {
        asm!("invlpg [{}]", in(reg) address, options(nostack, preserves_flags));
    }
}

pub fn install_physmap(offset: u64) {
    unsafe {
        write_volatile(addr_of_mut!(PHYS_OFFSET), offset);
    }
}

pub fn physical_memory_offset() -> Option<u64> {
    unsafe {
        let offset = read_volatile(core::ptr::addr_of!(PHYS_OFFSET));
        if offset == 0 { None } else { Some(offset) }
    }
}

#[inline(always)]
pub fn enable_protection() {
    unsafe {
        let cr0 = read_cr0() | (1 << 16);
        write_cr0(cr0);

        let nx_supported = core::arch::x86_64::__cpuid(0x80000000).eax >= 0x80000001
            && core::arch::x86_64::__cpuid(0x80000001).edx & (1 << 20) != 0;
        if nx_supported {
            let efer = read_efer() | (1 << 11);
            write_efer(efer);
        }
    }
}

#[inline(always)]
pub fn report(offset: Option<u64>) -> PagingReport {
    let efer = read_efer();
    let cr0 = read_cr0();
    let cr4 = read_cr4();
    PagingReport {
        cr3: read_cr3() & ENTRY_ADDR_MASK,
        physical_memory_offset: offset.or_else(physical_memory_offset),
        nx_enabled: efer & (1 << 11) != 0,
        write_protect: cr0 & (1 << 16) != 0,
        global_pages: cr4 & (1 << 7) != 0,
        pcid: cr4 & (1 << 17) != 0,
        long_mode: efer & (1 << 10) != 0,
    }
}

pub fn touch_current_page_tables(offset: u64) -> bool {
    let mapper = PageTableManager::new(offset);
    let cr3 = read_cr3() & ENTRY_ADDR_MASK;
    unsafe {
        let table = mapper.table_mut(cr3);
        let entry = read_volatile(&table[0]);
        entry & PRESENT != 0
    }
}

impl PageTableManager {
    #[inline(always)]
    pub fn protect_4k(&self, virtual_address: u64, flags: PageFlags) -> Result<(), MapError> {
        if !Self::canonical(virtual_address) {
            return Err(MapError::NonCanonicalAddress);
        }

        if virtual_address & (PAGE_SIZE - 1) != 0 {
            return Err(MapError::InvalidPhysicalAddress);
        }

        let cr3 = read_cr3() & ENTRY_ADDR_MASK;
        let (p4, p3, p2, p1) = Self::indexes(virtual_address);
        let leaf_bits = flags.bits() | PRESENT;

        unsafe {
            let l4 = self.table_mut(cr3);
            let e4 = read_volatile(&l4[p4]);
            if e4 & PRESENT == 0 {
                return Err(MapError::AlreadyMapped);
            }

            let l3 = self.table_mut(e4 & ENTRY_ADDR_MASK);
            let e3 = read_volatile(&l3[p3]);
            if e3 & PRESENT == 0 || e3 & HUGE != 0 {
                return Err(MapError::HugePageConflict);
            }

            let l2 = self.table_mut(e3 & ENTRY_ADDR_MASK);
            let e2 = read_volatile(&l2[p2]);
            if e2 & PRESENT == 0 || e2 & HUGE != 0 {
                return Err(MapError::HugePageConflict);
            }

            let l1 = self.table_mut(e2 & ENTRY_ADDR_MASK);
            let entry = read_volatile(&l1[p1]);
            if entry & PRESENT == 0 {
                return Err(MapError::AlreadyMapped);
            }

            let physical = entry & ENTRY_ADDR_MASK;
            write_volatile(&mut l1[p1], physical | leaf_bits);
            invlpg(virtual_address);
        }

        Ok(())
    }
}

pub fn current_cr3() -> u64 {
    read_cr3() & ENTRY_ADDR_MASK
}

#[inline(always)]
pub unsafe fn switch_cr3(root: u64) {
    core::arch::asm!(
        "mov cr3, {0}",
        in(reg) root & ENTRY_ADDR_MASK,
        options(nostack, preserves_flags)
    );
}

#[inline(always)]
pub fn create_empty_user_address_space(
    allocator: &mut crate::memory::FrameAllocator<'_>,
    physical_offset: u64,
) -> Option<u64> {
    let current = read_cr3() & ENTRY_ADDR_MASK;
    let frame = allocator.allocate()?;
    let root = frame.addr();

    let mapper = PageTableManager::new(physical_offset);

    unsafe {
        mapper.zero_table(root);

        let old_table = mapper.table_mut(current);
        let new_table = mapper.table_mut(root);

        let mut index = 0usize;

        while index < 512 {
            let value = read_volatile(&old_table[index]);

            if value & PRESENT != 0 && value & USER == 0 {
                write_volatile(&mut new_table[index], value);
            }

            index += 1;
        }
    }

    Some(root)
}
