#[derive(Clone, Copy)]
pub struct Binary {
    pub path: &'static [u8],
    pub name: &'static [u8],
    pub bytes: &'static [u8],
}

include!(concat!(env!("OUT_DIR"), "/vibux_binaries.rs"));

#[inline(always)]
pub fn find(path: &[u8]) -> Option<&'static Binary> {
    for binary in BINARIES.iter() {
        if binary.path == path {
            return Some(binary);
        }
    }

    let mut last = 0usize;
    for (i, &b) in path.iter().enumerate() {
        if b == b'/' {
            last = i + 1;
        }
    }
    let name = &path[last..];

    for binary in BINARIES.iter() {
        if binary.name == name {
            return Some(binary);
        }
    }

    if name.windows(4).any(|w| w == b".so.") {
        for binary in BINARIES.iter() {
            if binary.name.len() > name.len()
                && binary.name.starts_with(name)
                && binary.name[name.len()] == b'.'
            {
                return Some(binary);
            }
        }
    }
    None
}

#[inline(always)]
pub fn bash() -> &'static [u8] {
    find(b"/bin/bash")
        .expect("VELF internal error: embedded /bin/bash missing")
        .bytes
}

#[inline(always)]
pub fn sh() -> &'static [u8] {
    find(b"/bin/sh")
        .expect("VELF internal error: embedded /bin/sh missing")
        .bytes
}
