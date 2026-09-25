#![allow(static_mut_refs)]
use core::fmt::Write;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::arch::x86_64::paging;
use crate::arch::x86_64::syscall::{self, SyscallFrame};
use crate::memory::FrameAllocator;

const ENOENT: u64 = 2;
const ECHILD: u64 = 10;
const EFAULT: u64 = 14;
const EINVAL: u64 = 22;
const ENOSYS: u64 = 38;
const EAGAIN: u64 = 11;
const ENOMEM: u64 = 12;

const MAX_ARGS: usize = 32;
const MAX_ARG_LEN: usize = 256;
const MAX_CHILDREN: usize = 16;

const TLS_BASE: u64 = 0x0000_5FFF_0000_0000;

const VFORK_STACK_TOP: u64 = 0x0000_7F00_0000_0000;
const VFORK_STACK_BYTES: usize = 64 * 4096;
const VFORK_STACK_BOTTOM: u64 = VFORK_STACK_TOP - VFORK_STACK_BYTES as u64;

static NEXT_PID: AtomicU64 = AtomicU64::new(2);

#[unsafe(no_mangle)]
pub static mut vibux_vfork_child_active: u8 = 0;
#[unsafe(no_mangle)]
pub static mut vibux_vfork_child_pid: u64 = 0;
#[unsafe(no_mangle)]
pub static mut vibux_vfork_child_exit_code: u64 = 0;
#[unsafe(no_mangle)]
pub static mut vibux_vfork_child_status: u32 = 0;
#[unsafe(no_mangle)]
pub static mut vibux_vfork_child_valid: u8 = 0;
#[unsafe(no_mangle)]
pub static mut vibux_vfork_parent_cr3: u64 = 0;
#[unsafe(no_mangle)]
pub static mut vibux_vfork_parent_fs_base: u64 = 0;
#[unsafe(no_mangle)]
pub static mut vibux_vfork_parent_stack_shared: u8 = 0;
#[unsafe(no_mangle)]
pub static mut vibux_vfork_stack_snapshot_valid: u8 = 0;
#[unsafe(no_mangle)]
pub static mut vibux_vfork_tls_snapshot_valid: u8 = 0;

static mut VFORK_STACK_SNAPSHOT: [u8; VFORK_STACK_BYTES] = [0; VFORK_STACK_BYTES];
static mut VFORK_TLS_SNAPSHOT: [u8; 4096] = [0; 4096];

static mut PARENT_STATE: MaybeUninit<syscall::ProcessState> = MaybeUninit::uninit();

#[derive(Clone, Copy)]
struct ChildEntry {
    used: bool,
    pid: u64,
    status: u32,
}

impl ChildEntry {
    const fn empty() -> Self {
        Self {
            used: false,
            pid: 0,
            status: 0,
        }
    }
}

static mut CHILDREN: [ChildEntry; MAX_CHILDREN] = [ChildEntry::empty(); MAX_CHILDREN];

#[inline(always)]
fn push_exited_child(pid: u64, status: u32) {
    unsafe {
        let table = core::ptr::addr_of_mut!(CHILDREN) as *mut ChildEntry;
        for i in 0..MAX_CHILDREN {
            let slot = table.add(i);
            if !(*slot).used {
                (*slot).used = true;
                (*slot).pid = pid;
                (*slot).status = status;
                return;
            }
        }
    }
}

#[inline]
fn take_exited_child(requested_pid: i32) -> Option<ChildEntry> {
    unsafe {
        let table = core::ptr::addr_of_mut!(CHILDREN) as *mut ChildEntry;
        for i in 0..MAX_CHILDREN {
            let slot = table.add(i);
            if !(*slot).used {
                continue;
            }
            if requested_pid <= 0 || (*slot).pid == requested_pid as u64 {
                let entry = *slot;
                *slot = ChildEntry::empty();
                return Some(entry);
            }
        }
    }
    None
}

#[derive(Clone, Copy)]
struct UserString {
    bytes: [u8; MAX_ARG_LEN],
    len: usize,
}

