use crate::account::Session;
use crate::arch;
use crate::coreutils;
use crate::net;
use crate::storage;
use crate::terminal::FrameTerminal;
use core::arch::asm;
use core::fmt::Write;

const LINE_MAX: usize = 256;
const ARG_MAX: usize = 16;
const ARG_MAX_LEN: usize = 128;
const PATH_MAX: usize = 256;

pub struct ShellContext {
    pub cpu: &'static str,
    pub ram_mib: u64,
    pub frames: usize,
    pub storage_online: bool,
    pub session: Session,
    pub namespace_mib: u64,
    pub lba_bytes: u64,
    pub btrfs_valid: bool,
    pub btrfs_nodesize: u64,
    pub btrfs_used_mib: u64,
}

struct Parsed {
    argc: usize,
    spans: [(u16, u16); ARG_MAX],
}

impl Parsed {
    const fn new() -> Self {
        Self {
            argc: 0,
            spans: [(0, 0); ARG_MAX],
        }
    }

    fn arg<'a>(&self, line: &'a [u8; LINE_MAX], index: usize) -> &'a [u8] {
        let (start, end) = self.spans[index];
        &line[start as usize..end as usize]
    }
}

struct Shell {
    context: ShellContext,
    cwd_inode: u32,
    cwd_path: [u8; PATH_MAX],
    cwd_len: usize,
    line: [u8; LINE_MAX],
    line_len: usize,
    shift: bool,
    ctrl: bool,
    caps: bool,
}

pub fn run(screen: &mut FrameTerminal<'_>, context: ShellContext) -> ! {
    unsafe {
        asm!("sti", options(nomem, nostack, preserves_flags));
    }

    screen.set_serial_mirror(true);

    let cwd = b"/home/boi";
    let cwd_inode = match storage::vfs::resolve(cwd, storage::vfs::root_inode()) {
        Ok(inode) => inode,
        Err(_) => storage::vfs::root_inode(),
    };

    let mut shell = Shell {
        context,
        cwd_inode,
        cwd_path: [0; PATH_MAX],
        cwd_len: cwd.len(),
        line: [0; LINE_MAX],
        line_len: 0,
        shift: false,
        ctrl: false,
        caps: false,
    };
    shell.cwd_path[..cwd.len()].copy_from_slice(cwd);

    writeln!(screen).ok();
    writeln!(screen, "Vibux shell").ok();
    writeln!(screen, "Type `help` for commands.").ok();

    loop {
        shell.prompt(screen);

        loop {
            if let Some(byte) = crate::console::serial_read_byte() {
                if shell.handle_serial_byte(byte, screen) {
                    break;
                }
            } else if let Some(scancode) = arch::x86_64::pop_keyboard_scancode() {
                if shell.handle_scancode(scancode, screen) {
                    break;
                }
            } else {
                shell.hlt();
            }
        }
    }
}

impl Shell {
    fn handle_serial_byte(&mut self, byte: u8, screen: &mut FrameTerminal<'_>) -> bool {
        match byte {
            b'\r' | b'\n' => {
                writeln!(screen).ok();
                self.execute(screen);
                self.line_len = 0;
                true
            }

            0x08 | 0x7f => {
                if self.line_len > 0 {
                    self.line_len -= 1;
                    screen.backspace();
                }
                false
            }

            0x03 => {
                self.line_len = 0;
                writeln!(screen).ok();
                true
            }

            0x0c => {
                self.line_len = 0;
                writeln!(screen).ok();
                self.print_prompt(screen);
                false
            }

            0x04 => {
                self.line_len = 0;
                writeln!(screen).ok();
                true
            }

            b' '..=b'~' => {
                if self.line_len < LINE_MAX - 1 {
                    self.line[self.line_len] = byte;
                    self.line_len += 1;
                    screen.write_char(byte as char).ok();
                }

                false
            }

            _ => false,
        }
    }

    fn prompt(&self, screen: &mut FrameTerminal<'_>) {
        let user = self.context.session.user();
        let credentials = self.context.session.credentials();
        let marker = if credentials.is_root() { '#' } else { '$' };
        write!(screen, "{}@vibux:", user.name).ok();
        for &byte in &self.cwd_path[..self.cwd_len] {
            screen.write_char(byte as char).ok();
        }
        write!(screen, "{} ", marker).ok();
    }

