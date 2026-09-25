use core::arch::asm;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU8, AtomicU64, Ordering};

use crate::arch::x86_64::pic;

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    type_attr: u8,
    offset_mid: u16,
    offset_high: u32,
    reserved: u32,
}

impl IdtEntry {
    const MISSING: Self = Self {
        offset_low: 0,
        selector: 0,
        ist: 0,
        type_attr: 0,
        offset_mid: 0,
        offset_high: 0,
        reserved: 0,
    };

    #[inline(always)]
    fn new(handler: usize, dpl: u8, ist: u8) -> Self {
        Self {
            offset_low: handler as u16,
            selector: 0x08,
            ist,
            type_attr: 0x80 | ((dpl & 3) << 5) | 0x0e,
            offset_mid: (handler >> 16) as u16,
            offset_high: (handler >> 32) as u32,
            reserved: 0,
        }
    }
}

#[repr(C, packed)]
struct Idtr {
    limit: u16,
    base: u64,
}

#[repr(C)]
struct RawInterruptState {
    rax: u64,
    rcx: u64,
    rdx: u64,
    rbx: u64,
    rbp: u64,
    rsi: u64,
    rdi: u64,
    r8: u64,
    r9: u64,
    r10: u64,
    r11: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
    vector: u64,
    error_code: u64,
    rip: u64,
    cs: u64,
    rflags: u64,
    rsp: u64,
    ss: u64,
}

static mut IDT: [IdtEntry; 256] = [IdtEntry::MISSING; 256];

static TICKS: AtomicU64 = AtomicU64::new(0);
static LAST_SCANCODE: AtomicU8 = AtomicU8::new(0);
static PAGE_FAULT_ADDRESS: AtomicU64 = AtomicU64::new(0);
static PAGE_FAULT_ERROR: AtomicU64 = AtomicU64::new(0);

const SCANCODE_QUEUE_SIZE: usize = 256;
const SCANCODE_MASK: u8 = (SCANCODE_QUEUE_SIZE - 1) as u8;

struct ScancodeQueue {
    buffer: UnsafeCell<[u8; SCANCODE_QUEUE_SIZE]>,
    head: AtomicU8,
    tail: AtomicU8,
}

unsafe impl Sync for ScancodeQueue {}

impl ScancodeQueue {
    const fn new() -> Self {
        Self {
            buffer: UnsafeCell::new([0; SCANCODE_QUEUE_SIZE]),
            head: AtomicU8::new(0),
            tail: AtomicU8::new(0),
        }
    }

    #[inline(always)]
    fn push(&self, value: u8) {
        let head = self.head.load(Ordering::Relaxed);
        let next = head.wrapping_add(1);
        let tail = self.tail.load(Ordering::Acquire);
        if next == tail {
            return;
        }
        unsafe {
            (*self.buffer.get())[(head & SCANCODE_MASK) as usize] = value;
        }
        self.head.store(next, Ordering::Release);
    }

    #[inline(always)]
    fn pop(&self) -> Option<u8> {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);
        if tail == head {
            return None;
        }
        let value = unsafe { (*self.buffer.get())[(tail & SCANCODE_MASK) as usize] };
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        Some(value)
    }

    #[inline(always)]
    fn is_empty(&self) -> bool {
        self.tail.load(Ordering::Acquire) == self.head.load(Ordering::Acquire)
    }
}

static SCANCODES: ScancodeQueue = ScancodeQueue::new();

#[inline(always)]
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        asm!("in al, dx", in("dx") port, lateout("al") value,
             options(nostack, preserves_flags));
    }
    value
}

#[inline(always)]
unsafe fn read_cr2() -> u64 {
    let value: u64;
    unsafe {
        asm!("mov {}, cr2", out(reg) value,
             options(nomem, nostack, preserves_flags));
    }
    value
}

unsafe extern "C" {
    fn vibux_timer_isr();
    fn vibux_keyboard_isr();
    fn vibux_syscall_isr();
    fn vibux_fatal_isr();
}

static EXCEPTION_HANDLERS: [unsafe extern "C" fn(); 32] = [
    super::exceptions::vibux_exception_0,
    super::exceptions::vibux_exception_1,
    super::exceptions::vibux_exception_2,
    super::exceptions::vibux_exception_3,
    super::exceptions::vibux_exception_4,
    super::exceptions::vibux_exception_5,
    super::exceptions::vibux_exception_6,
    super::exceptions::vibux_exception_7,
    super::exceptions::vibux_exception_8,
    super::exceptions::vibux_exception_9,
    super::exceptions::vibux_exception_10,
    super::exceptions::vibux_exception_11,
    super::exceptions::vibux_exception_12,
    super::exceptions::vibux_exception_13,
    super::exceptions::vibux_exception_14,
    super::exceptions::vibux_exception_15,
    super::exceptions::vibux_exception_16,
    super::exceptions::vibux_exception_17,
    super::exceptions::vibux_exception_18,
    super::exceptions::vibux_exception_19,
    super::exceptions::vibux_exception_20,
    super::exceptions::vibux_exception_21,
    super::exceptions::vibux_exception_22,
    super::exceptions::vibux_exception_23,
    super::exceptions::vibux_exception_24,
    super::exceptions::vibux_exception_25,
    super::exceptions::vibux_exception_26,
    super::exceptions::vibux_exception_27,
    super::exceptions::vibux_exception_28,
    super::exceptions::vibux_exception_29,
    super::exceptions::vibux_exception_30,
    super::exceptions::vibux_exception_31,
];

