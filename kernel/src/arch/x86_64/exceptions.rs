use core::arch::asm;

#[repr(C)]
pub struct ExceptionFrame {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rbp: u64,
    pub rbx: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rax: u64,
    pub vector: u64,
    pub error_code: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

const RTLD_BASE: u64 = 0x0000_5000_0000_0000;
const RTLD_ENTRY: u64 = 0x0000_5000_0002_8440;
const MAIN_BASE: u64 = 0x0000_4000_0000_0000;

#[inline(always)]
unsafe fn outb(port: u16, value: u8) {
    unsafe {
        asm!(
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nostack, preserves_flags)
        );
    }
}

#[inline(always)]
fn serial_byte(value: u8) {
    unsafe {
        outb(0x3F8, value);
        outb(0xE9, value);
    }
}

#[inline(always)]
fn serial(bytes: &[u8]) {
    for &byte in bytes {
        serial_byte(byte);
    }
}

#[inline(always)]
fn crlf() {
    serial_byte(b'\r');
    serial_byte(b'\n');
}

#[inline(always)]
fn hex(value: u64) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    serial(b"0x");
    let mut shift = 60i32;
    while shift >= 0 {
        serial_byte(HEX[((value >> shift) & 0xF) as usize]);
        shift -= 4;
    }
}

#[inline(always)]
fn hex_nibble(value: u8) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    serial_byte(HEX[((value >> 4) & 0xF) as usize]);
    serial_byte(HEX[(value & 0xF) as usize]);
}

#[inline(always)]
fn dec(mut value: u64) {
    let mut buf = [0u8; 20];
    let mut pos = buf.len();
    if value == 0 {
        serial_byte(b'0');
        return;
    }
    while value != 0 {
        pos -= 1;
        buf[pos] = b'0' + (value % 10) as u8;
        value /= 10;
    }
    serial(&buf[pos..]);
}

#[inline(always)]
fn canonical(value: u64) -> bool {
    value <= 0x0000_7FFF_FFFF_FFFF || value >= 0xFFFF_8000_0000_0000
}

#[inline(always)]
fn rule() {
    serial(b"============================================================\r\n");
}

#[inline(always)]
fn bar(label: &[u8]) {
    serial(b"-- ");
    serial(label);
    serial(b" --");
    crlf();
}

fn exception_name(vector: u64) -> &'static [u8] {
    match vector {
        0 => b"#DE Divide Error",
        1 => b"#DB Debug",
        2 => b"NMI Non-Maskable Interrupt",
        3 => b"#BP Breakpoint",
        4 => b"#OF Overflow",
        5 => b"#BR Bound Range Exceeded",
        6 => b"#UD Invalid Opcode",
        7 => b"#NM Device Not Available",
        8 => b"#DF Double Fault",
        9 => b"Coprocessor Segment Overrun",
        10 => b"#TS Invalid TSS",
        11 => b"#NP Segment Not Present",
        12 => b"#SS Stack-Segment Fault",
        13 => b"#GP General Protection",
        14 => b"#PF Page Fault",
        15 => b"Reserved",
        16 => b"#MF x87 FP Exception",
        17 => b"#AC Alignment Check",
        18 => b"#MC Machine Check",
        19 => b"#XM/#XF SIMD FP Exception",
        20 => b"#VE Virtualization Exception",
        21 => b"#CP Control Protection Exception",
        22 => b"Reserved",
        23 => b"Reserved",
        24 => b"Reserved",
        25 => b"Reserved",
        26 => b"Reserved",
        27 => b"Reserved",
        28 => b"#HV Hypervisor Injection",
        29 => b"#VC VMM Communication",
        30 => b"#SX Security Exception",
        31 => b"Reserved",
        _ => b"Unknown",
    }
}

#[inline(always)]
unsafe fn read_cr0() -> u64 {
    let value: u64;
    unsafe {
        asm!("mov {}, cr0", out(reg) value, options(nostack, preserves_flags));
    }
    value
}

#[inline(always)]
unsafe fn read_cr2() -> u64 {
    let value: u64;
    unsafe {
        asm!("mov {}, cr2", out(reg) value, options(nostack, preserves_flags));
    }
    value
}

#[inline(always)]
unsafe fn read_cr3() -> u64 {
    let value: u64;
    unsafe {
        asm!("mov {}, cr3", out(reg) value, options(nostack, preserves_flags));
    }
    value
}

#[inline(always)]
unsafe fn read_cr4() -> u64 {
    let value: u64;
    unsafe {
        asm!("mov {}, cr4", out(reg) value, options(nostack, preserves_flags));
    }
    value
}