    fn handle_scancode(&mut self, scancode: u8, screen: &mut FrameTerminal<'_>) -> bool {
        let released = scancode & 0x80 != 0;
        let code = scancode & 0x7f;

        match code {
            0x2a | 0x36 => {
                self.shift = !released;
                return false;
            }
            0x1d => {
                self.ctrl = !released;
                return false;
            }
            0x3a if !released => {
                self.caps = !self.caps;
                return false;
            }
            _ if released => return false,
            _ => {}
        }

        if self.ctrl {
            match code {
                0x2e => {
                    self.line_len = 0;
                    writeln!(screen).ok();
                    return true;
                }
                0x26 => {
                    self.line_len = 0;
                    writeln!(screen).ok();
                    self.print_prompt(screen);
                    return false;
                }
                0x20 => {
                    self.line_len = 0;
                    writeln!(screen).ok();
                    return true;
                }
                _ => {}
            }
        }

        match code {
            0x1c => {
                writeln!(screen).ok();
                self.execute(screen);
                self.line_len = 0;
                return true;
            }
            0x0e => {
                if self.line_len > 0 {
                    self.line_len -= 1;
                    screen.backspace();
                }
                false
            }
            0x0f => {
                screen.write_char('\t').ok();
                false
            }
            _ => {
                if let Some(character) = self.decode_key(code) {
                    if self.line_len < LINE_MAX - 1 {
                        self.line[self.line_len] = character as u8;
                        self.line_len += 1;
                        screen.write_char(character).ok();
                    }
                }
                false
            }
        }
    }

    fn print_prompt(&self, screen: &mut FrameTerminal<'_>) {
        let user = self.context.session.user();
        let credentials = self.context.session.credentials();
        let marker = if credentials.is_root() { '#' } else { '$' };
        write!(screen, "{}@vibux:", user.name).ok();
        for &byte in &self.cwd_path[..self.cwd_len] {
            screen.write_char(byte as char).ok();
        }
        write!(screen, "{} ", marker).ok();
    }

    fn decode_key(&self, code: u8) -> Option<char> {
        let shifted = self.shift;
        let caps = self.caps;
        let ch = match code {
            0x02 => {
                if shifted {
                    '!'
                } else {
                    '1'
                }
            }
            0x03 => {
                if shifted {
                    '@'
                } else {
                    '2'
                }
            }
            0x04 => {
                if shifted {
                    '#'
                } else {
                    '3'
                }
            }
            0x05 => {
                if shifted {
                    '$'
                } else {
                    '4'
                }
            }
            0x06 => {
                if shifted {
                    '%'
                } else {
                    '5'
                }
            }
            0x07 => {
                if shifted {
                    '^'
                } else {
                    '6'
                }
            }
            0x08 => {
                if shifted {
                    '&'
                } else {
                    '7'
                }
            }
            0x09 => {
                if shifted {
                    '*'
                } else {
                    '8'
                }
            }
            0x0a => {
                if shifted {
                    '('
                } else {
                    '9'
                }
            }
            0x0b => {
                if shifted {
                    ')'
                } else {
                    '0'
                }
            }
            0x0c => {
                if shifted {
                    '_'
                } else {
                    '-'
                }
            }
            0x0d => {
                if shifted {
                    '+'
                } else {
                    '='
                }
            }
            0x10 => letter('q', caps, shifted),
            0x11 => letter('w', caps, shifted),
            0x12 => letter('e', caps, shifted),
            0x13 => letter('r', caps, shifted),
            0x14 => letter('t', caps, shifted),
            0x15 => letter('y', caps, shifted),
            0x16 => letter('u', caps, shifted),
            0x17 => letter('i', caps, shifted),
            0x18 => letter('o', caps, shifted),
            0x19 => letter('p', caps, shifted),
            0x1a => {
                if shifted {
                    '{'
                } else {
                    '['
                }
            }
            0x1b => {
                if shifted {
                    '}'
                } else {
                    ']'
                }
            }
            0x1e => letter('a', caps, shifted),
            0x1f => letter('s', caps, shifted),
            0x20 => letter('d', caps, shifted),
            0x21 => letter('f', caps, shifted),
            0x22 => letter('g', caps, shifted),
            0x23 => letter('h', caps, shifted),
            0x24 => letter('j', caps, shifted),
            0x25 => letter('k', caps, shifted),
            0x26 => letter('l', caps, shifted),
            0x27 => {
                if shifted {
                    ':'
                } else {
                    ';'
                }
            }
            0x28 => {
                if shifted {
                    '"'
                } else {
                    '\''
                }
            }
            0x29 => {
                if shifted {
                    '~'
                } else {
                    '`'
                }
            }
            0x2b => {
                if shifted {
                    '|'
                } else {
                    '\\'
                }
            }
            0x2c => letter('z', caps, shifted),
            0x2d => letter('x', caps, shifted),
            0x2e => letter('c', caps, shifted),
            0x2f => letter('v', caps, shifted),
            0x30 => letter('b', caps, shifted),
            0x31 => letter('n', caps, shifted),
            0x32 => letter('m', caps, shifted),
            0x33 => {
                if shifted {
                    '<'
                } else {
                    ','
                }
            }
            0x34 => {
                if shifted {
                    '>'
                } else {
                    '.'
                }
            }
            0x35 => {
                if shifted {
                    '?'
                } else {
                    '/'
                }
            }
            0x39 => ' ',
            _ => return None,
        };
        Some(ch)
    }

