use super::btrfs::{self, BtrfsInfo};
use super::pci::PciDevice;
use crate::console::Serial;
use crate::memory::FrameAllocator;
use crate::storage::vfs;
use core::fmt::Write;
use core::ptr::{read_volatile, write_bytes, write_volatile};
use core::sync::atomic::{Ordering, fence};

const PAGE_SIZE: usize = 4096;
const PAGE_SIZE_U64: u64 = PAGE_SIZE as u64;
const QUEUE_DEPTH: u16 = 16;
const ADMIN_DB_BASE: usize = 0x1000;
const REG_CAP: usize = 0x00;
const REG_CC: usize = 0x14;
const REG_CSTS: usize = 0x1C;
const REG_AQA: usize = 0x24;
const REG_ASQ: usize = 0x28;
const REG_ACQ: usize = 0x30;
const OP_ADMIN_CREATE_SQ: u8 = 0x01;
const OP_ADMIN_CREATE_CQ: u8 = 0x05;
const OP_ADMIN_IDENTIFY: u8 = 0x06;
const OP_IO_FLUSH: u8 = 0x00;
const OP_IO_WRITE: u8 = 0x01;
const OP_IO_READ: u8 = 0x02;
const OP_IO_WRITE_ZEROES: u8 = 0x08;
const PHASE_INITIAL: u16 = 1;
const WAIT_SPINS: usize = 20_000_000;
// CORRECT per NVMe spec: CNS 1 = controller, 0 = namespace
const IDENTIFY_CNS_CTRL: u32 = 1;
const IDENTIFY_CNS_NS: u32 = 0;
const SQ_ENTRY_BYTES: usize = 64;
const CQ_ENTRY_BYTES: usize = 16;