impl UserString {
    const fn empty() -> Self {
        Self {
            bytes: [0; MAX_ARG_LEN],
            len: 0,
        }
    }
    fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

#[inline(always)]
unsafe fn parent_state() -> &'static syscall::ProcessState {
    unsafe { (&*core::ptr::addr_of!(PARENT_STATE)).assume_init_ref() }
}

#[inline(always)]
unsafe fn parent_state_mut() -> &'static mut syscall::ProcessState {
    unsafe { (&mut *core::ptr::addr_of_mut!(PARENT_STATE)).assume_init_mut() }
}

#[cfg(debug_assertions)]
fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3u64);
    }
    hash
}

fn snapshot_vfork_shared_memory(parent_rsp: u64) {
    unsafe {
        vibux_vfork_parent_stack_shared = 0;
        vibux_vfork_stack_snapshot_valid = 0;
        vibux_vfork_tls_snapshot_valid = 0;
    }

    if parent_rsp < VFORK_STACK_BOTTOM || parent_rsp >= VFORK_STACK_TOP {
        return;
    }

    let paging = paging::PageTableManager::new(syscall::process_physical_offset());

    let mut page = VFORK_STACK_BOTTOM;
    while page < VFORK_STACK_TOP {
        if paging.translate(page).is_none() {
            return;
        }
        page += 4096;
    }

    unsafe {
        core::ptr::copy_nonoverlapping(
            VFORK_STACK_BOTTOM as *const u8,
            core::ptr::addr_of_mut!(VFORK_STACK_SNAPSHOT).cast::<u8>(),
            VFORK_STACK_BYTES,
        );
        vibux_vfork_stack_snapshot_valid = 1;
        vibux_vfork_parent_stack_shared = 1;

        if paging.translate(TLS_BASE).is_some() {
            core::ptr::copy_nonoverlapping(
                TLS_BASE as *const u8,
                core::ptr::addr_of_mut!(VFORK_TLS_SNAPSHOT).cast::<u8>(),
                4096,
            );
            vibux_vfork_tls_snapshot_valid = 1;
        }
    }

    #[cfg(debug_assertions)]
    {
        let stack_hash = unsafe {
            hash_bytes(core::slice::from_raw_parts(
                core::ptr::addr_of!(VFORK_STACK_SNAPSHOT).cast::<u8>(),
                VFORK_STACK_BYTES,
            ))
        };
        crate::println!(
            "[VFORK-STACK] snapshot ok rsp=0x{:016X} hash=0x{:016X}",
            parent_rsp,
            stack_hash
        );
    }
}

fn restore_vfork_shared_memory() {
    let stack_valid = unsafe { vibux_vfork_stack_snapshot_valid != 0 };
    let tls_valid = unsafe { vibux_vfork_tls_snapshot_valid != 0 };

    if stack_valid {
        unsafe {
            core::ptr::copy_nonoverlapping(
                core::ptr::addr_of!(VFORK_STACK_SNAPSHOT).cast::<u8>(),
                VFORK_STACK_BOTTOM as *mut u8,
                VFORK_STACK_BYTES,
            );
        }
    }

    if tls_valid {
        unsafe {
            core::ptr::copy_nonoverlapping(
                core::ptr::addr_of!(VFORK_TLS_SNAPSHOT).cast::<u8>(),
                TLS_BASE as *mut u8,
                4096,
            );
        }
    }

    unsafe {
        vibux_vfork_stack_snapshot_valid = 0;
        vibux_vfork_tls_snapshot_valid = 0;
        vibux_vfork_parent_stack_shared = 0;
    }
}