    fn execute(&mut self, screen: &mut FrameTerminal<'_>) -> bool {
        let parsed = parse_line(&mut self.line, self.line_len);
        if parsed.argc == 0 {
            return false;
        }
        let command = parsed.arg(&self.line, 0);

        if bytes_eq(command, b"help") {
            writeln!(
            screen,
            "help bash sh echo shutdown reboot poweroff mount clear pwd ls cd cat mkdir touch write rm rmdir cp mv head tail wc grep basename dirname hostname uptime date sleep env which true false stat df whoami id cpu mem ticks storage uname mount net ping"
        ).ok();
        } else if bytes_eq(command, b"bash") || bytes_eq(command, b"/bin/bash") {
            self.cmd_bash(screen);
        } else if bytes_eq(command, b"sh") {
            self.cmd_sh(&parsed, screen);
        } else if bytes_eq(command, b"echo") {
            self.cmd_echo(&parsed, screen);
        } else if bytes_eq(command, b"clear") {
            for _ in 0..40 {
                writeln!(screen).ok();
            }
        } else if bytes_eq(command, b"pwd") {
            self.print_path(screen);
        } else if bytes_eq(command, b"ls") {
            let path = if parsed.argc > 1 {
                parsed.arg(&self.line, 1)
            } else {
                b"."
            };
            let (uid, gid) = self.ids();
            report_fs_error(
                storage::vfs::list(path, self.cwd_inode, uid, gid, screen),
                screen,
            );
        } else if bytes_eq(command, b"cd") {
            self.cmd_cd(&parsed, screen);
        } else if bytes_eq(command, b"cat") {
            if parsed.argc < 2 {
                writeln!(screen, "cat: missing operand").ok();
            } else {
                let (uid, gid) = self.ids();
                report_fs_error(
                    storage::vfs::read_file(
                        parsed.arg(&self.line, 1),
                        self.cwd_inode,
                        uid,
                        gid,
                        screen,
                    ),
                    screen,
                );
            }
        } else if bytes_eq(command, b"mkdir") {
            if parsed.argc < 2 {
                writeln!(screen, "mkdir: missing operand").ok();
            } else {
                let (uid, gid) = self.ids();
                report_fs_error(
                    storage::vfs::make_dir(parsed.arg(&self.line, 1), self.cwd_inode, uid, gid),
                    screen,
                );
            }
        } else if bytes_eq(command, b"touch") {
            if parsed.argc < 2 {
                writeln!(screen, "touch: missing operand").ok();
            } else {
                let (uid, gid) = self.ids();
                report_fs_error(
                    storage::vfs::touch(parsed.arg(&self.line, 1), self.cwd_inode, uid, gid),
                    screen,
                );
            }
        } else if bytes_eq(command, b"write") {
            self.cmd_write(&parsed, screen);
        } else if bytes_eq(command, b"rm") {
            if parsed.argc < 2 {
                writeln!(screen, "rm: missing operand").ok();
            } else {
                let (uid, gid) = self.ids();
                report_fs_error(
                    storage::vfs::remove(parsed.arg(&self.line, 1), self.cwd_inode, uid, gid),
                    screen,
                );
            }
        } else if bytes_eq(command, b"stat") {
            if parsed.argc < 2 {
                writeln!(screen, "stat: missing operand").ok();
            } else {
                let (uid, gid) = self.ids();
                report_fs_error(
                    storage::vfs::stat(parsed.arg(&self.line, 1), self.cwd_inode, uid, gid, screen),
                    screen,
                );
            }
        } else if bytes_eq(command, b"df") {
            report_fs_error(storage::vfs::df(screen), screen);
        } else if bytes_eq(command, b"whoami") {
            let user = self.context.session.user();
            writeln!(screen, "{}", user.name).ok();
        } else if bytes_eq(command, b"id") {
            let user = self.context.session.user();
            let credentials = self.context.session.credentials();
            let uid = if credentials.is_root() { 0 } else { 1000 };
            writeln!(
                screen,
                "uid={}({}) gid={}({})",
                uid, user.name, uid, user.name
            )
            .ok();
        } else if bytes_eq(command, b"cpu") {
            writeln!(screen, "{}", self.context.cpu).ok();
        } else if bytes_eq(command, b"mem") {
            writeln!(
                screen,
                "usable={} MiB frames={}",
                self.context.ram_mib, self.context.frames
            )
            .ok();
        } else if bytes_eq(command, b"ticks") {
            writeln!(screen, "{}", arch::x86_64::timer_ticks()).ok();
        } else if bytes_eq(command, b"storage") {
            writeln!(
                screen,
                "NVMe={} filesystem={} capacity={} MiB LBA={} bytes",
                if self.context.storage_online {
                    "ONLINE"
                } else {
                    "OFFLINE"
                },
                if storage::vfs::mounted() {
                    "VIBUXFS"
                } else {
                    "OFFLINE"
                },
                self.context.namespace_mib,
                self.context.lba_bytes
            )
            .ok();
        } else if bytes_eq(command, b"net") {
            net::status(screen);
        } else if bytes_eq(command, b"ping") {
            if parsed.argc < 2 {
                writeln!(screen, "ping: missing IPv4 address").ok();
            } else {
                net::ping(parsed.arg(&self.line, 1), screen);
            }
        } else if bytes_eq(command, b"shutdown")
            || bytes_eq(command, b"poweroff")
            || bytes_eq(command, b"halt")
        {
            writeln!(screen, "Shutting down...").ok();
            crate::arch::x86_64::power::shutdown();
        } else if bytes_eq(command, b"reboot") {
            writeln!(screen, "Rebooting...").ok();
            crate::arch::x86_64::power::reboot();
        } else if bytes_eq(command, b"mount") {
            writeln!(screen, "Mounts:").ok();
            storage::vfs::list_mounts(screen);
            writeln!(screen, "/ on VIBUXFS RW").ok();
        } else if bytes_eq(command, b"mount") {
            writeln!(
                screen,
                "VIBUXFS {}",
                if storage::vfs::mounted() {
                    "mounted"
                } else {
                    "offline"
                }
            )
            .ok();
        } else if bytes_eq(command, b"uname") {
            writeln!(screen, "VibuxOS vibux 0.1 x86_64").ok();
        } else if bytes_eq(command, b"cp") {
            if parsed.argc < 3 {
                writeln!(screen, "cp: usage: cp SOURCE DEST").ok();
            } else {
                let (uid, gid) = self.ids();
                coreutils::cp(
                    parsed.arg(&self.line, 1),
                    parsed.arg(&self.line, 2),
                    self.cwd_inode,
                    uid,
                    gid,
                    screen,
                );
            }
        } else if bytes_eq(command, b"mv") {
            if parsed.argc < 3 {
                writeln!(screen, "mv: usage: mv SOURCE DEST").ok();
            } else {
                let (uid, gid) = self.ids();
                coreutils::mv(
                    parsed.arg(&self.line, 1),
                    parsed.arg(&self.line, 2),
                    self.cwd_inode,
                    uid,
                    gid,
                    screen,
                );
            }
        } else if bytes_eq(command, b"rmdir") {
            if parsed.argc < 2 {
                writeln!(screen, "rmdir: missing operand").ok();
            } else {
                let (uid, gid) = self.ids();
                coreutils::rmdir(parsed.arg(&self.line, 1), self.cwd_inode, uid, gid, screen);
            }
        } else if bytes_eq(command, b"head") {
            if parsed.argc < 2 {
                writeln!(screen, "head: usage: head FILE [LINES]").ok();
            } else {
                let lines = if parsed.argc >= 3 {
                    parse_usize(parsed.arg(&self.line, 2)).unwrap_or(10)
                } else {
                    10
                };
                let (uid, gid) = self.ids();
                coreutils::head(
                    parsed.arg(&self.line, 1),
                    lines,
                    self.cwd_inode,
                    uid,
                    gid,
                    screen,
                );
            }
        } else if bytes_eq(command, b"tail") {
            if parsed.argc < 2 {
                writeln!(screen, "tail: usage: tail FILE [LINES]").ok();
            } else {
                let lines = if parsed.argc >= 3 {
                    parse_usize(parsed.arg(&self.line, 2)).unwrap_or(10)
                } else {
                    10
                };
                let (uid, gid) = self.ids();
                coreutils::tail(
                    parsed.arg(&self.line, 1),
                    lines,
                    self.cwd_inode,
                    uid,
                    gid,
                    screen,
                );
            }
        } else if bytes_eq(command, b"wc") {
            if parsed.argc < 2 {
                writeln!(screen, "wc: missing operand").ok();
            } else {
                let (uid, gid) = self.ids();
                coreutils::wc(parsed.arg(&self.line, 1), self.cwd_inode, uid, gid, screen);
            }
        } else if bytes_eq(command, b"grep") {
            if parsed.argc < 3 {
                writeln!(screen, "grep: usage: grep PATTERN FILE").ok();
            } else {
                let (uid, gid) = self.ids();
                coreutils::grep(
                    parsed.arg(&self.line, 1),
                    parsed.arg(&self.line, 2),
                    self.cwd_inode,
                    uid,
                    gid,
                    screen,
                );
            }
        } else if bytes_eq(command, b"basename") {
            if parsed.argc < 2 {
                writeln!(screen, "basename: missing operand").ok();
            } else {
                coreutils::basename(parsed.arg(&self.line, 1), screen);
            }
        } else if bytes_eq(command, b"dirname") {
            if parsed.argc < 2 {
                writeln!(screen, "dirname: missing operand").ok();
            } else {
                coreutils::dirname(parsed.arg(&self.line, 1), screen);
            }
        } else if bytes_eq(command, b"hostname") {
            coreutils::hostname(screen);
        } else if bytes_eq(command, b"uptime") {
            coreutils::uptime(screen);
        } else if bytes_eq(command, b"date") {
            coreutils::date(screen);
        } else if bytes_eq(command, b"sleep") {
            let seconds = if parsed.argc >= 2 {
                parse_u64(parsed.arg(&self.line, 1)).unwrap_or(1)
            } else {
                1
            };
            coreutils::sleep(seconds, screen);
        } else if bytes_eq(command, b"env") {
            coreutils::env(screen);
        } else if bytes_eq(command, b"which") {
            if parsed.argc < 2 {
                writeln!(screen, "which: missing operand").ok();
            } else {
                coreutils::which(parsed.arg(&self.line, 1), screen);
            }
        } else if bytes_eq(command, b"true") {
            coreutils::true_cmd();
        } else if bytes_eq(command, b"false") {
            coreutils::false_cmd(screen);
        } else {
            write!(screen, "{}: command not found", bytes_to_str(command)).ok();
            writeln!(screen).ok();
        }

        false
    }