#[inline(always)]
unsafe fn read_fs_base() -> u64 {
    let low: u32;
    let high: u32;
    unsafe {
        asm!(
            "mov ecx, 0xC0000100",
            "rdmsr",
            lateout("eax") low,
            lateout("edx") high,
            options(nostack, preserves_flags)
        );
    }
    ((high as u64) << 32) | low as u64
}

#[inline(always)]
fn reg(name: &[u8], value: u64) {
    serial(b"  ");
    serial(name);
    serial(b" = ");
    hex(value);
    crlf();
}

#[inline(always)]
fn dump_bytes(address: u64, count: usize) {
    serial(b"  ");
    hex(address);
    serial(b" : ");

    let paging =
        crate::arch::x86_64::paging::PageTableManager::new(crate::velf::process_physical_offset());

    let page_offset = address & 0xFFF;

    for i in 0..count {
        if page_offset + i as u64 >= 0x1000 {
            serial(b" <page-end>");
            break;
        }
        let current = address.wrapping_add(i as u64);
        if paging.translate(current).is_none() {
            serial(b"?? ");
            continue;
        }
        let byte = unsafe { core::ptr::read_volatile(current as *const u8) };
        hex_nibble(byte);
        serial_byte(b' ');
    }
    crlf();
}

#[inline(always)]
fn dump_instruction_window(rip: u64) {
    serial(b"  window: ");
    let start = rip.wrapping_sub(16);
    for i in 0..48u64 {
        if i != 0 {
            serial_byte(b' ');
        }
        let byte = unsafe { core::ptr::read_volatile(start.wrapping_add(i) as *const u8) };
        hex_nibble(byte);
    }
    crlf();
    serial(b"  marker: ");
    for _ in 0..(16 * 3) {
        serial_byte(b' ');
    }
    serial(b"^^ <-- RIP");
    crlf();
}

#[inline(always)]
fn decode_opcode_hint(address: u64) {
    let b0 = unsafe { core::ptr::read_volatile(address as *const u8) };
    let b1 = unsafe { core::ptr::read_volatile((address + 1) as *const u8) };
    let b2 = unsafe { core::ptr::read_volatile((address + 2) as *const u8) };
    let b3 = unsafe { core::ptr::read_volatile((address + 3) as *const u8) };

    serial(b"  opcode hint: ");
    if b0 == 0x0F && b1 == 0x0B {
        serial(b"UD2 (explicit invalid-opcode)");
    } else if b0 == 0x62 {
        serial(b"EVEX prefix (AVX-512)");
    } else if b0 >= 0xC4 && b0 <= 0xC5 {
        serial(b"VEX prefix (AVX)");
    } else if b0 == 0xF3 && b1 == 0x0F && b2 == 0x1E && b3 == 0xFA {
        serial(b"ENDBR64 (CET landing pad)");
    } else if b0 == 0x0F && b1 == 0xAE {
        serial(b"0F AE system/state");
    } else if b0 == 0x0F && b1 == 0x38 {
        serial(b"0F 38 extended");
    } else if b0 == 0x0F && b1 == 0x3A {
        serial(b"0F 3A extended");
    } else {
        serial(b"common/unknown prefix");
    }
    crlf();
}

#[inline(always)]
fn decode_gp_instruction(frame: &ExceptionFrame) {
    let rip = frame.rip;
    let b0 = unsafe { core::ptr::read_volatile(rip as *const u8) };
    let b1 = unsafe { core::ptr::read_volatile(rip.wrapping_add(1) as *const u8) };
    let b2 = unsafe { core::ptr::read_volatile(rip.wrapping_add(2) as *const u8) };
    let b3 = unsafe { core::ptr::read_volatile(rip.wrapping_add(3) as *const u8) };
    let b4 = unsafe { core::ptr::read_volatile(rip.wrapping_add(4) as *const u8) };

    if b0 == 0xC5 && b1 == 0xF9 && b2 == 0x7F && b3 == 0x45 && b4 == 0x90 {
        let address = frame.rbp.wrapping_sub(0x70);
        serial(b"  decoded: VMOVDQA [RBP-0x70], XMM0  (VEX.128.66.0F.WIG 7F /r)");
        crlf();
        serial(b"  target = ");
        hex(address);
        serial(b"  (mod16 = ");
        hex(address & 0xF);
        serial(b")");
        crlf();
        if (address & 0xF) != 0 {
            serial(b"  >>> VMOVDQA target is NOT 16-byte aligned.");
            crlf();
            serial(b"  >>> #GP on unaligned operand. Userspace stack misaligned.");
            crlf();
        } else {
            serial(b"  target correctly aligned.");
            crlf();
        }
        return;
    }

    if b0 == 0xF3 && b1 == 0x0F && b2 == 0x1E && b3 == 0xFA {
        serial(b"  decoded: ENDBR64 (CET) - valid landing pad.");
        crlf();
        return;
    }

    if b0 == 0x0F && b1 == 0x0B {
        serial(b"  decoded: UD2 - explicit invalid opcode.");
        crlf();
        return;
    }

    serial(b"  decoded: unknown 5-byte pattern ");
    hex_nibble(b0);
    serial_byte(b' ');
    hex_nibble(b1);
    serial_byte(b' ');
    hex_nibble(b2);
    serial_byte(b' ');
    hex_nibble(b3);
    serial_byte(b' ');
    hex_nibble(b4);
    crlf();
}