#[derive(Clone, Copy)]
pub struct StorageReport {
    pub vendor: u16,
    pub device_id: u16,
    pub model: [u8; 40],
    pub model_len: usize,
    pub serial: [u8; 20],
    pub serial_len: usize,
    pub namespace_bytes: u64,
    pub lba_bytes: u64,
    pub btrfs: Option<BtrfsInfo>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NvmeError {
    Timeout,
    Status { status_code: u8, status_type: u8 },
    Invalid,
    OutOfRange,
    BufferTooSmall,
}
impl NvmeError {
    fn report<W: Write>(&self, out: &mut W) {
        match self {
            Self::Timeout => write!(out, "timeout").ok(),
            Self::Invalid => write!(out, "invalid controller state").ok(),
            Self::OutOfRange => write!(out, "LBA/count out of range").ok(),
            Self::BufferTooSmall => write!(out, "buffer too small").ok(),
            Self::Status {
                status_code,
                status_type,
            } => write!(
                out,
                "status SCT=0x{:02X} SC=0x{:02X}",
                status_type, status_code
            )
            .ok(),
        };
    }
}

pub struct Nvme {
    mmio: usize,
    doorbell_stride: usize,
    admin_sq_phys: u64,
    admin_cq_phys: u64,
    io_sq_phys: u64,
    io_cq_phys: u64,
    data_phys: u64,
    admin_sq: *mut u8,
    admin_cq: *mut u8,
    io_sq: *mut u8,
    io_cq: *mut u8,
    data: *mut u8,
    admin_tail: u16,
    admin_head: u16,
    admin_phase: u16,
    io_tail: u16,
    io_head: u16,
    io_phase: u16,
    next_cid: u16,
    queue_depth: u16,
    pub lba_bytes: u64,
    pub namespace_bytes: u64,
}
unsafe impl Send for Nvme {}
unsafe impl Sync for Nvme {}

impl Nvme {
    pub fn probe(
        pci: PciDevice,
        phys_offset: u64,
        allocator: &mut FrameAllocator<'_>,
        out: &mut Serial,
    ) -> Option<StorageReport> {
        writeln!(
            out,
            "NVMe {:02X}:{:02X}.{} vendor={:04X} device={:04X}",
            pci.bus, pci.device, pci.function, pci.vendor, pci.device_id,
        )
        .ok();
        writeln!(out, "BAR0              : 0x{:016X}", pci.bar0).ok();
        let mut nvme = match Self::new(&pci, phys_offset, allocator) {
            Ok(v) => v,
            Err(e) => {
                write!(out, "NVMe INIT FAILED   : ").ok();
                e.report(out);
                writeln!(out).ok();
                return None;
            }
        };
        let (model, model_len, serial, serial_len) = match nvme.identify_controller() {
            Ok(v) => v,
            Err(e) => {
                write!(out, "IDENTIFY FAILED    : ").ok();
                e.report(out);
                writeln!(out).ok();
                return None;
            }
        };
        write!(out, "model              : ").ok();
        print_bytes(out, &model[..model_len]);
        writeln!(out).ok();
        write!(out, "serial             : ").ok();
        print_bytes(out, &serial[..serial_len]);
        writeln!(out).ok();
        let (namespace_bytes, lba_bytes) = match nvme.identify_namespace() {
            Ok(v) => v,
            Err(e) => {
                write!(out, "NS IDENTIFY FAILED : ").ok();
                e.report(out);
                writeln!(out).ok();
                return None;
            }
        };
        nvme.namespace_bytes = namespace_bytes;
        nvme.lba_bytes = lba_bytes;
        writeln!(
            out,
            "namespace          : {} MiB",
            namespace_bytes / 1024 / 1024
        )
        .ok();
        writeln!(out, "LBA size           : {} bytes", lba_bytes).ok();
        writeln!(out, "capacity (bytes)   : {}", namespace_bytes).ok();
        let report_no_btrfs = |model, model_len, serial, serial_len| StorageReport {
            vendor: pci.vendor,
            device_id: pci.device_id,
            model,
            model_len,
            serial,
            serial_len,
            namespace_bytes,
            lba_bytes,
            btrfs: None,
        };
        if lba_bytes == 0 || PAGE_SIZE_U64 % lba_bytes != 0 {
            writeln!(out, "Btrfs              : unsupported LBA size").ok();
            return Some(report_no_btrfs(model, model_len, serial, serial_len));
        }
        let blocks = (PAGE_SIZE_U64 / lba_bytes) as u16;
        let lba = btrfs::SUPERBLOCK_OFFSET / lba_bytes;
        if let Err(e) = nvme.read_blocks(lba, blocks) {
            write!(out, "Btrfs READ FAILED  : ").ok();
            e.report(out);
            writeln!(out).ok();
            return Some(report_no_btrfs(model, model_len, serial, serial_len));
        }
        let block = unsafe { core::slice::from_raw_parts(nvme.data, PAGE_SIZE) };
        let btrfs_info = btrfs::parse(block);
        if let Some(info) = btrfs_info {
            btrfs::report(&info, out);
        } else {
            writeln!(out, "Btrfs              : NOT DETECTED").ok();
        }
        vfs::attach_raw(&mut nvme as *mut _);
        super::vfs::attach(nvme, namespace_bytes, lba_bytes, out);
        Some(StorageReport {
            vendor: pci.vendor,
            device_id: pci.device_id,
            model,
            model_len,
            serial,
            serial_len,
            namespace_bytes,
            lba_bytes,
            btrfs: btrfs_info,
        })
    }