    fn cmd_bash(&mut self, screen: &mut FrameTerminal<'_>) {
        writeln!(screen, "Launching /bin/bash...").ok();
        let mut serial = crate::console::Serial::new();
        serial.init();
        let code = crate::velf::run_bash_again(&mut serial);
        writeln!(screen, "bash exited with status {}", code).ok();
    }

    fn cmd_sh(&mut self, parsed: &Parsed, screen: &mut FrameTerminal<'_>) {
        if parsed.argc == 1 {
            writeln!(screen, "Vibux /bin/sh").ok();
            writeln!(screen, "kernel-hosted simulated Bourne shell").ok();
            writeln!(screen, "use: sh -c \"command\"").ok();
            return;
        }

        if parsed.argc >= 3 && bytes_eq(parsed.arg(&self.line, 1), b"-c") {
            let command = parsed.arg(&self.line, 2);

            if command.is_empty() {
                return;
            }

            let len = core::cmp::min(command.len(), LINE_MAX - 1);

            let mut nested = [0u8; LINE_MAX];
            nested[..len].copy_from_slice(&command[..len]);

            self.line = nested;
            self.line_len = len;

            self.execute(screen);

            self.line_len = 0;
            return;
        }

        writeln!(screen, "sh: usage: sh [-c \"command\"]").ok();
    }