#[unsafe(no_mangle)]
pub extern "C" fn vibux_exception_report(frame: &ExceptionFrame) -> ! {
    let cr0 = unsafe { read_cr0() };
    let cr2 = unsafe { read_cr2() };
    let cr3 = unsafe { read_cr3() };
    let cr4 = unsafe { read_cr4() };
    let fs_base = unsafe { read_fs_base() };
    let cpl = frame.cs & 3;

    crlf();
    rule();
    serial(b"                  VIBUX CPU EXCEPTION\r\n");
    rule();

    serial(b"  vector     = ");
    dec(frame.vector);
    serial(b" (");
    serial(exception_name(frame.vector));
    serial(b")");
    crlf();
    serial(b"  error code = ");
    hex(frame.error_code);
    crlf();
    serial(b"  cpl        = ");
    dec(cpl);
    serial(if cpl == 3 {
        b" (ring 3 user)"
    } else if cpl == 0 {
        b" (ring 0 kernel)"
    } else {
        b""
    });
    crlf();

    bar(b"control");
    reg(b"CR0", cr0);
    reg(b"CR2", cr2);
    reg(b"CR3", cr3);
    reg(b"CR4", cr4);
    reg(b"FS_BASE", fs_base);

    bar(b"return frame");
    reg(b"RIP", frame.rip);
    reg(b"CS ", frame.cs);
    reg(b"RFLAGS", frame.rflags);
    reg(b"RSP", frame.rsp);
    reg(b"SS ", frame.ss);

    bar(b"general registers");
    reg(b"RAX", frame.rax);
    reg(b"RBX", frame.rbx);
    reg(b"RCX", frame.rcx);
    reg(b"RDX", frame.rdx);
    reg(b"RSI", frame.rsi);
    reg(b"RDI", frame.rdi);
    reg(b"RBP", frame.rbp);
    reg(b"R8 ", frame.r8);
    reg(b"R9 ", frame.r9);
    reg(b"R10", frame.r10);
    reg(b"R11", frame.r11);
    reg(b"R12", frame.r12);
    reg(b"R13", frame.r13);
    reg(b"R14", frame.r14);
    reg(b"R15", frame.r15);

    serial(b"  RIP canonical = ");
    serial(if canonical(frame.rip) { b"yes" } else { b"NO" });
    serial(b"   RSP canonical = ");
    serial(if canonical(frame.rsp) { b"yes" } else { b"NO" });
    crlf();

    serial(b"  RBP-RSP delta = ");
    if frame.rbp >= frame.rsp {
        hex(frame.rbp - frame.rsp);
    } else {
        serial(b"-");
        hex(frame.rsp - frame.rbp);
    }
    crlf();

    match frame.vector {
        6 => {
            bar(b"#UD invalid opcode diagnostics");
            serial(b"  CPU rejected instruction at RIP.");
            crlf();
            serial(b"  RIP - RTLD base  = ");
            hex(frame.rip.wrapping_sub(RTLD_BASE));
            crlf();
            serial(b"  RIP - RTLD entry = ");
            hex(frame.rip.wrapping_sub(RTLD_ENTRY));
            crlf();
            serial(b"  RIP - MAIN base  = ");
            hex(frame.rip.wrapping_sub(MAIN_BASE));
            crlf();

            serial(b"  bytes at RIP:");
            crlf();
            dump_bytes(frame.rip, 32);

            serial(b"  bytes before RIP:");
            crlf();
            if (frame.rip & 0xFFF) >= 16 {
                dump_bytes(frame.rip - 16, 16);
            } else {
                serial(b"  near page boundary");
                crlf();
            }

            decode_opcode_hint(frame.rip);

            if cpl == 3 {
                serial(b"  note: CPL=3 -> kernel-to-user transition succeeded.");
                crlf();
                serial(b"  fault is in the ring-3 instruction stream.");
                crlf();
            }
        }

        13 => {
            bar(b"#GP general protection diagnostics");
            serial(b"  RSP mod16 = ");
            hex(frame.rsp & 0xF);
            serial(b"   RBP mod16 = ");
            hex(frame.rbp & 0xF);
            crlf();

            dump_instruction_window(frame.rip);
            decode_gp_instruction(frame);

            serial(b"  usual causes:");
            crlf();
            serial(b"    - unaligned SSE/AVX memory operand (VMOVDQA / MOVDQA)");
            crlf();
            serial(b"    - segment register load with invalid selector");
            crlf();
            serial(b"    - privileged instruction executed at CPL=3");
            crlf();
            serial(b"    - non-canonical address in a segment/descriptor access");
            crlf();
        }

        14 => {
            bar(b"#PF page fault diagnostics");
            serial(b"  CR2 faulting address = ");
            hex(cr2);
            crlf();
            serial(b"  RIP                   = ");
            hex(frame.rip);
            crlf();
            serial(b"  RIP - RTLD base       = ");
            hex(frame.rip.wrapping_sub(RTLD_BASE));
            crlf();
            serial(b"  RIP - RTLD entry      = ");
            hex(frame.rip.wrapping_sub(RTLD_ENTRY));
            crlf();
            serial(b"  RIP page offset       = ");
            hex(frame.rip & 0xFFF);
            crlf();
            serial(b"  RSP page offset       = ");
            hex(frame.rsp & 0xFFF);
            crlf();

            serial(b"  PF error bits: ");
            serial(if frame.error_code & 1 != 0 {
                b"P=1 "
            } else {
                b"P=0 "
            });
            serial(if frame.error_code & 2 != 0 {
                b"W=1 "
            } else {
                b"W=0 "
            });
            serial(if frame.error_code & 4 != 0 {
                b"U=1 "
            } else {
                b"U=0 "
            });
            serial(if frame.error_code & 8 != 0 {
                b"RSVD=1 "
            } else {
                b"RSVD=0 "
            });
            serial(if frame.error_code & 16 != 0 {
                b"I=1"
            } else {
                b"I=0"
            });
            crlf();

            if frame.error_code & 1 == 0 {
                serial(b"  cause: page not present -> UNMAPPED access.");
                crlf();
            } else if frame.error_code & 2 != 0 {
                serial(b"  cause: write to a read-only page (protection).");
                crlf();
            } else {
                serial(b"  cause: read of a present but protected page.");
                crlf();
            }
            if frame.error_code & 4 != 0 {
                serial(b"  access originated from ring 3 (user).");
                crlf();
            } else {
                serial(b"  access originated from ring 0 (kernel).");
                crlf();
            }

            serial(b"  bytes at RIP:");
            crlf();
            dump_bytes(frame.rip, 16);

            serial(b"  bytes before RIP:");
            crlf();
            if (frame.rip & 0xFFF) >= 16 {
                dump_bytes(frame.rip - 16, 16);
            } else {
                serial(b"  near page boundary");
                crlf();
            }

            if fs_base != 0 {
                serial(b"  FS_BASE = ");
                hex(fs_base);
                crlf();
            }

            let paging = crate::arch::x86_64::paging::PageTableManager::new(
                crate::velf::process_physical_offset(),
            );

            serial(b"  RIP mapping: ");
            match paging.translate(frame.rip) {
                Some(mapping) => {
                    serial(b"PA=");
                    hex(mapping.physical_address);
                    serial(b" flags=");
                    hex(mapping.flags.bits());
                    serial(b" page=");
                    hex(mapping.page_size);
                    serial(b" exec=");
                    serial(
                        if !mapping
                            .flags
                            .contains(crate::arch::x86_64::paging::PageFlags::NO_EXECUTE)
                        {
                            b"yes"
                        } else {
                            b"NO"
                        },
                    );
                    crlf();
                }
                None => {
                    serial(b"UNMAPPED");
                    crlf();
                }
            }

            serial(b"  CR2 mapping: ");
            match paging.translate(cr2) {
                Some(mapping) => {
                    serial(b"PA=");
                    hex(mapping.physical_address);
                    serial(b" flags=");
                    hex(mapping.flags.bits());
                    serial(b" page=");
                    hex(mapping.page_size);
                    crlf();
                }
                None => {
                    serial(b"UNMAPPED");
                    crlf();
                }
            }

            bar(b"user stack words at RSP");
            let mut stack_index = 0u64;
            while stack_index < 16 {
                let address = frame.rsp.wrapping_add(stack_index * 8);
                serial(b"  RSP+");
                hex(stack_index * 8);
                serial(b" va=");
                hex(address);
                serial(b" val=");
                if paging.translate(address).is_some() {
                    let value = unsafe { core::ptr::read_volatile(address as *const u64) };
                    hex(value);
                } else {
                    serial(b"UNMAPPED");
                }
                crlf();
                stack_index += 1;
            }
        }

        8 => {
            bar(b"#DF double fault");
            serial(b"  CPU could not deliver a prior exception cleanly.");
            crlf();
            serial(b"  common causes:");
            crlf();
            serial(b"    - kernel stack overflow (IST1 not set up)");
            crlf();
            serial(b"    - recursive fault during exception entry");
            crlf();
            serial(b"    - invalid IDT entry hit again");
            crlf();
        }

        10 | 11 | 12 => {
            bar(b"segment / TSS fault");
            serial(b"  check GDT selectors, descriptor privilege, TSS rsp0/IST.");
            crlf();
        }

        _ => {}
    }

    crlf();
    serial(b"VIBUX HALTED AFTER EXCEPTION");
    crlf();
    rule();

    loop {
        unsafe {
            asm!("cli", "hlt", options(nostack));
        }
    }
}

