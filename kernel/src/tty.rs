use core::arch::asm;
use core::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};

use crate::terminal::FrameTerminal;

const ICANON: u32 = 0x0000_0002;
const ECHO: u32 = 0x0000_0008;
const ISIG: u32 = 0x0000_0001;

static TTY: AtomicPtr<FrameTerminal<'static>> = AtomicPtr::new(core::ptr::null_mut());

static LOCK: AtomicBool = AtomicBool::new(false);

static LFLAG: AtomicU32 = AtomicU32::new(ICANON | ECHO | ISIG);

const INPUT_CAPACITY: usize = 4096;
const LINE_CAPACITY: usize = 4096;

static mut INPUT: [u8; 4096] = [0; 4096];
static mut INPUT_HEAD: usize = 0;
static mut INPUT_TAIL: usize = 0;

static mut LINE: [u8; 4096] = [0; 4096];
static mut LINE_LEN: usize = 0;

static mut SHIFT: bool = false;
static mut CTRL: bool = false;
static mut CAPS: bool = false;

fn lock() {
    while LOCK.swap(true, Ordering::Acquire) {
        core::hint::spin_loop();
    }
}

fn unlock() {
    LOCK.store(false, Ordering::Release);
}

pub fn attach(screen: &mut FrameTerminal<'_>) {
    let pointer = screen as *mut FrameTerminal<'_> as *mut FrameTerminal<'static>;

    TTY.store(pointer, Ordering::Release);
}

fn with_screen<F: FnOnce(&mut FrameTerminal<'_>)>(f: F) {
    let pointer = TTY.load(Ordering::Acquire);

    if pointer.is_null() {
        return;
    }

    lock();

    unsafe {
        f(&mut *pointer);
    }

    unlock();
}

pub fn write(bytes: &[u8]) -> usize {
    if bytes.is_empty() {
        return 0;
    }

    with_screen(|screen| {
        screen.write_bytes(bytes);
    });

    for &byte in bytes {
        crate::console::serial_write_byte(byte);
    }

    bytes.len()
}

pub fn lflag() -> u32 {
    LFLAG.load(Ordering::Acquire)
}

pub fn set_lflag(value: u32) {
    LFLAG.store(value, Ordering::Release);
}

fn push_input(byte: u8) {
    unsafe {
        let next = (INPUT_TAIL + 1) % INPUT_CAPACITY;

        if next == INPUT_HEAD {
            return;
        }

        INPUT[INPUT_TAIL] = byte;
        INPUT_TAIL = next;
    }
}

fn pop_input() -> Option<u8> {
    unsafe {
        if INPUT_HEAD == INPUT_TAIL {
            return None;
        }

        let byte = INPUT[INPUT_HEAD];

        INPUT_HEAD = (INPUT_HEAD + 1) % INPUT_CAPACITY;

        Some(byte)
    }
}

fn flush_line() {
    unsafe {
        for index in 0..LINE_LEN {
            push_input(LINE[index]);
        }

        push_input(b'\n');

        LINE_LEN = 0;
    }
}

fn echo(bytes: &[u8]) {
    if LFLAG.load(Ordering::Acquire) & ECHO != 0 {
        write(bytes);
    }
}

fn interrupt_echo(bytes: &[u8]) {
    let pointer = TTY.load(Ordering::Acquire);

    if !pointer.is_null()
        && LOCK
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    {
        unsafe {
            (*pointer).write_bytes(bytes);
        }
        LOCK.store(false, Ordering::Release);
    }

    for &byte in bytes {
        crate::console::serial_write_byte(byte);
    }
}

pub fn handle_keyboard_interrupt(scancode: u8) -> bool {
    let released = scancode & 0x80 != 0;
    let code = scancode & 0x7f;

    match code {
        0x2a | 0x36 => {
            unsafe {
                SHIFT = !released;
            }
            return true;
        }

        0x1d => {
            unsafe {
                CTRL = !released;
            }
            return true;
        }

        0x3a if !released => {
            unsafe {
                CAPS = !CAPS;
            }
            return true;
        }

        _ if released => return true,
        _ => {}
    }

    if unsafe { CTRL } {
        let flags = LFLAG.load(Ordering::Acquire);
        let isig = flags & ISIG != 0;
        let canonical = flags & ICANON != 0;

        match code {
            0x2e => {
                unsafe {
                    LINE_LEN = 0;
                }
                if isig {
                    interrupt_echo(
                        b"^C
",
                    );
                    crate::arch::x86_64::syscall::raise_signal_from_tty(2);
                } else {
                    push_input(0x03);
                }
                return true;
            }

            0x2b => {
                unsafe {
                    LINE_LEN = 0;
                }
                if isig {
                    interrupt_echo(
                        b"^\
",
                    );
                    crate::arch::x86_64::syscall::raise_signal_from_tty(3);
                } else {
                    push_input(0x1c);
                }
                return true;
            }

            0x2c => {
                unsafe {
                    LINE_LEN = 0;
                }
                if isig {
                    interrupt_echo(
                        b"^Z
",
                    );
                    crate::arch::x86_64::syscall::raise_signal_from_tty(20);
                } else {
                    push_input(0x1a);
                }
                return true;
            }

            0x20 => {
                if canonical {
                    unsafe {
                        if LINE_LEN == 0 {
                            push_input(0);
                        } else {
                            flush_line();
                        }
                    }
                } else {
                    push_input(0x04);
                }
                return true;
            }

            0x26 => {
                push_input(0x0c);
                return true;
            }

            _ => {}
        }
    }

    false
}