    fn cmd_echo(&self, parsed: &Parsed, screen: &mut FrameTerminal<'_>) {
        for index in 1..parsed.argc {
            if index != 1 {
                screen.write_char(' ').ok();
            }
            write!(screen, "{}", bytes_to_str(parsed.arg(&self.line, index))).ok();
        }
        writeln!(screen).ok();
    }

    fn cmd_cd(&mut self, parsed: &Parsed, screen: &mut FrameTerminal<'_>) {
        let target = if parsed.argc < 2 {
            b"/home/boi"
        } else {
            parsed.arg(&self.line, 1)
        };

        let mut normalized = [0u8; PATH_MAX];
        let len = normalize_path(&self.cwd_path[..self.cwd_len], target, &mut normalized);
        match storage::vfs::resolve(&normalized[..len], storage::vfs::root_inode()) {
            Ok(inode) => match storage::vfs::stat_kind(inode) {
                Ok(2) => {
                    self.cwd_inode = inode;
                    self.cwd_path = normalized;
                    self.cwd_len = len;
                }
                Ok(_) => {
                    writeln!(screen, "cd: not a directory").ok();
                }
                Err(error) => report_fs_error::<(), _>(Err(error), screen),
            },
            Err(error) => report_fs_error::<(), _>(Err(error), screen),
        }
    }

