use crate::arch;
use crate::storage;
use core::fmt::Write;

const BUF_SIZE: usize = 64 * 1024;

pub fn cp<W: Write>(src: &[u8], dst: &[u8], cwd: u32, uid: u32, gid: u32, out: &mut W) {
    let mut buffer = [0u8; BUF_SIZE];

    match storage::vfs::read_file_bytes(src, cwd, uid, gid, &mut buffer) {
        Ok(size) => match storage::vfs::write_file(dst, cwd, &buffer[..size], uid, gid) {
            Ok(()) => {}
            Err(error) => {
                writeln!(out, "cp: write failed").ok();
                report_error(error, out);
            }
        },

        Err(error) => {
            writeln!(out, "cp: read failed").ok();
            report_error(error, out);
        }
    }
}

pub fn mv<W: Write>(src: &[u8], dst: &[u8], cwd: u32, uid: u32, gid: u32, out: &mut W) {
    let mut buffer = [0u8; BUF_SIZE];

    match storage::vfs::read_file_bytes(src, cwd, uid, gid, &mut buffer) {
        Ok(size) => match storage::vfs::write_file(dst, cwd, &buffer[..size], uid, gid) {
            Ok(()) => match storage::vfs::remove(src, cwd, uid, gid) {
                Ok(()) => {}

                Err(error) => {
                    writeln!(out, "mv: source removal failed").ok();
                    report_error(error, out);
                }
            },

            Err(error) => {
                writeln!(out, "mv: destination write failed").ok();
                report_error(error, out);
            }
        },

        Err(error) => {
            writeln!(out, "mv: source read failed").ok();
            report_error(error, out);
        }
    }
}

pub fn rmdir<W: Write>(path: &[u8], cwd: u32, uid: u32, gid: u32, out: &mut W) {
    match storage::vfs::remove(path, cwd, uid, gid) {
        Ok(()) => {}

        Err(error) => {
            writeln!(out, "rmdir: failed").ok();
            report_error(error, out);
        }
    }
}

pub fn head<W: Write>(path: &[u8], lines: usize, cwd: u32, uid: u32, gid: u32, out: &mut W) {
    let mut buffer = [0u8; BUF_SIZE];

    let size = match storage::vfs::read_file_bytes(path, cwd, uid, gid, &mut buffer) {
        Ok(size) => size,

        Err(error) => {
            writeln!(out, "head: failed").ok();
            report_error(error, out);
            return;
        }
    };

    let mut count = 0usize;

    for &byte in &buffer[..size] {
        emit_byte(byte, out);

        if byte == b'\n' {
            count += 1;

            if count >= lines {
                break;
            }
        }
    }
}

pub fn tail<W: Write>(path: &[u8], lines: usize, cwd: u32, uid: u32, gid: u32, out: &mut W) {
    let mut buffer = [0u8; BUF_SIZE];

    let size = match storage::vfs::read_file_bytes(path, cwd, uid, gid, &mut buffer) {
        Ok(size) => size,

        Err(error) => {
            writeln!(out, "tail: failed").ok();
            report_error(error, out);
            return;
        }
    };

    if lines == 0 {
        return;
    }

    let mut starts = [0usize; 4096];
    let mut count = 0usize;

    starts[0] = 0;
    count = 1;

    for index in 0..size {
        if buffer[index] == b'\n' && count < starts.len() {
            starts[count] = index + 1;
            count += 1;
        }
    }

    let start = if count > lines {
        starts[count - lines]
    } else {
        0
    };

    for &byte in &buffer[start..size] {
        emit_byte(byte, out);
    }
}

pub fn wc<W: Write>(path: &[u8], cwd: u32, uid: u32, gid: u32, out: &mut W) {
    let mut buffer = [0u8; BUF_SIZE];

    let size = match storage::vfs::read_file_bytes(path, cwd, uid, gid, &mut buffer) {
        Ok(size) => size,

        Err(error) => {
            writeln!(out, "wc: failed").ok();
            report_error(error, out);
            return;
        }
    };

    let mut lines = 0usize;
    let mut words = 0usize;
    let mut in_word = false;

    for &byte in &buffer[..size] {
        if byte == b'\n' {
            lines += 1;
        }

        if byte == b' ' || byte == b'\n' || byte == b'\t' || byte == b'\r' {
            in_word = false;
        } else if !in_word {
            in_word = true;
            words += 1;
        }
    }

    writeln!(out, "{} {} {} {}", lines, words, size, bytes_str(path)).ok();
}

