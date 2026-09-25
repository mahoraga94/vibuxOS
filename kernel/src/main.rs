#![no_std]
#![no_main]
#![allow(warnings)]
#[macro_export]
macro_rules! println {
    ($($arg:tt)*) => {{
        #[cfg(debug_assertions)]
        {
            let mut serial = $crate::console::Serial::new();
            serial.init();
            let _ = core::fmt::Write::write_fmt(&mut serial, format_args!($($arg)*));
            let _ = core::fmt::Write::write_str(&mut serial, "\r\n");
        }
    }};
}

#[macro_export]
macro_rules! debug_writeln {
    ($dst:expr) => {{
        #[cfg(debug_assertions)]
        {
            core::fmt::Write::write_str(&mut *$dst, "\r\n")
        }
        #[cfg(not(debug_assertions))]
        {
            Ok::<(), core::fmt::Error>(())
        }
    }};
    ($dst:expr, $($arg:tt)*) => {{
        #[cfg(debug_assertions)]
        {
            core::fmt::Write::write_fmt(&mut *$dst, format_args!($($arg)*))
        }
        #[cfg(not(debug_assertions))]
        {
            Ok::<(), core::fmt::Error>(())
        }
    }};
}

#[macro_export]
macro_rules! debug_write {
    ($dst:expr, $($arg:tt)*) => {{
        #[cfg(debug_assertions)]
        {
            core::fmt::Write::write_fmt(&mut *$dst, format_args!($($arg)*))
        }
        #[cfg(not(debug_assertions))]
        {
            Ok::<(), core::fmt::Error>(())
        }
    }};
}

#[macro_export]
macro_rules! debug_write_char {
    ($dst:expr, $arg:expr) => {{
        #[cfg(debug_assertions)]
        {
            core::fmt::Write::write_char(&mut *$dst, $arg)
        }
        #[cfg(not(debug_assertions))]
        {
            Ok::<(), core::fmt::Error>(())
        }
    }};
}
mod account;
mod arch;
mod console;
mod coreutils;
mod memory;
mod net;
mod process;
mod shell;
mod storage;
mod terminal;
mod tty;
mod usb;
mod velf;
mod wifi;

use bootloader_api::config::{BootloaderConfig, Mapping};
use bootloader_api::{BootInfo, entry_point};

use core::arch::asm;
use core::fmt::Write;
use core::panic::PanicInfo;

#[macro_export]

pub static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.kernel_stack_size = 256 * 1024;
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    config.mappings.page_table_recursive = Some(Mapping::Dynamic);
    config
};

entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    arch::x86_64::init(boot_info);

    let mut serial = console::Serial::new();
    serial.init();

    writeln!(serial, "VIBUXOS KERNEL").ok();
    writeln!(serial, "initializing...").ok();

    let cpu = arch::x86_64::cpu::CpuInfo::detect();
    let memory = memory::MemoryMapReport::from_regions(&boot_info.memory_regions);

    writeln!(serial, "CPU                : {}", cpu.microarchitecture()).ok();
    writeln!(
        serial,
        "usable RAM         : {} MiB",
        memory.usable_bytes / memory::MIB
    )
    .ok();
    writeln!(serial, "usable frames      : {}", memory.usable_frames).ok();

    let physical_memory_offset = boot_info.physical_memory_offset.into_option();
    let Some(physical_memory_offset) = physical_memory_offset else {
        writeln!(serial, "physmap            : unavailable").ok();
        halt_forever();
    };

    writeln!(
        serial,
        "physmap            : 0x{:016X}",
        physical_memory_offset
    )
    .ok();

    let mut allocator = memory::FrameAllocator::new(&boot_info.memory_regions);

    let mut frame_count = 0usize;
    while frame_count < 4 {
        match allocator.allocate() {
            Some(frame) => {
                writeln!(
                    serial,
                    "frame {:02}          : 0x{:016X}",
                    frame_count,
                    frame.addr()
                )
                .ok();
                frame_count += 1;
            }
            None => break,
        }
    }

    let storage = storage::probe(&mut allocator, physical_memory_offset, &mut serial);

    writeln!(serial, "--------------------------------").ok();

    match storage.as_ref() {
        Some(report) => {
            writeln!(serial, "storage            : NVMe ONLINE").ok();
            writeln!(
                serial,
                "namespace          : {} MiB",
                report.namespace_bytes / memory::MIB
            )
            .ok();
            writeln!(serial, "LBA                : {} bytes", report.lba_bytes).ok();

            match report.btrfs.as_ref() {
                Some(info) => {
                    writeln!(serial, "filesystem         : BTRFS").ok();
                    writeln!(
                        serial,
                        "Btrfs checksum     : {}",
                        if info.checksum_valid {
                            "VALID"
                        } else {
                            "INVALID"
                        }
                    )
                    .ok();
                }
                None => {
                    writeln!(serial, "filesystem         : unknown").ok();
                }
            }
        }
        None => {
            writeln!(serial, "storage            : OFFLINE").ok();
        }
    }

    writeln!(serial, "--------------------------------").ok();
    writeln!(serial, "Vibux kernel ready.").ok();

    let storage_online = storage.is_some();

    let (namespace_mib, lba_bytes, btrfs_valid, btrfs_nodesize, btrfs_used_mib) =
        match storage.as_ref() {
            Some(report) => match report.btrfs.as_ref() {
                Some(info) => (
                    report.namespace_bytes / memory::MIB,
                    report.lba_bytes,
                    info.checksum_valid,
                    info.nodesize as u64,
                    info.bytes_used / memory::MIB,
                ),
                None => (
                    report.namespace_bytes / memory::MIB,
                    report.lba_bytes,
                    false,
                    0,
                    0,
                ),
            },
            None => (0, 0, false, 0, 0),
        };
    let _network_online = net::init(&mut allocator, physical_memory_offset, &mut serial);

    let shell_context = shell::ShellContext {
        cpu: cpu.microarchitecture(),
        ram_mib: memory.usable_bytes / memory::MIB,
        frames: memory.usable_frames as usize,
        storage_online,
        session: account::Session::initial(),
        namespace_mib,
        lba_bytes,
        btrfs_valid,
        btrfs_nodesize,
        btrfs_used_mib,
    };

    if let Some(framebuffer) = boot_info.framebuffer.as_mut() {
        let info = framebuffer.info();
        let mut screen = terminal::FrameTerminal::new(framebuffer);
        tty::attach(&mut screen);
        screen.set_serial_mirror(true);

        writeln!(screen, "VIBUXOS").ok();
        writeln!(screen, "KERNEL ONLINE").ok();
        writeln!(screen).ok();
        writeln!(screen, "CPU        {}", cpu.microarchitecture()).ok();
        writeln!(
            screen,
            "RAM        {} MiB",
            memory.usable_bytes / memory::MIB
        )
        .ok();
        writeln!(screen, "FRAMES     {}", memory.usable_frames).ok();
        writeln!(screen, "DISPLAY    {}x{}", info.width, info.height).ok();
        writeln!(screen).ok();

        match storage.as_ref() {
            Some(report) => {
                writeln!(screen, "STORAGE    NVMe ONLINE").ok();
                writeln!(
                    screen,
                    "CAPACITY   {} MiB",
                    report.namespace_bytes / memory::MIB
                )
                .ok();
                writeln!(screen, "LBA        {} B", report.lba_bytes).ok();
                match report.btrfs.as_ref() {
                    Some(info) => {
                        writeln!(screen, "FILESYSTEM BTRFS").ok();
                        writeln!(
                            screen,
                            "BTRFS CRC  {}",
                            if info.checksum_valid { "OK" } else { "BAD" }
                        )
                        .ok();
                        writeln!(screen, "BTRFS NODE {}", info.nodesize).ok();
                        writeln!(screen, "BTRFS USED {} MiB", info.bytes_used / memory::MIB).ok();
                    }
                    None => {
                        writeln!(screen, "FILESYSTEM VIBUXFS").ok();
                    }
                }
            }
            None => {
                writeln!(screen, "STORAGE    NOT FOUND").ok();
            }
        }

        writeln!(screen).ok();
        writeln!(screen, "MEMORY     READY").ok();
        writeln!(screen, "STORAGE    READY").ok();
        writeln!(screen, "KERNEL     READY").ok();

        let _ = &shell_context;

        loop {
            writeln!(screen, "\nlaunching /bin/bash ...\n").ok();

            velf::run_bash(&mut allocator, physical_memory_offset, &mut serial);

            writeln!(screen, "\n[bash exited]\n").ok();
            writeln!(serial, "[SHELL] no automatic restart; entering kernel halt").ok();

            halt_forever();
        }
    }

    halt_forever();
}

#[inline(never)]
fn halt_forever() -> ! {
    loop {
        unsafe {
            asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let mut serial = console::Serial::new();
    serial.init();
    writeln!(serial, "\nVIBUX KERNEL PANIC").ok();
    writeln!(serial, "{info}").ok();
    halt_forever();
}