    fn cmd_write(&self, parsed: &Parsed, screen: &mut FrameTerminal<'_>) {
        if parsed.argc < 3 {
            writeln!(screen, "write: usage: write FILE TEXT").ok();
            return;
        }
        let mut data = [0u8; LINE_MAX];
        let mut len = 0usize;
        for index in 2..parsed.argc {
            if index != 2 && len < data.len() {
                data[len] = b' ';
                len += 1;
            }
            let arg = parsed.arg(&self.line, index);
            let copy_len = core::cmp::min(arg.len(), data.len().saturating_sub(len));
            data[len..len + copy_len].copy_from_slice(&arg[..copy_len]);
            len += copy_len;
        }
        let (uid, gid) = self.ids();
        report_fs_error(
            storage::vfs::write_file(
                parsed.arg(&self.line, 1),
                self.cwd_inode,
                &data[..len],
                uid,
                gid,
            ),
            screen,
        );
    }

    fn ids(&self) -> (u32, u32) {
        if self.context.session.credentials().is_root() {
            (0, 0)
        } else {
            (1000, 1000)
        }
    }

    fn print_path(&self, screen: &mut FrameTerminal<'_>) {
        for &byte in &self.cwd_path[..self.cwd_len] {
            screen.write_char(byte as char).ok();
        }
        writeln!(screen).ok();
    }

