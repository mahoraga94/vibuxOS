use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn trace_runtime_dependencies(path: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let output = Command::new("ldd")
        .arg(path)
        .env_remove("LD_LIBRARY_PATH")
        .env_remove("LD_PRELOAD")
        .env_remove("LD_AUDIT")
        .output()
        .map_err(|error| format!("cannot execute ldd for {}: {}", path.display(), error))?;

    if !output.status.success() {
        return Err(format!(
            "ldd failed for {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut results = Vec::<(String, PathBuf)>::new();

    for line in stdout.lines() {
        let line = line.trim();

        if line.is_empty() || line.contains("linux-vdso") {
            continue;
        }

        let Some((name, rhs)) = line.split_once("=>") else {
            let mut fields = line.split_whitespace();

            let Some(first) = fields.next() else {
                continue;
            };

            if first.starts_with('/') {
                let candidate = Path::new(first);

                if candidate.is_file() {
                    let canonical = fs::canonicalize(candidate).map_err(|error| {
                        format!("cannot canonicalize {}: {}", candidate.display(), error)
                    })?;

                    let guest_name = canonical
                        .file_name()
                        .and_then(|value| value.to_str())
                        .ok_or_else(|| {
                            format!("dependency has no UTF-8 filename: {}", canonical.display())
                        })?
                        .to_string();

                    if !results.iter().any(|(_, existing)| existing == &canonical) {
                        results.push((guest_name, canonical));
                    }
                }
            }

            continue;
        };

        let name = name.trim();

        let Some(candidate_string) = rhs.split_whitespace().next() else {
            continue;
        };

        let candidate = Path::new(candidate_string);

        if !candidate.is_absolute() || !candidate.is_file() {
            continue;
        }

        let canonical = fs::canonicalize(candidate)
            .map_err(|error| format!("cannot canonicalize {}: {}", candidate.display(), error))?;

        if !results.iter().any(|(existing_name, existing_path)| {
            existing_name == name && existing_path == &canonical
        }) {
            results.push((name.to_string(), canonical));
        }
    }

    Ok(results)
}

fn stage_elf_for_embedding(path: &Path, out_dir: &Path, index: usize) -> Result<PathBuf, String> {
    let dir = out_dir.join("vibux-elf-assets");

    fs::create_dir_all(&dir)
        .map_err(|error| format!("cannot create {}: {}", dir.display(), error))?;

    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("elf");

    let staged = dir.join(format!("{index:04}-{filename}"));

    fs::copy(path, &staged)
        .map_err(|error| format!("cannot stage {}: {}", path.display(), error))?;

    let strip_tool = if Command::new("llvm-strip")
        .arg("--version")
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
    {
        "llvm-strip"
    } else {
        "strip"
    };

    let status = Command::new(strip_tool)
        .arg("--strip-unneeded")
        .arg(&staged)
        .status()
        .map_err(|error| {
            format!(
                "cannot execute {} on {}: {}",
                strip_tool,
                staged.display(),
                error
            )
        })?;

    if !status.success() {
        return Err(format!("{} failed for {}", strip_tool, staged.display()));
    }

    Ok(staged)
}

fn add_asset(assets: &mut Vec<BinaryAsset>, canonical: PathBuf, guest: String) {
    if assets
        .iter()
        .any(|x| x.canonical == canonical && x.guest == guest)
    {
        return;
    }

    if let Some(existing) = assets.iter().find(|x| x.guest == guest) {
        if existing.canonical != canonical {
            panic!(
                "VELF build: guest path collision: {} maps to both {} and {}",
                guest,
                existing.canonical.display(),
                canonical.display()
            );
        }

        return;
    }

    assets.push(BinaryAsset { canonical, guest });
}

fn verify_readline_asset(path: &Path) -> Result<(), String> {
    let output = Command::new("readelf")
        .args(["--dyn-syms", "--wide"])
        .arg(path)
        .output()
        .map_err(|error| format!("cannot execute readelf for {}: {}", path.display(), error))?;

    if !output.status.success() {
        return Err(format!(
            "readelf failed for {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let symbols = String::from_utf8_lossy(&output.stdout);

    if symbols.lines().any(|line| {
        line.split_whitespace()
            .last()
            .map(|name| name == "rl_copy_text" || name.starts_with("rl_copy_text@"))
            .unwrap_or(false)
    }) {
        return Ok(());
    }

    Err(format!(
        "{} does not export rl_copy_text; refusing to embed an incompatible Readline",
        path.display()
    ))
}

const PT_LOAD: u32 = 1;
const PT_DYNAMIC: u32 = 2;
const PT_INTERP: u32 = 3;
const DT_NEEDED: u64 = 1;
const DT_STRTAB: u64 = 5;
const DT_STRSZ: u64 = 10;

#[derive(Clone)]
struct BinaryAsset {
    canonical: PathBuf,
    guest: String,
}

fn le16(data: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([data[at], data[at + 1]])
}

fn le32(data: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]])
}

fn le64(data: &[u8], at: usize) -> u64 {
    u64::from_le_bytes([
        data[at],
        data[at + 1],
        data[at + 2],
        data[at + 3],
        data[at + 4],
        data[at + 5],
        data[at + 6],
        data[at + 7],
    ])
}

fn phdr(bytes: &[u8], index: usize) -> Option<(u32, u32, u64, u64, u64, u64)> {
    let phoff = le64(bytes, 32) as usize;
    let phentsize = le16(bytes, 54) as usize;
    let base = phoff.checked_add(index.checked_mul(phentsize)?)?;

    if base.checked_add(56)? > bytes.len() {
        return None;
    }

    Some((
        le32(bytes, base),
        le32(bytes, base + 4),
        le64(bytes, base + 8),
        le64(bytes, base + 16),
        le64(bytes, base + 32),
        le64(bytes, base + 40),
    ))
}

fn va_to_file(bytes: &[u8], address: u64) -> Option<u64> {
    let phnum = le16(bytes, 56) as usize;

    for index in 0..phnum {
        let (kind, _flags, offset, vaddr, filesz, _memsz) = phdr(bytes, index)?;

        if kind != PT_LOAD {
            continue;
        }

        let end = vaddr.checked_add(filesz)?;

        if address >= vaddr && address < end {
            return offset.checked_add(address - vaddr);
        }
    }

    None
}

fn parse_dependencies(path: &Path) -> Result<(Option<String>, Vec<String>), String> {
    let bytes =
        fs::read(path).map_err(|error| format!("cannot read {}: {}", path.display(), error))?;

    if bytes.len() < 64 {
        return Err(format!("{}: ELF image is too small", path.display()));
    }

    if &bytes[..4] != b"\x7FELF" {
        return Err(format!("{}: not an ELF image", path.display()));
    }

    if bytes[4] != 2 {
        return Err(format!("{}: not ELF64", path.display()));
    }

    if bytes[5] != 1 {
        return Err(format!("{}: not little-endian ELF", path.display()));
    }

    if le16(&bytes, 18) != 0x003E {
        return Err(format!("{}: not x86_64", path.display()));
    }

    let phnum = le16(&bytes, 56) as usize;

    let mut interpreter = None;
    let mut dynamic_offset = None;
    let mut dynamic_size = 0u64;

    for index in 0..phnum {
        let (kind, _flags, offset, _vaddr, filesz, _memsz) = phdr(&bytes, index)
            .ok_or_else(|| format!("{}: malformed program header", path.display()))?;

        match kind {
            PT_INTERP => {
                let start = offset as usize;
                let end = start
                    .checked_add(filesz as usize)
                    .ok_or_else(|| format!("{}: PT_INTERP overflow", path.display()))?;

                if end > bytes.len() || start >= end {
                    return Err(format!("{}: malformed PT_INTERP", path.display()));
                }

                let data = &bytes[start..end];

                let mut length = 0usize;

                while length < data.len() {
                    if data[length] == 0 {
                        break;
                    }

                    length += 1;
                }

                interpreter = Some(String::from_utf8_lossy(&data[..length]).into_owned());
            }

            PT_DYNAMIC => {
                dynamic_offset = Some(offset);
                dynamic_size = filesz;
            }

            _ => {}
        }
    }

    let mut needed = Vec::new();

    let Some(dynamic_offset) = dynamic_offset else {
        return Ok((interpreter, needed));
    };

    if dynamic_size < 16 {
        return Ok((interpreter, needed));
    }

    let start = dynamic_offset as usize;
    let end = start
        .checked_add(dynamic_size as usize)
        .ok_or_else(|| format!("{}: PT_DYNAMIC overflow", path.display()))?;

    if end > bytes.len() {
        return Err(format!("{}: PT_DYNAMIC outside file", path.display()));
    }

    let dynamic = &bytes[start..end];

    let mut strtab = 0u64;
    let mut strsz = 0u64;
    let mut needed_offsets = Vec::new();

    let mut cursor = 0usize;

    while cursor + 16 <= dynamic.len() {
        let tag = le64(dynamic, cursor);
        let value = le64(dynamic, cursor + 8);

        cursor += 16;

        if tag == 0 {
            break;
        }

        match tag {
            DT_NEEDED => needed_offsets.push(value),
            DT_STRTAB => strtab = value,
            DT_STRSZ => strsz = value,
            _ => {}
        }
    }

    if strtab == 0 || strsz == 0 {
        return Ok((interpreter, needed));
    }

    let strtab_file = va_to_file(&bytes, strtab)
        .ok_or_else(|| format!("{}: DT_STRTAB cannot be translated", path.display()))?;

    let str_start = strtab_file as usize;
    let str_end = str_start
        .checked_add(strsz as usize)
        .ok_or_else(|| format!("{}: string-table overflow", path.display()))?;

    if str_end > bytes.len() {
        return Err(format!(
            "{}: dynamic string table outside file",
            path.display()
        ));
    }

    let strings = &bytes[str_start..str_end];

    for offset in needed_offsets {
        if offset >= strsz {
            return Err(format!(
                "{}: DT_NEEDED points outside string table",
                path.display()
            ));
        }

        let start = offset as usize;
        let mut end = start;

        while end < strings.len() && strings[end] != 0 {
            end += 1;
        }

        if end == strings.len() {
            return Err(format!("{}: unterminated DT_NEEDED string", path.display()));
        }

        needed.push(String::from_utf8_lossy(&strings[start..end]).into_owned());
    }

    Ok((interpreter, needed))
}

fn search_library(name: &str) -> Option<PathBuf> {
    let candidates = [
        PathBuf::from(format!("/lib64/{}", name)),
        PathBuf::from(format!("/usr/lib64/{}", name)),
        PathBuf::from(format!("/lib/{}", name)),
        PathBuf::from(format!("/usr/lib/{}", name)),
        PathBuf::from(format!("/lib/x86_64-linux-gnu/{}", name)),
        PathBuf::from(format!("/usr/lib/x86_64-linux-gnu/{}", name)),
    ];

    for candidate in candidates {
        if candidate.is_file() {
            return fs::canonicalize(candidate).ok();
        }
    }

    None
}

fn resolve_library(name: &str, parent: &Path) -> Option<PathBuf> {
    let candidate = Path::new(name);

    if candidate.is_absolute() && candidate.is_file() {
        return fs::canonicalize(candidate).ok();
    }

    let local = parent.join(name);

    if local.is_file() {
        return fs::canonicalize(local).ok();
    }

    search_library(name)
}

fn find_host_binary(name: &str) -> PathBuf {
    if let Some(path) = env::var_os("VIBUX_BASH_ELF_PATH") {
        let path = PathBuf::from(path);

        if name == "bash" && path.is_file() {
            return fs::canonicalize(path).expect("VELF build: cannot canonicalize Bash");
        }
    }

    let path = env::var_os("PATH").expect("VELF build: PATH is missing");

    for directory in env::split_paths(&path) {
        let candidate = directory.join(name);

        if candidate.is_file() {
            return fs::canonicalize(&candidate).unwrap_or(candidate);
        }
    }

    panic!("VELF build: cannot locate host {}", name);
}

fn main() {
    println!("cargo:rerun-if-env-changed=VIBUX_BASH_ELF_PATH");
    println!("cargo:rerun-if-env-changed=VIBUX_FONT_PATH");

    let manifest = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("VELF build: CARGO_MANIFEST_DIR missing"),
    );

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("VELF build: OUT_DIR missing"));

    let firmware_source = manifest.join("assets").join("rtl8188fufw.bin");
    if firmware_source.is_file() {
        fs::copy(&firmware_source, out_dir.join("rtl8188fufw.bin"))
            .expect("VELF build: failed to stage rtl8188fufw.bin");
        println!("cargo:rerun-if-changed={}", firmware_source.display());
        println!(
            "cargo:warning=Vibux RTL8188F firmware: {} bytes",
            fs::metadata(&firmware_source)
                .expect("firmware metadata")
                .len()
        );
    } else {
        panic!("VELF build: missing {}", firmware_source.display());
    }

    let font_source = if let Some(path) = env::var_os("VIBUX_FONT_PATH") {
        PathBuf::from(path)
    } else {
        manifest.join("assets").join("vibux-font.psf")
    };

    if !font_source.is_file() {
        panic!("VELF build: PSF font missing: {}", font_source.display());
    }

    fs::copy(&font_source, out_dir.join("vibux-font.psf"))
        .expect("VELF build: failed to stage PSF font");

    println!("cargo:rerun-if-changed={}", font_source.display());

    let bash = find_host_binary("bash");

    let sh = {
        let candidate = PathBuf::from("/bin/sh");

        if candidate.is_file() {
            fs::canonicalize(candidate).expect("VELF build: cannot canonicalize /bin/sh")
        } else {
            find_host_binary("sh")
        }
    };

    let mut assets: Vec<BinaryAsset> = Vec::new();
    let ls = find_host_binary("ls");
    let cat = find_host_binary("cat");
    let sed = find_host_binary("sed");

    let ls = fs::canonicalize(&ls)
        .unwrap_or_else(|error| panic!("VELF build: cannot canonicalize ls: {}", error));

    let cat = fs::canonicalize(&cat)
        .unwrap_or_else(|error| panic!("VELF build: cannot canonicalize cat: {}", error));

    let sed = fs::canonicalize(&sed)
        .unwrap_or_else(|error| panic!("VELF build: cannot canonicalize sed: {}", error));

    let bash = fs::canonicalize(&bash)
        .unwrap_or_else(|error| panic!("VELF build: cannot canonicalize Bash: {}", error));

    let sh = fs::canonicalize(&sh)
        .unwrap_or_else(|error| panic!("VELF build: cannot canonicalize /bin/sh: {}", error));

    add_asset(&mut assets, bash.clone(), "/bin/bash".to_string());

    add_asset(&mut assets, sh.clone(), "/bin/sh".to_string());
    add_asset(&mut assets, ls.clone(), "/bin/ls".to_string());
    add_asset(&mut assets, cat.clone(), "/bin/cat".to_string());
    add_asset(&mut assets, sed.clone(), "/bin/sed".to_string());

    for root in [&bash, &sh, &ls, &cat, &sed] {
        let resolved = trace_runtime_dependencies(root)
            .unwrap_or_else(|error| panic!("VELF build: {}", error));

        for (dependency_name, path) in resolved {
            let guest = format!("/lib64/{}", dependency_name);

            add_asset(&mut assets, path, guest);
        }
    }

    let readline = assets
        .iter()
        .find(|asset| asset.guest == "/lib64/libreadline.so.8")
        .map(|asset| asset.canonical.clone())
        .unwrap_or_else(|| {
            panic!(
                "VELF build: Bash runtime closure did not produce \
                 /lib64/libreadline.so.8"
            )
        });

    verify_readline_asset(&readline).unwrap_or_else(|error| panic!("VELF build: {}", error));

    assets.sort_by(|a, b| a.guest.cmp(&b.guest));

    let mut staged_assets = Vec::<BinaryAsset>::with_capacity(assets.len());
    let mut staged_by_original = Vec::<(PathBuf, PathBuf)>::new();

    for (index, asset) in assets.iter().enumerate() {
        let original = asset.canonical.clone();

        let staged = if let Some((_, staged)) = staged_by_original
            .iter()
            .find(|(source, _)| source == &original)
        {
            staged.clone()
        } else {
            let staged = stage_elf_for_embedding(&original, &out_dir, index)
                .unwrap_or_else(|error| panic!("VELF build: {}", error));

            staged_by_original.push((original.clone(), staged.clone()));

            staged
        };

        let before = fs::metadata(&original)
            .expect("VELF build: cannot stat original ELF")
            .len();

        let after = fs::metadata(&staged)
            .expect("VELF build: cannot stat staged ELF")
            .len();

        println!(
            "VELF asset: {} -> {} ({} -> {} bytes)",
            original.display(),
            asset.guest,
            before,
            after
        );

        println!("cargo:rerun-if-changed={}", original.display());

        staged_assets.push(BinaryAsset {
            canonical: staged,
            guest: asset.guest.clone(),
        });
    }

    let generated = out_dir.join("vibux_binaries.rs");

    let mut source = String::new();

    for (index, asset) in staged_assets.iter().enumerate() {
        source.push_str(&format!(
            "static VIBUX_ELF_{index}: &[u8] = include_bytes!({:?});\n",
            asset.canonical.display().to_string()
        ));
    }

    source.push('\n');
    source.push_str("pub static BINARIES: &[Binary] = &[\n");

    for (index, asset) in staged_assets.iter().enumerate() {
        let name = Path::new(&asset.guest)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");

        source.push_str("    Binary {\n");
        source.push_str(&format!("        path: b{:?},\n", asset.guest));
        source.push_str(&format!("        name: b{:?},\n", name));
        source.push_str(&format!("        bytes: VIBUX_ELF_{index},\n"));
        source.push_str("    },\n");

        let bytes = fs::metadata(&asset.canonical)
            .expect("VELF build: cannot stat ELF")
            .len();

        println!(
            "VELF asset: {} -> {} ({} bytes)",
            asset.canonical.display(),
            asset.guest,
            bytes
        );

        println!("cargo:rerun-if-changed={}", asset.canonical.display());
    }

    source.push_str("];\n");

    fs::write(&generated, source).expect("VELF build: cannot generate binary store");

    println!("VELF build: embedded {} ELF images", staged_assets.len());
}
