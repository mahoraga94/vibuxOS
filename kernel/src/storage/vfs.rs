use super::checksum::crc32c;
use super::nvme::Nvme;
use crate::console::Serial;
use core::fmt::Write;

const BLOCK_SIZE: usize = 4096;
const SUPER_BLOCK: u32 = 0;
const INODE_START: u32 = 1;
const INODE_COUNT: u32 = 4096;
const INODE_SIZE: u32 = 128;
const INODE_BLOCKS: u32 = (INODE_COUNT * INODE_SIZE) / BLOCK_SIZE as u32;
const INODES_PER_BLOCK: usize = BLOCK_SIZE / INODE_SIZE as usize;
const BITMAP_START: u32 = INODE_START + INODE_BLOCKS;
const FS_START_BYTES: u64 = 32 * 1024 * 1024;
const FS_MAGIC: &[u8; 8] = b"VIBUXFS1";
const FS_VERSION: u32 = 1;
const MAX_BITMAP_BYTES: usize = 16384;
const MAX_FILE_BLOCKS: usize = 1024;
const ROOT_INODE: u32 = 1;

static mut RAW: Option<*mut Nvme> = None;
pub fn attach_raw(device: *mut Nvme) {
    unsafe {
        core::ptr::addr_of_mut!(RAW).write(Some(device));
    }
}
pub fn with_raw<R>(f: impl FnOnce(&mut Nvme) -> R) -> Option<R> {
    unsafe {
        let ptr = core::ptr::addr_of_mut!(RAW).read();
        ptr.map(|p| f(&mut *p))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FsError {
    Offline,
    Io,
    Invalid,
    NotFound,
    Exists,
    NotDir,
    IsDir,
    NotEmpty,
    Permission,
    NoSpace,
    TooLarge,
}

#[derive(Clone, Copy)]
struct FsMeta {
    total_blocks: u32,
    bitmap_blocks: u32,
}
impl FsMeta {
    fn data_start(&self) -> u32 {
        BITMAP_START + self.bitmap_blocks
    }
    fn data_blocks(&self) -> u32 {
        self.total_blocks.saturating_sub(self.data_start())
    }
}

#[derive(Clone, Copy)]
struct Inode {
    kind: u8,
    mode: u16,
    uid: u32,
    gid: u32,
    parent: u32,
    first_child: u32,
    next_sibling: u32,
    size: u64,
    start_block: u32,
    block_count: u32,
    generation: u64,
    name: [u8; 56],
}
impl Inode {
    fn name_slice(&self) -> &[u8] {
        let mut len = 0;
        while len < 56 && self.name[len] != 0 {
            len += 1;
        }
        &self.name[..len]
    }
    fn name_str(&self) -> &str {
        core::str::from_utf8(self.name_slice()).unwrap_or("?")
    }
}

struct State {
    nvme: Nvme,
    meta: FsMeta,
    next_inode_hint: u32,
}
struct Mount {
    name: [u8; 16],
    name_len: usize,
    state: State,
    read_only: bool,
}
static mut STATE: Option<State> = None;
static mut MOUNTS: [Option<Mount>; 8] = [None, None, None, None, None, None, None, None];

pub fn mounted() -> bool {
    unsafe { core::ptr::read_volatile(core::ptr::addr_of!(STATE)).is_some() }
}
pub fn root_inode() -> u32 {
    ROOT_INODE
}
fn block_bytes(fs_block: u32) -> u64 {
    FS_START_BYTES + fs_block as u64 * BLOCK_SIZE as u64
}

fn parse_super(block: &[u8; BLOCK_SIZE]) -> Option<FsMeta> {
    if &block[0..8] != FS_MAGIC {
        return None;
    }
    let total = u32::from_le_bytes([block[8], block[9], block[10], block[11]]);
    let bitmap = u32::from_le_bytes([block[12], block[13], block[14], block[15]]);
    Some(FsMeta {
        total_blocks: total,
        bitmap_blocks: bitmap,
    })
}

fn load_bitmap(state: &mut State, bitmap: &mut [u8; MAX_BITMAP_BYTES]) -> Result<(), FsError> {
    let blocks = state.meta.bitmap_blocks as usize;
    let mut offset = 0;
    let mut tmp = [0u8; BLOCK_SIZE];
    for i in 0..blocks {
        state
            .nvme
            .read_range(block_bytes(BITMAP_START + i as u32), &mut tmp)
            .map_err(|_| FsError::Io)?;
        let copy = core::cmp::min(BLOCK_SIZE, MAX_BITMAP_BYTES - offset);
        bitmap[offset..offset + copy].copy_from_slice(&tmp[..copy]);
        offset += copy;
    }
    Ok(())
}
fn store_bitmap(state: &mut State, bitmap: &[u8; MAX_BITMAP_BYTES]) -> Result<(), FsError> {
    let blocks = state.meta.bitmap_blocks as usize;
    let mut offset = 0;
    let mut tmp = [0u8; BLOCK_SIZE];
    for i in 0..blocks {
        let copy = core::cmp::min(BLOCK_SIZE, MAX_BITMAP_BYTES - offset);
        tmp[..copy].copy_from_slice(&bitmap[offset..offset + copy]);
        state
            .nvme
            .write_range(block_bytes(BITMAP_START + i as u32), &tmp)
            .map_err(|_| FsError::Io)?;
        offset += copy;
    }
    Ok(())
}
fn bit_is_set(bitmap: &[u8], block: u32) -> bool {
    let i = block as usize;
    bitmap[i / 8] & (1 << (i % 8)) != 0
}
fn set_bit(bitmap: &mut [u8], block: u32) {
    let i = block as usize;
    bitmap[i / 8] |= 1 << (i % 8);
}
fn clear_bit(bitmap: &mut [u8], block: u32) {
    let i = block as usize;
    bitmap[i / 8] &= !(1 << (i % 8));
}

fn read_inode(state: &mut State, inode_no: u32) -> Result<Inode, FsError> {
    if inode_no == 0 || inode_no > INODE_COUNT {
        return Err(FsError::Invalid);
    }
    let block_no = INODE_START + (inode_no - 1) / INODES_PER_BLOCK as u32;
    let offset = ((inode_no - 1) as usize % INODES_PER_BLOCK) * INODE_SIZE as usize;
    let mut block = [0u8; BLOCK_SIZE];
    state
        .nvme
        .read_range(block_bytes(block_no), &mut block)
        .map_err(|_| FsError::Io)?;
    let mut inode = Inode {
        kind: 0,
        mode: 0,
        uid: 0,
        gid: 0,
        parent: 0,
        first_child: 0,
        next_sibling: 0,
        size: 0,
        start_block: 0,
        block_count: 0,
        generation: 0,
        name: [0; 56],
    };
    unsafe {
        core::ptr::copy_nonoverlapping(
            block.as_ptr().add(offset),
            &mut inode as *mut _ as *mut u8,
            INODE_SIZE as usize,
        );
    }
    Ok(inode)
}
fn write_inode(state: &mut State, inode_no: u32, inode: &Inode) -> Result<(), FsError> {
    let block_no = INODE_START + (inode_no - 1) / INODES_PER_BLOCK as u32;
    let offset = ((inode_no - 1) as usize % INODES_PER_BLOCK) * INODE_SIZE as usize;
    let mut block = [0u8; BLOCK_SIZE];
    state
        .nvme
        .read_range(block_bytes(block_no), &mut block)
        .map_err(|_| FsError::Io)?;
    unsafe {
        core::ptr::copy_nonoverlapping(
            inode as *const _ as *const u8,
            block.as_mut_ptr().add(offset),
            INODE_SIZE as usize,
        );
    }
    state
        .nvme
        .write_range(block_bytes(block_no), &block)
        .map_err(|_| FsError::Io)
}
fn zero_inode(state: &mut State, inode_no: u32) -> Result<(), FsError> {
    let inode = Inode {
        kind: 0,
        mode: 0,
        uid: 0,
        gid: 0,
        parent: 0,
        first_child: 0,
        next_sibling: 0,
        size: 0,
        start_block: 0,
        block_count: 0,
        generation: 0,
        name: [0; 56],
    };
    write_inode(state, inode_no, &inode)
}

fn with_state<R, F: FnOnce(&mut State) -> Result<R, FsError>>(f: F) -> Result<R, FsError> {
    unsafe {
        match &mut *core::ptr::addr_of_mut!(STATE) {
            Some(s) => f(s),
            None => Err(FsError::Offline),
        }
    }
}
fn can_access(inode: &Inode, _uid: u32, _gid: u32, _r: bool, _w: bool, _x: bool) -> bool {
    true
}

fn resolve_with(state: &mut State, path: &[u8], cwd: u32) -> Result<u32, FsError> {
    if path.is_empty() || path == b"/" {
        return Ok(ROOT_INODE);
    }
    let mut cur = if path[0] == b'/' { ROOT_INODE } else { cwd };
    for part in path
        .split(|&b| b == b'/')
        .filter(|p| !p.is_empty() && *p != b".")
    {
        if *part == *b".." {
            let inode = read_inode(state, cur)?;
            if inode.parent != 0 {
                cur = inode.parent;
            }
            continue;
        }
        let dir = read_inode(state, cur)?;
        if dir.kind != 2 {
            return Err(FsError::NotDir);
        }
        let mut child = dir.first_child;
        let mut found = 0;
        while child != 0 {
            let e = read_inode(state, child)?;
            if e.kind != 0 && *e.name_slice() == *part {
                found = child;
                break;
            }
            child = e.next_sibling;
        }
        if found == 0 {
            return Err(FsError::NotFound);
        }
        cur = found;
    }
    Ok(cur)
}

pub fn resolve(path: &[u8], cwd: u32) -> Result<u32, FsError> {
    with_state(|s| resolve_with(s, path, cwd))
}
pub fn stat_kind(inode: u32) -> Result<u8, FsError> {
    with_state(|s| Ok(read_inode(s, inode)?.kind))
}

pub fn list<W: Write>(
    path: &[u8],
    cwd: u32,
    uid: u32,
    gid: u32,
    out: &mut W,
) -> Result<(), FsError> {
    // Handle /mnt listing
    if path == b"/mnt" || path == b"/mnt/" {
        writeln!(out, "[D] nvme1/").ok();
        list_mounts(out);
        return Ok(());
    }
    with_state(|state| {
        let inode = resolve_with(state, path, cwd)?;
        let dir = read_inode(state, inode)?;
        if dir.kind != 2 {
            return Err(FsError::NotDir);
        }
        let mut child = dir.first_child;
        if child == 0 {
            writeln!(out, "(empty)").ok();
            return Ok(());
        }
        while child != 0 {
            let e = read_inode(state, child)?;
            if e.kind != 0 {
                let suf = if e.kind == 2 { "/" } else { "" };
                let tag = if e.kind == 2 { "[D]" } else { "[F]" };
                writeln!(out, "{} {}{}", tag, e.name_str(), suf).ok();
            }
            child = e.next_sibling;
        }
        Ok(())
    })
}

pub fn read_file<W: Write>(
    path: &[u8],
    cwd: u32,
    uid: u32,
    gid: u32,
    out: &mut W,
) -> Result<(), FsError> {
    with_state(|state| {
        let ino = resolve_with(state, path, cwd)?;
        let inode = read_inode(state, ino)?;
        if inode.kind != 1 {
            return Err(FsError::IsDir);
        }
        let mut rem = inode.size;
        let mut idx = 0u32;
        let mut block = [0u8; BLOCK_SIZE];
        while rem > 0 {
            state
                .nvme
                .read_range(block_bytes(inode.start_block + idx), &mut block)
                .map_err(|_| FsError::Io)?;
            let cnt = core::cmp::min(rem as usize, BLOCK_SIZE);
            for &b in &block[..cnt] {
                out.write_char(b as char).ok();
            }
            rem -= cnt as u64;
            idx += 1;
        }
        Ok(())
    })
}
pub fn read_file_bytes(
    path: &[u8],
    cwd: u32,
    uid: u32,
    gid: u32,
    out: &mut [u8],
) -> Result<usize, FsError> {
    with_state(|state| {
        let ino = resolve_with(state, path, cwd)?;
        let inode = read_inode(state, ino)?;
        if inode.kind != 1 {
            return Err(FsError::IsDir);
        }
        let sz = inode.size as usize;
        if sz > out.len() {
            return Err(FsError::TooLarge);
        }
        if sz != 0 {
            state
                .nvme
                .read_range(block_bytes(inode.start_block), &mut out[..sz])
                .map_err(|_| FsError::Io)?;
        }
        Ok(sz)
    })
}
pub fn make_dir(path: &[u8], cwd: u32, uid: u32, gid: u32) -> Result<(), FsError> {
    with_state(|s| create_entry(s, path, cwd, 2, 0o755, uid, gid).map(|_| ()))
}
pub fn touch(path: &[u8], cwd: u32, uid: u32, gid: u32) -> Result<(), FsError> {
    with_state(|s| match resolve_with(s, path, cwd) {
        Ok(_) => Ok(()),
        Err(FsError::NotFound) => create_entry(s, path, cwd, 1, 0o644, uid, gid).map(|_| ()),
        Err(e) => Err(e),
    })
}
pub fn write_file(path: &[u8], cwd: u32, data: &[u8], uid: u32, gid: u32) -> Result<(), FsError> {
    with_state(|s| write_file_with(s, path, cwd, data, uid, gid))
}
pub fn remove(path: &[u8], cwd: u32, uid: u32, gid: u32) -> Result<(), FsError> {
    with_state(|state| {
        let ino = resolve_with(state, path, cwd)?;
        if ino == ROOT_INODE {
            return Err(FsError::Permission);
        }
        let inode = read_inode(state, ino)?;
        let parent = read_inode(state, inode.parent)?;
        if inode.kind == 2 && inode.first_child != 0 {
            return Err(FsError::NotEmpty);
        }
        unlink_child(state, inode.parent, ino)?;
        if inode.kind == 1 && inode.block_count != 0 {
            let mut bmp = [0u8; MAX_BITMAP_BYTES];
            load_bitmap(state, &mut bmp)?;
            for i in 0..inode.block_count {
                clear_bit(&mut bmp, inode.start_block + i);
            }
            store_bitmap(state, &bmp)?;
        }
        zero_inode(state, ino)
    })
}
pub fn stat<W: Write>(
    path: &[u8],
    cwd: u32,
    uid: u32,
    gid: u32,
    out: &mut W,
) -> Result<(), FsError> {
    with_state(|state| {
        let ino = resolve_with(state, path, cwd)?;
        let inode = read_inode(state, ino)?;
        writeln!(
            out,
            "type={} size={} uid={} gid={} mode={:04o}",
            if inode.kind == 2 { "dir" } else { "file" },
            inode.size,
            inode.uid,
            inode.gid,
            inode.mode
        )
        .ok();
        Ok(())
    })
}
pub fn df<W: Write>(out: &mut W) -> Result<(), FsError> {
    with_state(|state| {
        let mut bmp = [0u8; MAX_BITMAP_BYTES];
        load_bitmap(state, &mut bmp)?;
        let data_start = state.meta.data_start();
        let data_blocks = state.meta.data_blocks();
        let mut used = 0u32;
        for i in 0..data_blocks {
            if bit_is_set(&bmp, data_start + i) {
                used += 1;
            }
        }
        let total = data_blocks as u64 * BLOCK_SIZE as u64;
        let used_b = used as u64 * BLOCK_SIZE as u64;
        writeln!(
            out,
            "filesystem: VIBUXFS\ncapacity: {} MiB\nused: {} KiB\nfree: {} MiB",
            total / 1024 / 1024,
            used_b / 1024,
            (total - used_b) / 1024 / 1024
        )
        .ok();
        Ok(())
    })
}

fn create_entry(
    state: &mut State,
    path: &[u8],
    cwd: u32,
    kind: u8,
    mode: u16,
    uid: u32,
    gid: u32,
) -> Result<u32, FsError> {
    let (parent_path, name) = split_parent(path);
    let parent_ino = if parent_path.is_empty() {
        cwd
    } else {
        resolve_with(state, parent_path, cwd)?
    };
    let parent = read_inode(state, parent_ino)?;
    if parent.kind != 2 {
        return Err(FsError::NotDir);
    }
    // check exists
    let mut child = parent.first_child;
    while child != 0 {
        let e = read_inode(state, child)?;
        if e.name_slice() == name {
            return Err(FsError::Exists);
        }
        child = e.next_sibling;
    }
    // alloc inode
    let mut ino = 0;
    for i in 1..=INODE_COUNT {
        if read_inode(state, i)?.kind == 0 {
            ino = i;
            break;
        }
    }
    if ino == 0 {
        return Err(FsError::NoSpace);
    }
    let mut new_inode = Inode {
        kind,
        mode,
        uid,
        gid,
        parent: parent_ino,
        first_child: 0,
        next_sibling: parent.first_child,
        size: 0,
        start_block: 0,
        block_count: 0,
        generation: 0,
        name: [0; 56],
    };
    let len = name.len().min(56);
    new_inode.name[..len].copy_from_slice(&name[..len]);
    write_inode(state, ino, &new_inode)?;
    let mut parent_mut = parent;
    parent_mut.first_child = ino;
    write_inode(state, parent_ino, &parent_mut)?;
    Ok(ino)
}
fn split_parent<'a>(path: &'a [u8]) -> (&'a [u8], &'a [u8]) {
    if let Some(pos) = path.iter().rposition(|&b| b == b'/') {
        (&path[..pos], &path[pos + 1..])
    } else {
        (b"", path)
    }
}
fn unlink_child(state: &mut State, parent_ino: u32, child_ino: u32) -> Result<(), FsError> {
    let mut parent = read_inode(state, parent_ino)?;
    let mut cur = parent.first_child;
    let mut prev = 0u32;
    while cur != 0 {
        if cur == child_ino {
            let child = read_inode(state, cur)?;
            if prev == 0 {
                parent.first_child = child.next_sibling;
            } else {
                let mut p = read_inode(state, prev)?;
                p.next_sibling = child.next_sibling;
                write_inode(state, prev, &p)?;
            }
            write_inode(state, parent_ino, &parent)?;
            return Ok(());
        }
        let e = read_inode(state, cur)?;
        prev = cur;
        cur = e.next_sibling;
    }
    Err(FsError::NotFound)
}
fn write_file_with(
    state: &mut State,
    path: &[u8],
    cwd: u32,
    data: &[u8],
    uid: u32,
    gid: u32,
) -> Result<(), FsError> {
    let ino = match resolve_with(state, path, cwd) {
        Ok(i) => i,
        Err(FsError::NotFound) => create_entry(state, path, cwd, 1, 0o644, uid, gid)?,
        Err(e) => return Err(e),
    };
    let mut inode = read_inode(state, ino)?;
    if inode.kind != 1 {
        return Err(FsError::IsDir);
    }
    let needed = ((data.len() + BLOCK_SIZE - 1) / BLOCK_SIZE) as u32;
    if needed as usize > MAX_FILE_BLOCKS {
        return Err(FsError::TooLarge);
    }
    // free old
    if inode.block_count != 0 {
        let mut bmp = [0u8; MAX_BITMAP_BYTES];
        load_bitmap(state, &mut bmp)?;
        for i in 0..inode.block_count {
            clear_bit(&mut bmp, inode.start_block + i);
        }
        store_bitmap(state, &bmp)?;
    }
    if needed > 0 {
        let mut bmp = [0u8; MAX_BITMAP_BYTES];
        load_bitmap(state, &mut bmp)?;
        let mut start = 0u32;
        let mut found = 0u32;
        for b in state.meta.data_start()..state.meta.data_start() + state.meta.data_blocks() {
            if !bit_is_set(&bmp, b) {
                if found == 0 {
                    start = b;
                }
                found += 1;
                if found == needed {
                    break;
                }
            } else {
                found = 0;
            }
        }
        if found != needed {
            return Err(FsError::NoSpace);
        }
        for i in 0..needed {
            set_bit(&mut bmp, start + i);
        }
        store_bitmap(state, &bmp)?;
        // write data
        let mut off = 0;
        for i in 0..needed {
            let mut blk = [0u8; BLOCK_SIZE];
            let copy = core::cmp::min(BLOCK_SIZE, data.len() - off);
            blk[..copy].copy_from_slice(&data[off..off + copy]);
            state
                .nvme
                .write_range(block_bytes(start + i), &blk)
                .map_err(|_| FsError::Io)?;
            off += copy;
        }
        inode.start_block = start;
        inode.block_count = needed;
    } else {
        inode.start_block = 0;
        inode.block_count = 0;
    }
    inode.size = data.len() as u64;
    write_inode(state, ino, &inode)
}

fn format(nvme: &mut Nvme, candidate: &FsMeta, _out: &mut Serial) -> Result<FsMeta, FsError> {
    let mut block = [0u8; BLOCK_SIZE];
    block[0..8].copy_from_slice(FS_MAGIC);
    block[8..12].copy_from_slice(&candidate.total_blocks.to_le_bytes());
    block[12..16].copy_from_slice(&candidate.bitmap_blocks.to_le_bytes());
    nvme.write_range(block_bytes(SUPER_BLOCK), &block)
        .map_err(|_| FsError::Io)?;
    // zero bitmap
    let mut bmp = [0u8; MAX_BITMAP_BYTES];
    // reserve super + inodes + bitmap blocks
    for i in 0..(BITMAP_START + candidate.bitmap_blocks) {
        if i < MAX_BITMAP_BYTES as u32 * 8 {
            set_bit(&mut bmp, i);
        }
    }
    let mut tmp = [0u8; BLOCK_SIZE];
    let mut off = 0;
    for i in 0..candidate.bitmap_blocks as usize {
        let copy = core::cmp::min(BLOCK_SIZE, MAX_BITMAP_BYTES - off);
        tmp[..copy].copy_from_slice(&bmp[off..off + copy]);
        nvme.write_range(block_bytes(BITMAP_START + i as u32), &tmp)
            .map_err(|_| FsError::Io)?;
        off += copy;
    }
    // root inode
    let mut root = Inode {
        kind: 2,
        mode: 0o755,
        uid: 0,
        gid: 0,
        parent: 0,
        first_child: 0,
        next_sibling: 0,
        size: 0,
        start_block: 0,
        block_count: 0,
        generation: 0,
        name: [0; 56],
    };
    root.name[0] = b'/';
    let mut blk = [0u8; BLOCK_SIZE];
    unsafe {
        core::ptr::copy_nonoverlapping(
            &root as *const _ as *const u8,
            blk.as_mut_ptr(),
            INODE_SIZE as usize,
        );
    }
    nvme.write_range(block_bytes(INODE_START), &blk)
        .map_err(|_| FsError::Io)?;
    Ok(*candidate)
}

pub fn attach(mut nvme: Nvme, namespace_bytes: u64, lba_bytes: u64, out: &mut Serial) {
    if lba_bytes == 0 || BLOCK_SIZE as u64 % lba_bytes != 0 {
        writeln!(out, "VIBUXFS : unsupported block size").ok();
        return;
    }
    let total_blocks = namespace_bytes.saturating_sub(FS_START_BYTES) / BLOCK_SIZE as u64;
    if total_blocks < 1024 {
        writeln!(out, "VIBUXFS : disk too small").ok();
        return;
    }
    let bitmap_bytes = ((total_blocks + 7) / 8) as usize;
    let bitmap_blocks = ((bitmap_bytes + BLOCK_SIZE - 1) / BLOCK_SIZE) as u32;
    if bitmap_blocks as usize * BLOCK_SIZE > MAX_BITMAP_BYTES {
        writeln!(out, "VIBUXFS : too big").ok();
        return;
    }
    let data_start = BITMAP_START + bitmap_blocks;
    if data_start >= total_blocks as u32 {
        writeln!(out, "VIBUXFS : layout error").ok();
        return;
    }
    let candidate = FsMeta {
        total_blocks: total_blocks as u32,
        bitmap_blocks,
    };
    let mut block = [0u8; BLOCK_SIZE];
    if nvme
        .read_range(block_bytes(SUPER_BLOCK), &mut block)
        .is_err()
    {
        writeln!(out, "VIBUXFS : superblock read failed").ok();
        return;
    }
    let existing = parse_super(&block).filter(|m| {
        m.total_blocks == candidate.total_blocks && m.bitmap_blocks == candidate.bitmap_blocks
    });
    let meta = match existing {
        Some(v) => v,
        None => match format(&mut nvme, &candidate, out) {
            Ok(v) => v,
            Err(_) => {
                writeln!(out, "VIBUXFS : format failed").ok();
                return;
            }
        },
    };
    unsafe {
        core::ptr::addr_of_mut!(STATE).write(Some(State {
            nvme,
            meta,
            next_inode_hint: ROOT_INODE + 1,
        }));
    }
    if existing.is_some() {
        writeln!(out, "VIBUXFS : mounted").ok();
    } else {
        writeln!(out, "VIBUXFS : formatted and mounted").ok();
    }
}

pub fn mount_readonly(
    mut nvme: Nvme,
    namespace_bytes: u64,
    lba_bytes: u64,
    name: &[u8],
    out: &mut Serial,
) -> Result<(), FsError> {
    if lba_bytes == 0 {
        return Err(FsError::Invalid);
    }
    let total_blocks = namespace_bytes.saturating_sub(FS_START_BYTES) / BLOCK_SIZE as u64;
    if total_blocks < 10 {
        return Err(FsError::Invalid);
    }
    let mut block = [0u8; BLOCK_SIZE];
    if nvme
        .read_range(block_bytes(SUPER_BLOCK), &mut block)
        .is_err()
    {
        return Err(FsError::Io);
    }
    let meta = parse_super(&block).ok_or(FsError::Invalid)?;
    let state = State {
        nvme,
        meta,
        next_inode_hint: ROOT_INODE + 1,
    };
    unsafe {
        for slot in &mut *core::ptr::addr_of_mut!(MOUNTS) {
            if slot.is_none() {
                let mut buf = [0u8; 16];
                let len = name.len().min(16);
                buf[..len].copy_from_slice(&name[..len]);
                *slot = Some(Mount {
                    name: buf,
                    name_len: len,
                    state,
                    read_only: true,
                });
                writeln!(
                    out,
                    "MOUNT /mnt/{} RO {} MiB",
                    core::str::from_utf8(&buf[..len]).unwrap_or("?"),
                    namespace_bytes / 1024 / 1024
                )
                .ok();
                return Ok(());
            }
        }
    }
    Err(FsError::NoSpace)
}

pub fn list_mounts<W: Write>(out: &mut W) {
    unsafe {
        for m in &*core::ptr::addr_of!(MOUNTS) {
            if let Some(mount) = m {
                // FIXED: use public API, not private field
                let bytes = mount.state.nvme.capacity_bytes() * mount.state.nvme.lba_size();
                writeln!(
                    out,
                    "/mnt/{} : {} MiB RO",
                    core::str::from_utf8(&mount.name[..mount.name_len]).unwrap_or("?"),
                    bytes / 1024 / 1024
                )
                .ok();
            }
        }
    }
}

fn seed_simulated_userspace() -> Result<(), FsError> {
    Ok(())
}