    pub fn probe_secondary(
        pci: super::pci::PciDevice,
        phys_offset: u64,
        allocator: &mut FrameAllocator<'_>,
        out: &mut Serial,
    ) -> Option<(StorageReport, Self)> {
        let mut nvme = match Self::new(&pci, phys_offset, allocator) {
            Ok(v) => v,
            Err(_) => return None,
        };
        let (model, ml, serial, sl) = match nvme.identify_controller() {
            Ok(v) => v,
            Err(_) => return None,
        };
        let (ns_bytes, lba) = match nvme.identify_namespace() {
            Ok(v) => v,
            Err(_) => return None,
        };
        nvme.namespace_bytes = ns_bytes;
        nvme.lba_bytes = lba;
        let rep = StorageReport {
            vendor: pci.vendor,
            device_id: pci.device_id,
            model,
            model_len: ml,
            serial,
            serial_len: sl,
            namespace_bytes: ns_bytes,
            lba_bytes: lba,
            btrfs: None,
        };
        Some((rep, nvme))
    }

    fn new(
        pci: &PciDevice,
        phys_offset: u64,
        allocator: &mut FrameAllocator<'_>,
    ) -> Result<Self, NvmeError> {
        let mmio = phys_offset
            .checked_add(pci.bar0)
            .ok_or(NvmeError::Invalid)? as usize;
        let admin_sq = allocator.allocate().ok_or(NvmeError::Invalid)?;
        let admin_cq = allocator.allocate().ok_or(NvmeError::Invalid)?;
        let io_sq = allocator.allocate().ok_or(NvmeError::Invalid)?;
        let io_cq = allocator.allocate().ok_or(NvmeError::Invalid)?;
        let data = allocator.allocate().ok_or(NvmeError::Invalid)?;
        let admin_sq_virt = phys_to_virt(phys_offset, admin_sq.addr())?;
        let admin_cq_virt = phys_to_virt(phys_offset, admin_cq.addr())?;
        let io_sq_virt = phys_to_virt(phys_offset, io_sq.addr())?;
        let io_cq_virt = phys_to_virt(phys_offset, io_cq.addr())?;
        let data_virt = phys_to_virt(phys_offset, data.addr())?;
        unsafe {
            clear_page(admin_sq_virt);
            clear_page(admin_cq_virt);
            clear_page(io_sq_virt);
            clear_page(io_cq_virt);
            clear_page(data_virt);
        }
        let cap = unsafe { read_volatile((mmio + REG_CAP) as *const u64) };
        let max_entries = ((cap & 0xFFFF) as u16).saturating_add(1);
        let queue_depth = max_entries.min(QUEUE_DEPTH);
        if queue_depth < 2 {
            return Err(NvmeError::Invalid);
        }
        let mps_min = ((cap >> 48) & 0xF) as u8;
        if mps_min != 0 {
            return Err(NvmeError::Invalid);
        }
        let dstrd = ((cap >> 32) & 0xF) as u32;
        let doorbell_stride = 4usize.checked_shl(dstrd).ok_or(NvmeError::Invalid)?;
        let mut nvme = Self {
            mmio,
            doorbell_stride,
            admin_sq_phys: admin_sq.addr(),
            admin_cq_phys: admin_cq.addr(),
            io_sq_phys: io_sq.addr(),
            io_cq_phys: io_cq.addr(),
            data_phys: data.addr(),
            admin_sq: admin_sq_virt,
            admin_cq: admin_cq_virt,
            io_sq: io_sq_virt,
            io_cq: io_cq_virt,
            data: data_virt,
            admin_tail: 0,
            admin_head: 0,
            admin_phase: PHASE_INITIAL,
            io_tail: 0,
            io_head: 0,
            io_phase: PHASE_INITIAL,
            next_cid: 1,
            queue_depth,
            lba_bytes: 512,
            namespace_bytes: 0,
        };
        nvme.disable()?;
        nvme.configure_admin();
        nvme.enable()?;
        nvme.create_io_queues()?;
        Ok(nvme)
    }
    fn disable(&mut self) -> Result<(), NvmeError> {
        let cc = self.read32(REG_CC);
        if cc & 1 == 0 {
            return Ok(());
        }
        self.write32(REG_CC, cc & !1);
        for _ in 0..WAIT_SPINS {
            if self.read32(REG_CSTS) & 1 == 0 {
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(NvmeError::Timeout)
    }
    fn configure_admin(&mut self) {
        let qsize = (self.queue_depth - 1) as u32;
        self.write32(REG_AQA, qsize | (qsize << 16));
        self.write64(REG_ASQ, self.admin_sq_phys);
        self.write64(REG_ACQ, self.admin_cq_phys);
    }
    fn enable(&mut self) -> Result<(), NvmeError> {
        let cc = 1u32 | (6u32 << 16) | (4u32 << 20);
        self.write32(REG_CC, cc);
        for _ in 0..WAIT_SPINS {
            if self.read32(REG_CSTS) & 1 != 0 {
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(NvmeError::Timeout)
    }
    fn create_io_queues(&mut self) -> Result<(), NvmeError> {
        let qsize = (self.queue_depth - 1) as u32;
        let cq_cdw10 = 1u32 | (qsize << 16);
        self.submit_admin(OP_ADMIN_CREATE_CQ, 0, self.io_cq_phys, cq_cdw10, 1, 0)?;
        let sq_cdw10 = 1u32 | (qsize << 16);
        let sq_cdw11 = 1u32 | (1u32 << 16);
        self.submit_admin(
            OP_ADMIN_CREATE_SQ,
            0,
            self.io_sq_phys,
            sq_cdw10,
            sq_cdw11,
            0,
        )?;
        Ok(())
    }
    fn identify_controller(&mut self) -> Result<([u8; 40], usize, [u8; 20], usize), NvmeError> {
        unsafe { clear_page(self.data) };
        self.submit_admin(
            OP_ADMIN_IDENTIFY,
            0,
            self.data_phys,
            IDENTIFY_CNS_CTRL,
            0,
            0,
        )?;
        let data = unsafe { core::slice::from_raw_parts(self.data, PAGE_SIZE) };
        let mut model = [b' '; 40];
        let mut serial = [b' '; 20];
        model.copy_from_slice(&data[24..64]);
        serial.copy_from_slice(&data[4..24]);
        let model_len = trim(&mut model);
        let serial_len = trim(&mut serial);
        Ok((model, model_len, serial, serial_len))
    }
    fn identify_namespace(&mut self) -> Result<(u64, u64), NvmeError> {
        unsafe { clear_page(self.data) };
        self.submit_admin(OP_ADMIN_IDENTIFY, 1, self.data_phys, IDENTIFY_CNS_NS, 0, 0)?;
        let data = unsafe { core::slice::from_raw_parts(self.data, PAGE_SIZE) };
        let nsze = le64(data, 0);
        let flbas = data[26];
        let format_index = (flbas & 0x0F) as usize;
        let offset = 128usize + format_index * 4;
        if offset + 4 > data.len() {
            return Err(NvmeError::Invalid);
        }
        let lbads = data[offset + 2];
        if lbads == 0 || lbads >= 63 {
            return Err(NvmeError::Invalid);
        }
        let lba_bytes = 1u64 << lbads;
        let capacity = nsze.checked_mul(lba_bytes).ok_or(NvmeError::Invalid)?;
        Ok((capacity, lba_bytes))
    }
    #[inline(always)]
    fn read_blocks(&mut self, lba: u64, count: u16) -> Result<(), NvmeError> {
        self.io_transfer(OP_IO_READ, lba, count, true)
    }
    #[inline(always)]
    fn write_blocks(&mut self, lba: u64, count: u16) -> Result<(), NvmeError> {
        self.io_transfer(OP_IO_WRITE, lba, count, false)
    }
    fn io_transfer(
        &mut self,
        opcode: u8,
        lba: u64,
        count: u16,
        read: bool,
    ) -> Result<(), NvmeError> {
        if count == 0 {
            return Err(NvmeError::Invalid);
        }
        if read {
            unsafe { clear_page(self.data) };
        }
        let slot = self.io_tail as usize;
        let cid = self.next_cid;
        let command = unsafe { self.io_sq.add(slot * SQ_ENTRY_BYTES) as *mut u32 };
        unsafe {
            write_bytes(command.cast::<u8>(), 0, SQ_ENTRY_BYTES);
            write_volatile(command, (opcode as u32) | ((cid as u32) << 16));
            write_volatile(command.add(1), 1);
            write_volatile(command.add(6), self.data_phys as u32);
            write_volatile(command.add(7), (self.data_phys >> 32) as u32);
            write_volatile(command.add(10), lba as u32);
            write_volatile(command.add(11), (lba >> 32) as u32);
            write_volatile(command.add(12), (count as u32) - 1);
        }
        fence(Ordering::Release);
        self.io_tail = (self.io_tail + 1) % self.queue_depth;
        self.write32(self.db(2), self.io_tail as u32);
        self.wait_io(cid)?;
        self.next_cid = self.next_cid.wrapping_add(1).max(1);
        Ok(())
    }
    fn submit_admin(
        &mut self,
        opcode: u8,
        nsid: u32,
        prp1: u64,
        cdw10: u32,
        cdw11: u32,
        cdw12: u32,
    ) -> Result<(), NvmeError> {
        let slot = self.admin_tail as usize;
        let cid = self.next_cid;
        let command = unsafe { self.admin_sq.add(slot * SQ_ENTRY_BYTES) as *mut u32 };
        unsafe {
            write_bytes(command.cast::<u8>(), 0, SQ_ENTRY_BYTES);
            write_volatile(command, (opcode as u32) | ((cid as u32) << 16));
            write_volatile(command.add(1), nsid);
            write_volatile(command.add(6), prp1 as u32);
            write_volatile(command.add(7), (prp1 >> 32) as u32);
            write_volatile(command.add(10), cdw10);
            write_volatile(command.add(11), cdw11);
            write_volatile(command.add(12), cdw12);
        }
        fence(Ordering::Release);
        self.admin_tail = (self.admin_tail + 1) % self.queue_depth;
        self.write32(self.db(0), self.admin_tail as u32);
        self.wait_admin(cid)?;
        self.next_cid = self.next_cid.wrapping_add(1).max(1);
        Ok(())
    }
    #[inline]
    fn wait_admin(&mut self, cid: u16) -> Result<(), NvmeError> {
        self.wait_cq(cid, true)
    }
    #[inline]
    fn wait_io(&mut self, cid: u16) -> Result<(), NvmeError> {
        self.wait_cq(cid, false)
    }
    fn wait_cq(&mut self, cid: u16, admin: bool) -> Result<(), NvmeError> {
        let (base, mut head, mut phase) = if admin {
            (self.admin_cq, self.admin_head, self.admin_phase)
        } else {
            (self.io_cq, self.io_head, self.io_phase)
        };
        for _ in 0..WAIT_SPINS {
            let entry = unsafe { base.add(head as usize * CQ_ENTRY_BYTES) as *const u32 };
            let dw3 = unsafe { read_volatile(entry.add(3)) };
            let raw_status = (dw3 >> 16) as u16;
            if raw_status & 1 != phase {
                core::hint::spin_loop();
                continue;
            }
            let got_cid = dw3 as u16;
            let status_code = ((raw_status >> 1) & 0xFF) as u8;
            let status_type = ((raw_status >> 9) & 0x7) as u8;
            head += 1;
            if head >= self.queue_depth {
                head = 0;
                phase ^= 1;
            }
            if admin {
                self.admin_head = head;
                self.admin_phase = phase;
                fence(Ordering::Release);
                self.write32(self.db(1), head as u32);
            } else {
                self.io_head = head;
                self.io_phase = phase;
                fence(Ordering::Release);
                self.write32(self.db(3), head as u32);
            }
            if got_cid != cid {
                return Err(NvmeError::Invalid);
            }
            if status_code != 0 {
                return Err(NvmeError::Status {
                    status_code,
                    status_type,
                });
            }
            return Ok(());
        }
        Err(NvmeError::Timeout)
    }
    #[inline(always)]
    fn db(&self, index: usize) -> usize {
        ADMIN_DB_BASE + index * self.doorbell_stride
    }
    #[inline(always)]
    fn read32(&self, offset: usize) -> u32 {
        unsafe { read_volatile((self.mmio + offset) as *const u32) }
    }
    #[inline(always)]
    fn write32(&self, offset: usize, value: u32) {
        unsafe { write_volatile((self.mmio + offset) as *mut u32, value) }
    }
    #[inline(always)]
    fn write64(&self, offset: usize, value: u64) {
        unsafe { write_volatile((self.mmio + offset) as *mut u64, value) }
    }
    #[inline(always)]
    pub fn capacity_bytes(&self) -> u64 {
        self.namespace_bytes
    }
    #[inline(always)]
    pub fn lba_size(&self) -> u64 {
        self.lba_bytes
    }
    #[inline(always)]
    fn blocks_per_page(&self) -> u64 {
        PAGE_SIZE_U64 / self.lba_bytes
    }
    pub(crate) fn read_4k(&mut self, lba: u64, out: &mut [u8; PAGE_SIZE]) -> Result<(), NvmeError> {
        if self.lba_bytes == 0 || PAGE_SIZE_U64 % self.lba_bytes != 0 {
            return Err(NvmeError::Invalid);
        }
        let count = self.blocks_per_page() as u16;
        self.read_blocks(lba, count)?;
        unsafe {
            core::ptr::copy_nonoverlapping(self.data as *const u8, out.as_mut_ptr(), PAGE_SIZE);
        }
        Ok(())
    }
    pub(crate) fn write_4k(&mut self, lba: u64, input: &[u8; PAGE_SIZE]) -> Result<(), NvmeError> {
        if self.lba_bytes == 0 || PAGE_SIZE_U64 % self.lba_bytes != 0 {
            return Err(NvmeError::Invalid);
        }
        let count = self.blocks_per_page() as u16;
        unsafe {
            core::ptr::copy_nonoverlapping(input.as_ptr(), self.data, PAGE_SIZE);
        }
        self.write_blocks(lba, count)
    }
    pub fn flush(&mut self) -> Result<(), NvmeError> {
        let slot = self.io_tail as usize;
        let cid = self.next_cid;
        let command = unsafe { self.io_sq.add(slot * SQ_ENTRY_BYTES) as *mut u32 };
        unsafe {
            write_bytes(command.cast::<u8>(), 0, SQ_ENTRY_BYTES);
            write_volatile(command, (OP_IO_FLUSH as u32) | ((cid as u32) << 16));
            write_volatile(command.add(1), 1);
        }
        fence(Ordering::Release);
        self.io_tail = (self.io_tail + 1) % self.queue_depth;
        self.write32(self.db(2), self.io_tail as u32);
        self.wait_io(cid)?;
        self.next_cid = self.next_cid.wrapping_add(1).max(1);
        Ok(())
    }
    pub fn write_zeroes(&mut self, lba: u64, count: u16) -> Result<(), NvmeError> {
        if count == 0 {
            return Err(NvmeError::Invalid);
        }
        let slot = self.io_tail as usize;
        let cid = self.next_cid;
        let command = unsafe { self.io_sq.add(slot * SQ_ENTRY_BYTES) as *mut u32 };
        unsafe {
            write_bytes(command.cast::<u8>(), 0, SQ_ENTRY_BYTES);
            write_volatile(command, (OP_IO_WRITE_ZEROES as u32) | ((cid as u32) << 16));
            write_volatile(command.add(1), 1);
            write_volatile(command.add(10), lba as u32);
            write_volatile(command.add(11), (lba >> 32) as u32);
            write_volatile(command.add(12), (count as u32) - 1);
        }
        fence(Ordering::Release);
        self.io_tail = (self.io_tail + 1) % self.queue_depth;
        self.write32(self.db(2), self.io_tail as u32);
        self.wait_io(cid)?;
        self.next_cid = self.next_cid.wrapping_add(1).max(1);
        Ok(())
    }
    pub fn read_range(&mut self, byte_offset: u64, buf: &mut [u8]) -> Result<(), NvmeError> {
        if self.lba_bytes == 0 || PAGE_SIZE_U64 % self.lba_bytes != 0 {
            return Err(NvmeError::Invalid);
        }
        if buf.is_empty() {
            return Ok(());
        }
        let end = byte_offset
            .checked_add(buf.len() as u64)
            .ok_or(NvmeError::OutOfRange)?;
        if end > self.namespace_bytes {
            return Err(NvmeError::OutOfRange);
        }
        let bpp = self.blocks_per_page();
        let mut offset = byte_offset;
        let mut dst = 0usize;
        let mut remaining = buf.len();
        let mut page = [0u8; PAGE_SIZE];
        while remaining > 0 {
            let page_index = offset / PAGE_SIZE_U64;
            let page_offset = (offset % PAGE_SIZE_U64) as usize;
            self.read_4k(page_index * bpp, &mut page)?;
            let n = core::cmp::min(PAGE_SIZE - page_offset, remaining);
            buf[dst..dst + n].copy_from_slice(&page[page_offset..page_offset + n]);
            offset += n as u64;
            dst += n;
            remaining -= n;
        }
        Ok(())
    }
    pub fn write_range(&mut self, byte_offset: u64, buf: &[u8]) -> Result<(), NvmeError> {
        if self.lba_bytes == 0 || PAGE_SIZE_U64 % self.lba_bytes != 0 {
            return Err(NvmeError::Invalid);
        }
        if buf.is_empty() {
            return Ok(());
        }
        let end = byte_offset
            .checked_add(buf.len() as u64)
            .ok_or(NvmeError::OutOfRange)?;
        if end > self.namespace_bytes {
            return Err(NvmeError::OutOfRange);
        }
        let bpp = self.blocks_per_page();
        let mut offset = byte_offset;
        let mut src = 0usize;
        let mut remaining = buf.len();
        let mut page = [0u8; PAGE_SIZE];
        while remaining > 0 {
            let page_index = offset / PAGE_SIZE_U64;
            let page_offset = (offset % PAGE_SIZE_U64) as usize;
            let lba = page_index * bpp;
            let n = core::cmp::min(PAGE_SIZE - page_offset, remaining);
            if page_offset == 0 && n == PAGE_SIZE {
                page.copy_from_slice(&buf[src..src + n]);
            } else {
                self.read_4k(lba, &mut page)?;
                page[page_offset..page_offset + n].copy_from_slice(&buf[src..src + n]);
            }
            self.write_4k(lba, &page)?;
            offset += n as u64;
            src += n;
            remaining -= n;
        }
        Ok(())
    }
    pub fn zero_range(&mut self, byte_offset: u64, length: u64) -> Result<(), NvmeError> {
        if length == 0 {
            return Ok(());
        }
        let end = byte_offset
            .checked_add(length)
            .ok_or(NvmeError::OutOfRange)?;
        if end > self.namespace_bytes {
            return Err(NvmeError::OutOfRange);
        }
        if self.lba_bytes == 0 || PAGE_SIZE_U64 % self.lba_bytes != 0 {
            return Err(NvmeError::Invalid);
        }
        let bpp = self.blocks_per_page();
        let mut offset = byte_offset;
        let mut remaining = length;
        while remaining > 0 {
            let page_index = offset / PAGE_SIZE_U64;
            let page_offset = (offset % PAGE_SIZE_U64) as usize;
            let lba = page_index * bpp;
            if page_offset == 0 && remaining >= PAGE_SIZE_U64 {
                self.write_zeroes(lba, bpp as u16)?;
                offset += PAGE_SIZE_U64;
                remaining -= PAGE_SIZE_U64;
            } else {
                let mut page = [0u8; PAGE_SIZE];
                self.read_4k(lba, &mut page)?;
                let n = core::cmp::min(PAGE_SIZE - page_offset, remaining as usize);
                for byte in &mut page[page_offset..page_offset + n] {
                    *byte = 0;
                }
                self.write_4k(lba, &page)?;
                offset += n as u64;
                remaining -= n as u64;
            }
        }
        Ok(())
    }
    pub fn read_blocks_into(
        &mut self,
        lba: u64,
        count: u16,
        buf: &mut [u8],
    ) -> Result<(), NvmeError> {
        if count == 0 {
            return Ok(());
        }
        if self.lba_bytes == 0 {
            return Err(NvmeError::Invalid);
        }
        let bytes_needed = (count as u64)
            .checked_mul(self.lba_bytes)
            .ok_or(NvmeError::OutOfRange)?;
        if (buf.len() as u64) < bytes_needed {
            return Err(NvmeError::BufferTooSmall);
        }
        let bpp = self.blocks_per_page() as u16;
        let mut current_lba = lba;
        let mut remaining_blocks = count;
        let mut dst = 0usize;
        while remaining_blocks > 0 {
            let chunk = remaining_blocks.min(bpp);
            self.read_blocks(current_lba, chunk)?;
            let bytes = chunk as usize * self.lba_bytes as usize;
            unsafe {
                core::ptr::copy_nonoverlapping(self.data, buf.as_mut_ptr().add(dst), bytes);
            }
            dst += bytes;
            current_lba += chunk as u64;
            remaining_blocks -= chunk;
        }
        Ok(())
    }
    pub fn write_blocks_from(&mut self, lba: u64, count: u16, buf: &[u8]) -> Result<(), NvmeError> {
        if count == 0 {
            return Ok(());
        }
        if self.lba_bytes == 0 {
            return Err(NvmeError::Invalid);
        }
        let bytes_needed = (count as u64)
            .checked_mul(self.lba_bytes)
            .ok_or(NvmeError::OutOfRange)?;
        if (buf.len() as u64) < bytes_needed {
            return Err(NvmeError::BufferTooSmall);
        }
        let bpp = self.blocks_per_page() as u16;
        let mut current_lba = lba;
        let mut remaining_blocks = count;
        let mut src = 0usize;
        while remaining_blocks > 0 {
            let chunk = remaining_blocks.min(bpp);
            let bytes = chunk as usize * self.lba_bytes as usize;
            unsafe {
                core::ptr::copy_nonoverlapping(buf.as_ptr().add(src), self.data, bytes);
            }
            self.write_blocks(current_lba, chunk)?;
            src += bytes;
            current_lba += chunk as u64;
            remaining_blocks -= chunk;
        }
        Ok(())
    }
}
#[inline(always)]
fn phys_to_virt(offset: u64, physical: u64) -> Result<*mut u8, NvmeError> {
    let va = offset.checked_add(physical).ok_or(NvmeError::Invalid)?;
    Ok(va as *mut u8)
}
#[inline(always)]
unsafe fn clear_page(page: *mut u8) {
    unsafe { write_bytes(page, 0, PAGE_SIZE) };
}
#[inline(always)]
fn trim(data: &mut [u8]) -> usize {
    let mut end = data.len();
    while end > 0 {
        let byte = data[end - 1];
        if byte == 0 || byte == b' ' {
            end -= 1;
        } else {
            break;
        }
    }
    end
}
fn print_bytes(out: &mut Serial, data: &[u8]) {
    for &byte in data {
        let c = if (0x20..0x7F).contains(&byte) {
            byte as char
        } else {
            ' '
        };
        out.write_char(c).ok();
    }
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
impl Nvme {
    pub fn capacity_lba(&self) -> u64 {
        self.namespace_bytes / self.lba_bytes
    }
}