pub fn grep<W: Write>(pattern: &[u8], path: &[u8], cwd: u32, uid: u32, gid: u32, out: &mut W) {
    let mut buffer = [0u8; BUF_SIZE];

    let size = match storage::vfs::read_file_bytes(path, cwd, uid, gid, &mut buffer) {
        Ok(size) => size,

        Err(error) => {
            writeln!(out, "grep: failed").ok();
            report_error(error, out);
            return;
        }
    };

    if pattern.is_empty() {
        return;
    }

    let mut line_start = 0usize;

    for index in 0..=size {
        if index == size || buffer[index] == b'\n' {
            let line = &buffer[line_start..index];

            if contains(line, pattern) {
                for &byte in line {
                    emit_byte(byte, out);
                }

                writeln!(out).ok();
            }

            line_start = index + 1;
        }
    }
}

pub fn basename<W: Write>(path: &[u8], out: &mut W) {
    let mut end = path.len();

    while end > 0 && path[end - 1] == b'/' {
        end -= 1;
    }

    let mut start = 0usize;

    for index in 0..end {
        if path[index] == b'/' {
            start = index + 1;
        }
    }

    print_bytes(&path[start..end], out);

    writeln!(out).ok();
}

pub fn dirname<W: Write>(path: &[u8], out: &mut W) {
    let mut end = path.len();

    while end > 1 && path[end - 1] == b'/' {
        end -= 1;
    }

    let mut slash = None;

    for index in 0..end {
        if path[index] == b'/' {
            slash = Some(index);
        }
    }

    match slash {
        Some(0) => {
            writeln!(out, "/").ok();
        }

        Some(index) => {
            print_bytes(&path[..index], out);
            writeln!(out).ok();
        }

        None => {
            writeln!(out, ".").ok();
        }
    }
}

pub fn hostname<W: Write>(out: &mut W) {
    writeln!(out, "vibux").ok();
}

pub fn uptime<W: Write>(out: &mut W) {
    let ticks = arch::x86_64::timer_ticks();

    let seconds = ticks / 250;

    let minutes = seconds / 60;

    let hours = minutes / 60;

    writeln!(
        out,
        "{} seconds ({}:{:02}:{:02})",
        seconds,
        hours,
        minutes % 60,
        seconds % 60,
    )
    .ok();
}

pub fn date<W: Write>(out: &mut W) {
    let second = cmos(0x00);
    let minute = cmos(0x02);
    let hour = cmos(0x04);
    let day = cmos(0x07);
    let month = cmos(0x08);
    let year = cmos(0x09);

    writeln!(
        out,
        "{:02}-{:02}-20{:02} {:02}:{:02}:{:02}",
        bcd(day),
        bcd(month),
        bcd(year),
        bcd(hour),
        bcd(minute),
        bcd(second),
    )
    .ok();
}