fn process_input_byte(byte: u8) -> Option<u8> {
    let flags = LFLAG.load(Ordering::Acquire);

    if flags & ISIG != 0 {
        match byte {
            0x03 => {
                unsafe {
                    LINE_LEN = 0;
                }
                echo(b"^C\r\n");
                crate::arch::x86_64::syscall::raise_signal_from_tty(2);
                return None;
            }

            0x1c => {
                unsafe {
                    LINE_LEN = 0;
                }
                echo(b"^\\\r\n");
                crate::arch::x86_64::syscall::raise_signal_from_tty(3);
                return None;
            }

            0x1a => {
                unsafe {
                    LINE_LEN = 0;
                }
                echo(b"^Z\r\n");
                crate::arch::x86_64::syscall::raise_signal_from_tty(20);
                return None;
            }

            _ => {}
        }
    }

    if flags & ICANON == 0 {
        if byte == 0x7f || byte == 0x08 {
            echo(b"\x08 \x08");
        } else {
            echo(&[byte]);
        }

        return Some(byte);
    }

    match byte {
        0x08 | 0x7f => unsafe {
            if LINE_LEN != 0 {
                LINE_LEN -= 1;
                echo(b"\x08 \x08");
            }

            None
        },

        0x04 => unsafe {
            trace_text(b"[TTY CTRL-D] received 0x04 line_len=");
            trace_hex8(core::cmp::min(LINE_LEN, 0xFF) as u8);
            trace_text(b"\\r\\n");

            if LINE_LEN == 0 {
                trace_text(b"[TTY EOF] Ctrl-D on empty canonical line -> returning 0\\r\\n");
                Some(0)
            } else {
                trace_text(b"[TTY CTRL-D] buffered line exists -> flush_line()\\r\\n");
                flush_line();

                let out = pop_input();

                match out {
                    Some(value) => trace_byte(b"[TTY CTRL-D FLUSH] first byte=", value),
                    None => trace_text(b"[TTY CTRL-D FLUSH] first byte=NONE\\r\\n"),
                }

                out
            }
        },

        b'\r' | b'\n' => {
            echo(b"\r\n");
            flush_line();
            pop_input()
        }

        _ => unsafe {
            if LINE_LEN < LINE_CAPACITY - 1 {
                LINE[LINE_LEN] = byte;
                LINE_LEN += 1;
            }

            echo(&[byte]);
            None
        },
    }
}