#[inline(always)]
fn copy_user_u64(address: u64) -> Result<u64, u64> {
    let mut bytes = [0u8; 8];
    syscall::copy_user_in(address, &mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

fn read_user_vector(pointer: u64, output: &mut [UserString; MAX_ARGS]) -> Result<usize, u64> {
    if pointer == 0 {
        return Err(EFAULT);
    }
    let mut count = 0usize;
    while count < MAX_ARGS {
        let entry = copy_user_u64(pointer.checked_add((count as u64) * 8).ok_or(EFAULT)?)?;
        if entry == 0 {
            return Ok(count);
        }
        let mut bytes = [0u8; MAX_ARG_LEN];
        let len = syscall::read_c_string(entry, &mut bytes)?;
        output[count] = UserString { bytes, len };
        count += 1;
    }
    Err(EINVAL)
}

pub fn do_fork(frame: *mut SyscallFrame, _flags: u64, child_stack: u64) -> u64 {
    unsafe {
        if vibux_vfork_child_active != 0 {
            return 0u64.wrapping_sub(EAGAIN);
        }

        let parent_cr3 = paging::current_cr3();
        let parent_fs = syscall::fs_base();

        syscall::snapshot_process_state_into(PARENT_STATE.as_mut_ptr());
        vibux_vfork_parent_cr3 = parent_cr3;
        vibux_vfork_parent_fs_base = parent_fs;

        let child_pid = NEXT_PID.fetch_add(1, Ordering::Relaxed);
        vibux_vfork_child_pid = child_pid;
        vibux_vfork_child_exit_code = 0;
        vibux_vfork_child_status = 0;
        vibux_vfork_child_valid = 0;
        vibux_vfork_child_active = 1;

        let parent_rsp = syscall::saved_user_rsp();
        let child_rsp = if child_stack != 0 {
            child_stack
        } else {
            parent_rsp
        };

        let child_cr3 = match crate::velf::with_process_allocator(|allocator, physical_offset| {
            let paging = paging::PageTableManager::new(physical_offset);
            paging.clone_user_address_space(parent_cr3, allocator)
        })
        .flatten()
        {
            Some(v) => v,
            None => {
                vibux_vfork_child_active = 0;
                return 0u64.wrapping_sub(ENOMEM);
            }
        };

        crate::println!(
            "[FORK] parent_cr3=0x{:016X} child_cr3=0x{:016X} pid=0x{:016X} rsp=0x{:016X}",
            parent_cr3,
            child_cr3,
            child_pid,
            child_rsp
        );

        syscall::set_pid(child_pid);
        paging::switch_cr3(child_cr3);

        let _exit = syscall::enter_vfork_child(frame, child_rsp);

        frame.as_mut().unwrap().rax = child_pid;
        child_pid
    }
}

pub fn do_vfork(frame: *mut SyscallFrame, _flags: u64, child_stack: u64) -> u64 {
    unsafe {
        if vibux_vfork_child_active != 0 {
            return 0u64.wrapping_sub(EAGAIN);
        }

        syscall::snapshot_process_state_into(PARENT_STATE.as_mut_ptr());
        vibux_vfork_parent_cr3 = paging::current_cr3();
        vibux_vfork_parent_fs_base = syscall::fs_base();

        let child_pid = NEXT_PID.fetch_add(1, Ordering::Relaxed);
        vibux_vfork_child_pid = child_pid;
        vibux_vfork_child_exit_code = 0;
        vibux_vfork_child_status = 0;
        vibux_vfork_child_valid = 0;
        vibux_vfork_child_active = 1;

        let parent_rsp = syscall::saved_user_rsp();
        let child_rsp = if child_stack != 0 {
            child_stack
        } else {
            parent_rsp
        };

        syscall::set_pid(child_pid);

        if child_stack == 0 {
            snapshot_vfork_shared_memory(parent_rsp);
        }

        let _exit_code = syscall::enter_vfork_child(frame, child_rsp);

        if child_stack == 0 {
            restore_vfork_shared_memory();
        }

        frame.as_mut().unwrap().rax = child_pid;
        child_pid
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn vibux_vfork_child_exit(code: u64) {
    unsafe {
        vibux_vfork_child_exit_code = code;
        vibux_vfork_child_status = ((code as u32) & 0xFF) << 8;
        vibux_vfork_child_valid = 1;

        if vibux_vfork_parent_cr3 != 0 {
            paging::switch_cr3(vibux_vfork_parent_cr3);
        }

        syscall::restore_process_state(*parent_state());
        syscall::clear_exit_state();
        syscall::set_fs_base(vibux_vfork_parent_fs_base);

        push_exited_child(vibux_vfork_child_pid, vibux_vfork_child_status);

        vibux_vfork_child_active = 0;
    }
}

pub fn prepare_exec_tls() -> bool {
    let mapped = crate::velf::with_process_allocator(|allocator, physical_offset| {
        let paging = paging::PageTableManager::new(physical_offset);

        if paging.translate(TLS_BASE).is_none() {
            let Some(frame) = allocator.allocate() else {
                return false;
            };

            if paging
                .map_4k(
                    TLS_BASE,
                    frame.addr(),
                    crate::arch::x86_64::paging::PageFlags::USER
                        .union(crate::arch::x86_64::paging::PageFlags::WRITABLE)
                        .union(crate::arch::x86_64::paging::PageFlags::NO_EXECUTE),
                    || allocator.allocate().map(|x| x.addr()),
                )
                .is_err()
            {
                return false;
            }

            unsafe {
                core::ptr::write_bytes((physical_offset + frame.addr()) as *mut u8, 0, 4096);
            }
        }

        true
    });

    match mapped {
        Some(true) => {
            syscall::set_fs_base(TLS_BASE);
            true
        }
        _ => false,
    }
}

#[cold]
#[inline(never)]
fn exec_fail(code: u64) -> ! {
    unsafe { syscall::exec_failure_fast(code) }
}

pub fn execve(path_address: u64, argv_address: u64, envp_address: u64) -> u64 {
    unsafe {
        if vibux_vfork_child_active == 0 {
            return 0u64.wrapping_sub(ENOSYS);
        }
    }

    let mut path = [0u8; 160];

    let path_len = match syscall::read_c_string(path_address, &mut path) {
        Ok(v) => v,
        Err(code) => return exec_fail(code),
    };

    let mut argv = [UserString::empty(); MAX_ARGS];
    let argc = match read_user_vector(argv_address, &mut argv) {
        Ok(v) => v,
        Err(code) => return exec_fail(code),
    };

    let mut envp = [UserString::empty(); MAX_ARGS];
    let envc = if envp_address == 0 {
        0
    } else {
        match read_user_vector(envp_address, &mut envp) {
            Ok(v) => v,
            Err(code) => return exec_fail(code),
        }
    };

    let mut argv_refs = [&[][..]; MAX_ARGS];
    let mut envp_refs = [&[][..]; MAX_ARGS];

    let mut i = 0usize;
    while i < argc {
        argv_refs[i] = argv[i].as_slice();
        i += 1;
    }

    i = 0;
    while i < envc {
        envp_refs[i] = envp[i].as_slice();
        i += 1;
    }

    let new_cr3 = crate::velf::with_process_allocator(|allocator, physical_offset| {
        paging::create_empty_user_address_space(allocator, physical_offset)
    })
    .flatten();

    let Some(new_cr3) = new_cr3 else {
        return exec_fail(ENOMEM);
    };

    unsafe {
        paging::switch_cr3(new_cr3);
    }

    let mut serial = crate::console::Serial::new();
    serial.init();

    let result = crate::velf::runner::exec_program(
        allocator(),
        physical_offset(),
        &mut serial,
        &path[..path_len],
        &argv_refs[..argc],
        &envp_refs[..envc],
    );

    match result {
        Ok(()) => unsafe { core::hint::unreachable_unchecked() },
        Err(code) => exec_fail(code),
    }
}

#[inline(always)]
fn allocator() -> &'static mut FrameAllocator<'static> {
    unsafe { crate::velf::current_frame_allocator() }
}

#[inline(always)]
fn physical_offset() -> u64 {
    crate::velf::process_physical_offset()
}

pub fn wait4(requested_pid: i32, status_address: u64, options: u64, rusage_address: u64) -> u64 {
    let wnohang = options & 1 != 0;

    if let Some(entry) = take_exited_child(requested_pid) {
        if status_address != 0 {
            if syscall::copy_user_out(status_address, &entry.status.to_le_bytes()).is_err() {
                return 0u64.wrapping_sub(EFAULT);
            }
        }
        if rusage_address != 0 {
            let usage = [0u8; 144];
            if syscall::copy_user_out(rusage_address, &usage).is_err() {
                return 0u64.wrapping_sub(EFAULT);
            }
        }
        return entry.pid;
    }

    if wnohang {
        return 0;
    }

    0u64.wrapping_sub(ECHILD)
}