pub fn init() {
    unsafe {
        let idt = core::ptr::addr_of_mut!(IDT);
        let slot = idt.cast::<IdtEntry>();

        for vector in 0..256usize {
            let handler = match vector {
                0..=31 => EXCEPTION_HANDLERS[vector] as usize,
                32 => vibux_timer_isr as usize,
                33 => vibux_keyboard_isr as usize,
                0x80 => vibux_syscall_isr as usize,
                _ => vibux_fatal_isr as usize,
            };

            let ist = if vector == 8 { 1 } else { 0 };
            let dpl = if vector == 0x80 { 3 } else { 0 };

            core::ptr::write(slot.add(vector), IdtEntry::new(handler, dpl, ist));
        }

        let idtr = Idtr {
            limit: (core::mem::size_of::<[IdtEntry; 256]>() - 1) as u16,
            base: idt as usize as u64,
        };

        asm!("lidt [{idtr}]",
             idtr = in(reg) core::ptr::addr_of!(idtr),
             options(readonly, nostack, preserves_flags));
    }
}

#[inline(always)]
pub fn enable() {
    unsafe {
        asm!("sti", options(nomem, nostack, preserves_flags));
    }
}

#[inline(always)]
pub fn disable() {
    unsafe {
        asm!("cli", options(nomem, nostack, preserves_flags));
    }
}

#[inline(always)]
pub fn timer_ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

#[inline(always)]
pub fn last_keyboard_scancode() -> u8 {
    LAST_SCANCODE.load(Ordering::Relaxed)
}

#[inline(always)]
pub fn pop_keyboard_scancode() -> Option<u8> {
    SCANCODES.pop()
}

#[inline(always)]
pub fn keyboard_has_scancode() -> bool {
    !SCANCODES.is_empty()
}

#[inline(always)]
pub fn page_fault_address() -> u64 {
    PAGE_FAULT_ADDRESS.load(Ordering::Relaxed)
}

#[inline(always)]
pub fn page_fault_error() -> u64 {
    PAGE_FAULT_ERROR.load(Ordering::Relaxed)
}

#[unsafe(no_mangle)]
extern "C" fn interrupt_dispatch(state: *mut RawInterruptState) {
    let state = unsafe { &mut *state };
    let vector = state.vector as u8;

    match vector {
        32 => {
            TICKS.fetch_add(1, Ordering::Relaxed);
            pic::end_of_interrupt(vector);
        }

        33 => {
            let scancode = unsafe { inb(0x60) };
            LAST_SCANCODE.store(scancode, Ordering::Relaxed);
            if !crate::tty::handle_keyboard_interrupt(scancode) {
                SCANCODES.push(scancode);
            }
            pic::end_of_interrupt(vector);
        }

        14 => {
            #[cfg(debug_assertions)]
            crate::arch::x86_64::syscall::dump_recent_syscalls();

            PAGE_FAULT_ADDRESS.store(unsafe { read_cr2() }, Ordering::Relaxed);
            PAGE_FAULT_ERROR.store(state.error_code, Ordering::Relaxed);

            report_exception(vector, state);
            fatal();
        }

        0x80 => {
            state.rax = (-38i64) as u64;
        }

        _ => {
            report_exception(vector, state);
            fatal();
        }
    }
}

#[cold]
#[inline(never)]
fn report_exception(vector: u8, state: &RawInterruptState) {
    use core::fmt::Write as _;
    let mut s = crate::console::Serial::new();
    s.init();
    writeln!(
        s,
        "[EXC] vec={} err=0x{:X} rip=0x{:016X} cs=0x{:X} rsp=0x{:016X} rflags=0x{:X}",
        vector, state.error_code, state.rip, state.cs, state.rsp, state.rflags,
    )
    .ok();
}

#[cold]
#[inline(never)]
fn fatal() -> ! {
    unsafe {
        asm!("cli", options(nomem, nostack, preserves_flags));
        loop {
            asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}

core::arch::global_asm!(
    r#"
.text

.global vibux_common_isr
.type vibux_common_isr,@function
vibux_common_isr:
    push r15
    push r14
    push r13
    push r12
    push r11
    push r10
    push r9
    push r8
    push rdi
    push rsi
    push rbp
    push rbx
    push rdx
    push rcx
    push rax

    mov rdi, rsp
    sub rsp, 8
    call interrupt_dispatch
    add rsp, 8

    pop rax
    pop rcx
    pop rdx
    pop rbx
    pop rbp
    pop rsi
    pop rdi
    pop r8
    pop r9
    pop r10
    pop r11
    pop r12
    pop r13
    pop r14
    pop r15

    add rsp, 16
    iretq

.global vibux_timer_isr
.type vibux_timer_isr,@function
vibux_timer_isr:
    push 0
    push 32
    jmp vibux_common_isr

.global vibux_keyboard_isr
.type vibux_keyboard_isr,@function
vibux_keyboard_isr:
    push 0
    push 33
    jmp vibux_common_isr

.global vibux_syscall_isr
.type vibux_syscall_isr,@function
vibux_syscall_isr:
    push 0
    push 128
    jmp vibux_common_isr

.global vibux_fatal_isr
.type vibux_fatal_isr,@function
vibux_fatal_isr:
    cli
.Lvibux_fatal:
    hlt
    jmp .Lvibux_fatal
"#
);
