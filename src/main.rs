use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio, exit};

const DISK_SIZE: u64 = 128 * 1024 * 1024;
fn interesting(line: &str) -> bool {
    let l = line.to_ascii_lowercase();

    l.contains("check_exception")
        || l.contains("cpu reset")
        || l.contains("guest_errors")
        || l.contains("guest error")
        || l.contains("unimp")
        || l.contains("triple fault")
        || l.contains("double fault")
        || l.contains("page fault")
        || l.contains("general protection")
        || l.contains("invalid opcode")
        || l.contains("stack fault")
        || l.contains("segment")
        || l.contains("exception")
        || l.contains("panic")
        || l.contains("fatal")
        || l.contains("[vfork")
        || l.contains("[sysret")
        || l.contains("[usermap")
        || l.contains("[userstack")
        || l.contains("[mmap")
        || l.contains("[fault")
        || l.contains("[syscall")
}

fn main() {
    let image_base = env!("VIBUX_IMAGE");
    let build_mode = option_env!("VIBUX_BUILD_MODE").unwrap_or("");

    let image = match build_mode {
        "me" | "native" | "portable" => {
            PathBuf::from(image_base).with_file_name(format!("vibux-{build_mode}.img"))
        }
        _ => PathBuf::from(image_base),
    };

    if !Path::new(&image).is_file() {
        panic!("Vibux image does not exist: {}", image.display());
    }

    let image_size = std::fs::metadata(&image)
        .expect("Vibux: cannot stat image")
        .len();

    if image_size != DISK_SIZE {
        panic!(
            "Vibux image is {} bytes ({:.2} MiB), expected exactly 128 MiB",
            image_size,
            image_size as f64 / 1048576.0
        );
    }

    let kvm = Path::new("/dev/kvm").is_file() || Path::new("/dev/kvm").exists();

    let mut qemu = Command::new("qemu-system-x86_64");

    qemu.arg("-machine")
        .arg("q35")
        .arg("-m")
        .arg("512M")
        .arg("-display")
        .arg("gtk,gl=off")
        .arg("-vga")
        .arg("std")
        .arg("-serial")
        .arg("stdio")
        .arg("-drive")
        .arg(format!(
            "file={},if=none,id=vibuxdisk,format=raw,cache=writeback,aio=threads",
            image.display()
        ))
        .arg("-device")
        .arg("nvme,serial=VIBUXROOT,drive=vibuxdisk")
        .arg("-boot")
        .arg("c")
        .arg("-no-reboot")
        .arg("-no-shutdown")
        .arg("-d")
        .arg("int,guest_errors,unimp,cpu_reset")
        .arg("-D")
        .arg("/dev/stderr")
        .arg("-msg")
        .arg("timestamp=on")
        .arg("-netdev")
        .arg("user,id=vibuxnet")
        .arg("-device")
        .arg("rtl8139,netdev=vibuxnet")
        .stdout(Stdio::inherit())
        .stderr(Stdio::piped());

    if kvm {
        qemu.arg("-accel").arg("kvm").arg("-cpu").arg("host");
    } else {
        qemu.arg("-accel")
            .arg("tcg,thread=multi,tb-size=4096")
            .arg("-cpu")
            .arg("max");
    }

    println!();
    println!("============================================================");
    println!("                    VIBUX BOOT");
    println!("============================================================");
    println!("IMAGE       : {}", image.display());
    println!("IMAGE SIZE  : 128 MiB EXACT");
    println!("ACCEL       : {}", if kvm { "KVM" } else { "TCG" });
    println!("CPU         : {}", if kvm { "host" } else { "max" });
    println!("DISPLAY     : GTK");
    println!("SERIAL      : stdio");
    println!("QEMU TRACE  : interrupts/errors/resets");
    println!("KERNEL TRACE: syscall/vfork/sysret/mmap/fault");
    println!("LOG         : ./debug.txt");
    println!("============================================================");
    println!();

    let mut child = qemu.spawn().expect("failed to launch qemu-system-x86_64");

    let stderr = child.stderr.take().expect("failed to capture QEMU stderr");

    let mut log = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open("debug.txt")
        .expect("failed to create debug.txt");

    for line in BufReader::new(stderr).lines() {
        let Ok(line) = line else {
            continue;
        };

        if interesting(&line) {
            writeln!(log, "{line}").ok();
            log.flush().ok();
            println!("[QEMU-DEBUG] {line}");
        }
    }

    let status = child.wait().expect("failed waiting for QEMU");

    println!();
    println!("============================================================");
    println!("QEMU EXITED");
    println!("STATUS : {status}");
    println!("IMAGE  : 128 MiB EXACT");
    println!("LOG    : ./debug.txt");
    println!("============================================================");

    exit(status.code().unwrap_or(1));
}
