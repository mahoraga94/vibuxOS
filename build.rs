use bootloader::DiskImageBuilder;
use std::env;
use std::fs::{self, OpenOptions};
use std::path::PathBuf;

// ============================================================
// VIBUX IMAGE CONFIG
// ============================================================

const BOOT_REGION_BYTES: u64 = 32 * 1024 * 1024;
const DISK_BYTES: u64 = 128 * 1024 * 1024;

// ============================================================
// BUILD
// ============================================================

fn main() {
    println!("cargo:rerun-if-env-changed=VIBUX_BUILD_MODE");

    // --------------------------------------------------------
    // Kernel artifact
    // --------------------------------------------------------

    let kernel = PathBuf::from(
        env::var_os("CARGO_BIN_FILE_KERNEL_kernel")
            .expect("Vibux build: kernel artifact dependency missing"),
    );

    // --------------------------------------------------------
    // Paths
    // --------------------------------------------------------

    let manifest = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR")
            .expect("Vibux build: CARGO_MANIFEST_DIR missing"),
    );

    let target_dir = env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest.join("target"));

    let output_dir = target_dir.join("vibux");

    fs::create_dir_all(&output_dir)
        .expect("Vibux build: cannot create target/vibux");

    let final_image = output_dir.join("vibux.img");
    let intermediate = output_dir.join("vibux-boot.img");

    // --------------------------------------------------------
    // Kernel size
    // --------------------------------------------------------

    let kernel_size = fs::metadata(&kernel)
        .expect("Vibux build: cannot stat kernel ELF")
        .len();

    println!(
        "cargo:warning=VIBUX KERNEL ELF = {} bytes ({:.2} MiB)",
        kernel_size,
        kernel_size as f64 / 1048576.0
    );

    if kernel_size >= BOOT_REGION_BYTES {
        panic!(
            "Vibux kernel ELF is {:.2} MiB and exceeds the {:.2} MiB boot region",
            kernel_size as f64 / 1048576.0,
            BOOT_REGION_BYTES as f64 / 1048576.0
        );
    }

    // --------------------------------------------------------
    // Let bootloader create the BIOS image.
    //
    // IMPORTANT:
    // Do NOT modify the MBR afterward.
    //
    // bootloader 0.11.17 creates its own BIOS layout,
    // including Stage 2 and the FAT boot partition.
    // --------------------------------------------------------

    let builder = DiskImageBuilder::new(kernel);

    builder
        .create_bios_image(&intermediate)
        .expect("Vibux build: failed to create BIOS image");

    // --------------------------------------------------------
    // Check bootloader image size
    // --------------------------------------------------------

    let boot_size = fs::metadata(&intermediate)
        .expect("Vibux build: cannot stat boot image")
        .len();

    println!(
        "cargo:warning=VIBUX BOOT IMAGE = {} bytes ({:.2} MiB)",
        boot_size,
        boot_size as f64 / 1048576.0
    );

    if boot_size > BOOT_REGION_BYTES {
        panic!(
            "Vibux boot payload is {:.2} MiB and exceeds the {:.2} MiB boot region",
            boot_size as f64 / 1048576.0,
            BOOT_REGION_BYTES as f64 / 1048576.0
        );
    }

    // --------------------------------------------------------
    // Copy bootloader image to final image
    // --------------------------------------------------------

    fs::copy(&intermediate, &final_image)
        .expect("Vibux build: failed to create final image");

    // --------------------------------------------------------
    // Extend image to fixed disk size.
    //
    // This DOES NOT modify the bootloader's MBR or filesystem.
    // --------------------------------------------------------

    let file = OpenOptions::new()
        .write(true)
        .open(&final_image)
        .expect("Vibux build: cannot open final image");

    file.set_len(DISK_BYTES)
        .expect("Vibux build: cannot extend final image");

    // --------------------------------------------------------
    // Verify final image size
    // --------------------------------------------------------

    let final_size = fs::metadata(&final_image)
        .expect("Vibux build: cannot stat final image")
        .len();

    if final_size != DISK_BYTES {
        panic!(
            "Vibux build: image size is {} bytes, expected {}",
            final_size,
            DISK_BYTES
        );
    }

    println!(
        "cargo:warning=VIBUX IMAGE = exactly {:.0} MiB",
        DISK_BYTES as f64 / 1048576.0
    );

    println!(
        "cargo:warning=VIBUX BOOT REGION LIMIT = {:.0} MiB",
        BOOT_REGION_BYTES as f64 / 1048576.0
    );

    println!(
        "cargo:warning=VIBUX: bootloader MBR left untouched for BIOS/Stage-2 compatibility"
    );

    println!(
        "cargo:rustc-env=VIBUX_IMAGE={}",
        final_image.display()
    );
}