pub fn sleep<W: Write>(seconds: u64, _out: &mut W) {
    let target = arch::x86_64::timer_ticks().saturating_add(seconds.saturating_mul(250));

    while arch::x86_64::timer_ticks() < target {
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}

pub fn env<W: Write>(out: &mut W) {
    writeln!(out, "PATH=/bin:/usr/bin").ok();

    writeln!(out, "HOME=/home/boi").ok();

    writeln!(out, "USER=boi").ok();

    writeln!(out, "SHELL=/bin/sh").ok();

    writeln!(out, "HOSTNAME=vibux").ok();
}

pub fn which<W: Write>(command: &[u8], out: &mut W) {
    if is_builtin(command) {
        writeln!(out, "{}: shell builtin", bytes_str(command)).ok();
    } else {
        writeln!(out, "{}: not found", bytes_str(command)).ok();
    }
}

pub fn true_cmd() {}

pub fn false_cmd<W: Write>(out: &mut W) {
    writeln!(out, "false").ok();
}

fn is_builtin(command: &[u8]) -> bool {
    const COMMANDS: &[&[u8]] = &[
        b"help",
        b"echo",
        b"clear",
        b"pwd",
        b"ls",
        b"cd",
        b"cat",
        b"mkdir",
        b"touch",
        b"write",
        b"rm",
        b"rmdir",
        b"cp",
        b"mv",
        b"stat",
        b"df",
        b"head",
        b"tail",
        b"wc",
        b"grep",
        b"basename",
        b"dirname",
        b"whoami",
        b"id",
        b"cpu",
        b"mem",
        b"ticks",
        b"storage",
        b"net",
        b"ping",
        b"mount",
        b"uname",
        b"hostname",
        b"uptime",
        b"date",
        b"sleep",
        b"env",
        b"which",
        b"true",
        b"false",
    ];

    for &item in COMMANDS {
        if item == command {
            return true;
        }
    }

    false
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.len() > haystack.len() {
        return false;
    }

    for start in 0..=haystack.len() - needle.len() {
        let mut matched = true;

        for index in 0..needle.len() {
            if haystack[start + index] != needle[index] {
                matched = false;
                break;
            }
        }

        if matched {
            return true;
        }
    }

    false
}

fn emit_byte<W: Write>(byte: u8, out: &mut W) {
    match byte {
        b'\n' => {
            out.write_char('\n').ok();
        }

        b'\r' => {}

        b'\t' => {
            out.write_char('\t').ok();
        }

        0x20..=0x7E => {
            out.write_char(byte as char).ok();
        }

        _ => {
            out.write_char('·').ok();
        }
    }
}

fn print_bytes<W: Write>(bytes: &[u8], out: &mut W) {
    for &byte in bytes {
        emit_byte(byte, out);
    }
}

fn bytes_str(bytes: &[u8]) -> &'static str {
    match bytes {
        b"help" => "help",
        b"echo" => "echo",
        b"clear" => "clear",
        b"pwd" => "pwd",
        b"ls" => "ls",
        b"cd" => "cd",
        b"cat" => "cat",
        b"mkdir" => "mkdir",
        b"touch" => "touch",
        b"write" => "write",
        b"rm" => "rm",
        b"rmdir" => "rmdir",
        b"cp" => "cp",
        b"mv" => "mv",
        b"stat" => "stat",
        b"df" => "df",
        b"head" => "head",
        b"tail" => "tail",
        b"wc" => "wc",
        b"grep" => "grep",
        b"basename" => "basename",
        b"dirname" => "dirname",
        b"whoami" => "whoami",
        b"id" => "id",
        b"cpu" => "cpu",
        b"mem" => "mem",
        b"ticks" => "ticks",
        b"storage" => "storage",
        b"net" => "net",
        b"ping" => "ping",
        b"mount" => "mount",
        b"uname" => "uname",
        b"hostname" => "hostname",
        b"uptime" => "uptime",
        b"date" => "date",
        b"sleep" => "sleep",
        b"env" => "env",
        b"which" => "which",
        b"true" => "true",
        b"false" => "false",
        _ => "?",
    }
}

fn bytes_str_ptr(bytes: &[u8]) -> &'static str {
    bytes_str(bytes)
}

fn bcd(value: u8) -> u8 {
    (value & 0x0F) + ((value >> 4) * 10)
}

fn cmos(register: u8) -> u8 {
    unsafe {
        core::arch::asm!(
            "out dx, al",
            in("dx") 0x70u16,
            in("al") register,
            options(
                nostack,
                preserves_flags
            )
        );

        let value: u8;

        core::arch::asm!(
            "in al, dx",
            in("dx") 0x71u16,
            lateout("al") value,
            options(
                nostack,
                preserves_flags
            )
        );

        value
    }
}

fn report_error<W: Write>(error: storage::vfs::FsError, out: &mut W) {
    let text = match error {
        storage::vfs::FsError::Offline => "filesystem offline",
        storage::vfs::FsError::Io => "I/O error",
        storage::vfs::FsError::Invalid => "invalid filesystem state",
        storage::vfs::FsError::NotFound => "no such file or directory",
        storage::vfs::FsError::Exists => "already exists",
        storage::vfs::FsError::NotDir => "not a directory",
        storage::vfs::FsError::IsDir => "is a directory",
        storage::vfs::FsError::NotEmpty => "directory not empty",
        storage::vfs::FsError::Permission => "permission denied",
        storage::vfs::FsError::NoSpace => "no space left",
        storage::vfs::FsError::TooLarge => "file too large",
    };

    writeln!(out, "{}", text).ok();
}