fn keyboard_ascii(scancode: u8) -> Option<u8> {
    let released = scancode & 0x80 != 0;
    let code = scancode & 0x7f;

    match code {
        0x2a | 0x36 => {
            unsafe {
                SHIFT = !released;
            }
            return None;
        }

        0x1d => {
            unsafe {
                CTRL = !released;
            }
            return None;
        }

        0x3a if !released => {
            unsafe {
                CAPS = !CAPS;
            }
            return None;
        }

        _ if released => return None,
        _ => {}
    }

    let (shift, ctrl, caps) = unsafe { (SHIFT, CTRL, CAPS) };

    if ctrl {
        return match code {
            0x2e => Some(0x03),
            0x20 => Some(0x04),
            0x26 => Some(0x0c),
            _ => None,
        };
    }

    let letter = |base: u8| {
        if caps ^ shift {
            base.to_ascii_uppercase()
        } else {
            base
        }
    };

    Some(match code {
        0x01 => 0x1b,
        0x02 => {
            if shift {
                b'!'
            } else {
                b'1'
            }
        }
        0x03 => {
            if shift {
                b'@'
            } else {
                b'2'
            }
        }
        0x04 => {
            if shift {
                b'#'
            } else {
                b'3'
            }
        }
        0x05 => {
            if shift {
                b'$'
            } else {
                b'4'
            }
        }
        0x06 => {
            if shift {
                b'%'
            } else {
                b'5'
            }
        }
        0x07 => {
            if shift {
                b'^'
            } else {
                b'6'
            }
        }
        0x08 => {
            if shift {
                b'&'
            } else {
                b'7'
            }
        }
        0x09 => {
            if shift {
                b'*'
            } else {
                b'8'
            }
        }
        0x0a => {
            if shift {
                b'('
            } else {
                b'9'
            }
        }
        0x0b => {
            if shift {
                b')'
            } else {
                b'0'
            }
        }
        0x0c => {
            if shift {
                b'_'
            } else {
                b'-'
            }
        }
        0x0d => {
            if shift {
                b'+'
            } else {
                b'='
            }
        }
        0x0e => 0x7f,
        0x0f => b'\t',

        0x10 => letter(b'q'),
        0x11 => letter(b'w'),
        0x12 => letter(b'e'),
        0x13 => letter(b'r'),
        0x14 => letter(b't'),
        0x15 => letter(b'y'),
        0x16 => letter(b'u'),
        0x17 => letter(b'i'),
        0x18 => letter(b'o'),
        0x19 => letter(b'p'),

        0x1a => {
            if shift {
                b'{'
            } else {
                b'['
            }
        }
        0x1b => {
            if shift {
                b'}'
            } else {
                b']'
            }
        }
        0x1c => b'\n',

        0x1e => letter(b'a'),
        0x1f => letter(b's'),
        0x20 => letter(b'd'),
        0x21 => letter(b'f'),
        0x22 => letter(b'g'),
        0x23 => letter(b'h'),
        0x24 => letter(b'j'),
        0x25 => letter(b'k'),
        0x26 => letter(b'l'),

        0x27 => {
            if shift {
                b':'
            } else {
                b';'
            }
        }
        0x28 => {
            if shift {
                b'"'
            } else {
                b'\''
            }
        }
        0x29 => {
            if shift {
                b'~'
            } else {
                b'`'
            }
        }

        0x2b => {
            if shift {
                b'|'
            } else {
                b'\\'
            }
        }

        0x2c => letter(b'z'),
        0x2d => letter(b'x'),
        0x2e => letter(b'c'),
        0x2f => letter(b'v'),
        0x30 => letter(b'b'),
        0x31 => letter(b'n'),
        0x32 => letter(b'm'),

        0x33 => {
            if shift {
                b'<'
            } else {
                b','
            }
        }
        0x34 => {
            if shift {
                b'>'
            } else {
                b'.'
            }
        }
        0x35 => {
            if shift {
                b'?'
            } else {
                b'/'
            }
        }

        0x39 => b' ',
        _ => return None,
    })
}

fn trace_text(bytes: &[u8]) {
    for &byte in bytes {
        crate::console::serial_write_byte(byte);
    }
}

fn trace_hex8(value: u8) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";

    trace_text(b"0x");
    crate::console::serial_write_byte(HEX[(value >> 4) as usize]);
    crate::console::serial_write_byte(HEX[(value & 0x0F) as usize]);
}

fn trace_byte(prefix: &[u8], value: u8) {
    trace_text(prefix);
    trace_hex8(value);

    trace_text(b" ascii=");

    if value >= 0x20 && value <= 0x7E {
        crate::console::serial_write_byte(b'\'');
        crate::console::serial_write_byte(value);
        crate::console::serial_write_byte(b'\'');
    } else {
        trace_text(b"<nonprintable>");
    }

    trace_text(b"\\r\\n");
}

pub fn input_available() -> bool {
    unsafe {
        if INPUT_HEAD != INPUT_TAIL {
            return true;
        }
    }

    if crate::console::serial_has_data() {
        return true;
    }

    if crate::arch::x86_64::keyboard_has_scancode() {
        return true;
    }

    false
}

fn poll_raw_input() -> Option<u8> {
    if let Some(byte) = crate::console::serial_read_byte() {
        trace_byte(b"[TTY RAW SERIAL] ", byte);
        return Some(byte);
    }

    if let Some(scancode) = crate::arch::x86_64::pop_keyboard_scancode() {
        trace_byte(b"[TTY RAW KEYBOARD SCANCODE] ", scancode);

        let byte = keyboard_ascii(scancode);

        match byte {
            Some(value) => {
                trace_byte(b"[TTY KEYBOARD ASCII] ", value);
            }
            None => {
                trace_text(b"[TTY KEYBOARD ASCII] NONE\\r\\n");
            }
        }

        return byte;
    }

    None
}

pub fn read_byte() -> u8 {
    loop {
        if crate::arch::x86_64::syscall::signal_pending_unblocked() {
            return 0;
        }

        if let Some(byte) = pop_input() {
            trace_byte(b"[TTY READ_BYTE BUFFER] ", byte);
            return byte;
        }

        if let Some(raw) = poll_raw_input() {
            trace_byte(b"[TTY PROCESS INPUT] ", raw);

            if let Some(byte) = process_input_byte(raw) {
                trace_byte(b"[TTY READ_BYTE PROCESSED] ", byte);
                return byte;
            }

            if let Some(byte) = pop_input() {
                trace_byte(b"[TTY READ_BYTE AFTER_PROCESS] ", byte);
                return byte;
            }

            continue;
        }

        if crate::arch::x86_64::syscall::signal_pending_unblocked() {
            return 0;
        }

        unsafe {
            asm!("sti", "hlt", options(nomem, nostack, preserves_flags));
        }
    }
}