    fn hlt(&self) {
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}

fn report_fs_error<T, W: Write>(result: Result<T, storage::vfs::FsError>, out: &mut W) {
    if let Err(error) = result {
        let text = match error {
            storage::vfs::FsError::Offline => "filesystem offline",
            storage::vfs::FsError::Io => "I/O error",
            storage::vfs::FsError::Invalid => "invalid path or filesystem",
            storage::vfs::FsError::NotFound => "no such file or directory",
            storage::vfs::FsError::Exists => "file or directory already exists",
            storage::vfs::FsError::NotDir => "not a directory",
            storage::vfs::FsError::IsDir => "is a directory",
            storage::vfs::FsError::NotEmpty => "directory not empty",
            storage::vfs::FsError::Permission => "permission denied",
            storage::vfs::FsError::NoSpace => "no space left on device",
            storage::vfs::FsError::TooLarge => "file too large",
        };
        writeln!(out, "{}", text).ok();
    }
}

fn letter(base: char, caps: bool, shifted: bool) -> char {
    if caps ^ shifted {
        base.to_ascii_uppercase()
    } else {
        base
    }
}

fn bytes_eq(a: &[u8], b: &[u8]) -> bool {
    a == b
}

fn bytes_to_str(data: &[u8]) -> &str {
    core::str::from_utf8(data).unwrap_or("?")
}

#[inline(always)]
fn parse_usize(value: &[u8]) -> Option<usize> {
    if value.is_empty() {
        return None;
    }

    let mut result = 0usize;

    for &byte in value {
        if !byte.is_ascii_digit() {
            return None;
        }

        result = result
            .checked_mul(10)?
            .checked_add((byte - b'0') as usize)?;
    }

    Some(result)
}

#[inline(always)]
fn parse_u64(value: &[u8]) -> Option<u64> {
    if value.is_empty() {
        return None;
    }

    let mut result = 0u64;

    for &byte in value {
        if !byte.is_ascii_digit() {
            return None;
        }

        result = result.checked_mul(10)?.checked_add((byte - b'0') as u64)?;
    }

    Some(result)
}

fn parse_line(line: &mut [u8; LINE_MAX], len: usize) -> Parsed {
    let mut parsed = Parsed::new();
    let mut read = 0usize;
    let mut write_pos = 0usize;

    while read < len {
        while read < len && line[read].is_ascii_whitespace() {
            read += 1;
        }
        if read >= len || parsed.argc >= ARG_MAX {
            break;
        }

        let start = write_pos;
        let mut quote = 0u8;

        while read < len {
            let c = line[read];

            if quote == 0 && c.is_ascii_whitespace() {
                break;
            }

            if c == b'\\' {
                read += 1;
                if read < len {
                    line[write_pos] = line[read];
                    write_pos += 1;
                    read += 1;
                }
                continue;
            }

            if c == b'\'' || c == b'"' {
                if quote == 0 {
                    quote = c;
                } else if quote == c {
                    quote = 0;
                } else {
                    line[write_pos] = c;
                    write_pos += 1;
                }
                read += 1;
                continue;
            }

            line[write_pos] = c;
            write_pos += 1;
            read += 1;
        }

        let end = core::cmp::min(write_pos, start + ARG_MAX_LEN);
        parsed.spans[parsed.argc] = (start as u16, end as u16);
        parsed.argc += 1;

        while read < len && line[read].is_ascii_whitespace() {
            read += 1;
        }
    }

    parsed
}

fn normalize_path(cwd: &[u8], input: &[u8], out: &mut [u8; PATH_MAX]) -> usize {
    let absolute = input.first() == Some(&b'/');
    let mut len = if absolute {
        out[0] = b'/';
        1
    } else {
        let copy_len = core::cmp::min(cwd.len(), PATH_MAX);
        out[..copy_len].copy_from_slice(&cwd[..copy_len]);
        copy_len
    };

    let mut i = 0usize;
    while i < input.len() {
        while i < input.len() && input[i] == b'/' {
            i += 1;
        }
        if i >= input.len() {
            break;
        }
        let start = i;
        while i < input.len() && input[i] != b'/' {
            i += 1;
        }
        let part = &input[start..i];

        if part == b"." || part.is_empty() {
            continue;
        }

        if part == b".." {
            if len > 1 {
                len -= 1;
                while len > 1 && out[len - 1] != b'/' {
                    len -= 1;
                }
            }
            continue;
        }

        if len > 1 && out[len - 1] != b'/' {
            if len < PATH_MAX {
                out[len] = b'/';
                len += 1;
            }
        }
        let copy_len = core::cmp::min(part.len(), PATH_MAX.saturating_sub(len));
        out[len..len + copy_len].copy_from_slice(&part[..copy_len]);
        len += copy_len;
    }

    if len == 0 {
        out[0] = b'/';
        1
    } else {
        len
    }
}