unsafe extern "C" {
    pub fn vibux_exception_0();
    pub fn vibux_exception_1();
    pub fn vibux_exception_2();
    pub fn vibux_exception_3();
    pub fn vibux_exception_4();
    pub fn vibux_exception_5();
    pub fn vibux_exception_6();
    pub fn vibux_exception_7();
    pub fn vibux_exception_8();
    pub fn vibux_exception_9();
    pub fn vibux_exception_10();
    pub fn vibux_exception_11();
    pub fn vibux_exception_12();
    pub fn vibux_exception_13();
    pub fn vibux_exception_14();
    pub fn vibux_exception_15();
    pub fn vibux_exception_16();
    pub fn vibux_exception_17();
    pub fn vibux_exception_18();
    pub fn vibux_exception_19();
    pub fn vibux_exception_20();
    pub fn vibux_exception_21();
    pub fn vibux_exception_22();
    pub fn vibux_exception_23();
    pub fn vibux_exception_24();
    pub fn vibux_exception_25();
    pub fn vibux_exception_26();
    pub fn vibux_exception_27();
    pub fn vibux_exception_28();
    pub fn vibux_exception_29();
    pub fn vibux_exception_30();
    pub fn vibux_exception_31();
}

core::arch::global_asm!(
    r#"
.section .text

.macro NOERR name, vector
.global \name
.type \name,@function
\name:
    pushq $0
    pushq $\vector
    jmp vibux_exception_common
.endm

.macro ERR name, vector
.global \name
.type \name,@function
\name:
    pushq $\vector
    jmp vibux_exception_common
.endm

vibux_exception_common:
    cli

    pushq %rax
    pushq %rcx
    pushq %rdx
    pushq %rbx
    pushq %rbp
    pushq %rsi
    pushq %rdi
    pushq %r8
    pushq %r9
    pushq %r10
    pushq %r11
    pushq %r12
    pushq %r13
    pushq %r14
    pushq %r15

    movq %rsp, %rdi

    andq $-16, %rsp
    subq $8, %rsp

    call vibux_exception_report

1:
    cli
    hlt
    jmp 1b

NOERR vibux_exception_0, 0
NOERR vibux_exception_1, 1
NOERR vibux_exception_2, 2
NOERR vibux_exception_3, 3
NOERR vibux_exception_4, 4
NOERR vibux_exception_5, 5
NOERR vibux_exception_6, 6
NOERR vibux_exception_7, 7
ERR   vibux_exception_8, 8
NOERR vibux_exception_9, 9
ERR   vibux_exception_10, 10
ERR   vibux_exception_11, 11
ERR   vibux_exception_12, 12
ERR   vibux_exception_13, 13
ERR   vibux_exception_14, 14
NOERR vibux_exception_15, 15
NOERR vibux_exception_16, 16
ERR   vibux_exception_17, 17
NOERR vibux_exception_18, 18
NOERR vibux_exception_19, 19
NOERR vibux_exception_20, 20
ERR   vibux_exception_21, 21
NOERR vibux_exception_22, 22
NOERR vibux_exception_23, 23
NOERR vibux_exception_24, 24
NOERR vibux_exception_25, 25
NOERR vibux_exception_26, 26
NOERR vibux_exception_27, 27
NOERR vibux_exception_28, 28
ERR   vibux_exception_29, 29
ERR   vibux_exception_30, 30
NOERR vibux_exception_31, 31
"#,
    options(att_syntax)
);
