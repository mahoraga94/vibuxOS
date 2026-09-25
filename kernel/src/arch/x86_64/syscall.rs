use core::arch::asm;
use core::ptr::write_bytes;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[inline(always)]
fn neg_errno(code: u64) -> u64 {
    0u64.wrapping_sub(code)
}
unsafe extern "C" {
    fn vibux_syscall_entry();
    static mut vibux_syscall_saved_rsp: u64;
    static mut vibux_child_syscall_saved_rsp: u64;
}

use crate::arch::x86_64::paging::{PageFlags, PageTableManager};
use crate::velf::runner::{HEAP_BASE, MMAP_BASE};

const IA32_EFER: u32 = 0xC000_0080;
const IA32_STAR: u32 = 0xC000_0081;
const IA32_LSTAR: u32 = 0xC000_0082;
const IA32_FMASK: u32 = 0xC000_0084;
const IA32_FS_BASE: u32 = 0xC000_0100;
const EFER_SCE: u64 = 1;
const USER_CS: u64 = 0x23;
const USER_SS: u64 = 0x1B;
const RFLAGS_USER: u64 = 0x202;

const EPERM: u64 = 1;
const ENOENT: u64 = 2;
const ESRCH: u64 = 3;
const EINTR: u64 = 4;
const EIO: u64 = 5;
const ENXIO: u64 = 6;
const E2BIG: u64 = 7;
const ENOEXEC: u64 = 8;
const EBADF: u64 = 9;
const ECHILD: u64 = 10;
const EAGAIN: u64 = 11;
const ENOMEM: u64 = 12;
const EACCES: u64 = 13;
const EFAULT: u64 = 14;
const ENOTBLK: u64 = 15;
const EBUSY: u64 = 16;
const EEXIST: u64 = 17;
const EXDEV: u64 = 18;
const ENODEV: u64 = 19;
const ENOTDIR: u64 = 20;
const EISDIR: u64 = 21;
const EINVAL: u64 = 22;
const ENFILE: u64 = 23;
const EMFILE: u64 = 24;
const ENOTTY: u64 = 25;
const ETXTBSY: u64 = 26;
const EFBIG: u64 = 27;
const ENOSPC: u64 = 28;
const ESPIPE: u64 = 29;
const EROFS: u64 = 30;
const EMLINK: u64 = 31;
const EPIPE: u64 = 32;
const EDOM: u64 = 33;
const ERANGE: u64 = 34;
const EDEADLK: u64 = 35;
const ENAMETOOLONG: u64 = 36;
const ENOLCK: u64 = 37;
const ENOSYS: u64 = 38;
const ENOTEMPTY: u64 = 39;
const ELOOP: u64 = 40;
const EOVERFLOW: u64 = 75;
const ENOTSOCK: u64 = 88;
const EDESTADDRREQ: u64 = 89;
const EMSGSIZE: u64 = 90;
const EPROTOTYPE: u64 = 91;
const ENOPROTOOPT: u64 = 92;
const EPROTONOSUPPORT: u64 = 93;
const ESOCKTNOSUPPORT: u64 = 94;
const EOPNOTSUPP: u64 = 95;
const EPFNOSUPPORT: u64 = 96;
const EAFNOSUPPORT: u64 = 97;
const EADDRINUSE: u64 = 98;
const EADDRNOTAVAIL: u64 = 99;
const ENETDOWN: u64 = 100;
const ENETUNREACH: u64 = 101;
const ENETRESET: u64 = 102;
const ECONNABORTED: u64 = 103;
const ECONNRESET: u64 = 104;
const ENOBUFS: u64 = 105;
const EISCONN: u64 = 106;
const ENOTCONN: u64 = 107;
const ESHUTDOWN: u64 = 108;
const ETIMEDOUT: u64 = 110;
const ECONNREFUSED: u64 = 111;
const EHOSTUNREACH: u64 = 113;
const EALREADY: u64 = 114;
const EINPROGRESS: u64 = 115;
const ESTALE: u64 = 116;
const EDQUOT: u64 = 122;
const ECANCELED: u64 = 125;
const ENODATA: u64 = 61;
const ENOKEY: u64 = 126;
const EKEYEXPIRED: u64 = 127;
const EKEYREVOKED: u64 = 128;
const EKEYREJECTED: u64 = 129;
const EOWNERDEAD: u64 = 130;
const ENOTRECOVERABLE: u64 = 131;

const SIG_DFL: u64 = 0;
const SIG_IGN: u64 = 1;
const SIGINT: u64 = 2;
const SIGQUIT: u64 = 3;
const SIGABRT: u64 = 6;
const SIGKILL: u64 = 9;
const SIGSEGV: u64 = 11;
const SIGTERM: u64 = 15;
const FILE_KIND_BLOCK: u8 = 18;
const SIGCHLD: u64 = 17;
const SIGCONT: u64 = 18;
const SIGSTOP: u64 = 19;
const SIGTSTP: u64 = 20;
const SIGURG: u64 = 23;
const SIGWINCH: u64 = 28;
const SA_NODEFER: u64 = 0x4000_0000;
const SA_RESETHAND: u64 = 0x8000_0000;
const SA_RESTORER: u64 = 0x0400_0000;
const SS_DISABLE: u64 = 2;
const MINSIGSTKSZ: u64 = 2048;
const RLIM_INFINITY: u64 = u64::MAX;

const O_RDONLY: u64 = 0;
const O_WRONLY: u64 = 1;
const O_RDWR: u64 = 2;
const O_CREAT: u64 = 0x40;
const O_EXCL: u64 = 0x80;
const O_NOCTTY: u64 = 0x100;
const O_TRUNC: u64 = 0x200;
const O_APPEND: u64 = 0x400;
const O_NONBLOCK: u64 = 0x800;
const O_DIRECT: u64 = 0x4000;
const O_DIRECTORY: u64 = 0x10000;
const O_NOFOLLOW: u64 = 0x20000;
const O_CLOEXEC: u64 = 0x80000;
const O_TMPFILE: u64 = 0x410000;
const O_PATH: u64 = 0x200000;
const O_ACCMODE: u64 = 3;

const AT_FDCWD: i64 = -100;
const AT_SYMLINK_NOFOLLOW: u64 = 0x100;
const AT_REMOVEDIR: u64 = 0x200;
const AT_SYMLINK_FOLLOW: u64 = 0x400;
const AT_NO_AUTOMOUNT: u64 = 0x800;
const AT_EMPTY_PATH: u64 = 0x1000;
const AT_EACCESS: u64 = 0x200;
const RENAME_NOREPLACE: u64 = 1;
const RENAME_EXCHANGE: u64 = 2;
const RENAME_WHITEOUT: u64 = 4;

const F_DUPFD: u64 = 0;
const F_GETFD: u64 = 1;
const F_SETFD: u64 = 2;
const F_GETFL: u64 = 3;
const F_SETFL: u64 = 4;
const F_GETLK: u64 = 5;
const F_SETLK: u64 = 6;
const F_SETLKW: u64 = 7;
const F_SETOWN: u64 = 8;
const F_GETOWN: u64 = 9;
const F_SETSIG: u64 = 10;
const F_GETSIG: u64 = 11;
const F_DUPFD_CLOEXEC: u64 = 1030;

const POLLIN: u16 = 0x0001;
const POLLPRI: u16 = 0x0002;
const POLLOUT: u16 = 0x0004;
const POLLERR: u16 = 0x0008;
const POLLHUP: u16 = 0x0010;
const POLLNVAL: u16 = 0x0020;
const POLLRDNORM: u16 = 0x0040;
const POLLRDBAND: u16 = 0x0080;
const POLLWRNORM: u16 = 0x0100;
const POLLWRBAND: u16 = 0x0200;

const FUTEX_WAIT: u64 = 0;
const FUTEX_WAKE: u64 = 1;
const FUTEX_PRIVATE_FLAG: u64 = 128;
const FUTEX_BITSET_MATCH_ANY: u64 = 0xFFFF_FFFF;

const ARCH_SET_GS: u64 = 0x1001;
const ARCH_SET_FS: u64 = 0x1002;
const ARCH_GET_FS: u64 = 0x1003;
const ARCH_GET_GS: u64 = 0x1004;

const PR_SET_PDEATHSIG: u64 = 1;
const PR_GET_PDEATHSIG: u64 = 2;
const PR_GET_DUMPABLE: u64 = 3;
const PR_SET_DUMPABLE: u64 = 4;
const PR_SET_NAME: u64 = 15;
const PR_GET_NAME: u64 = 16;
const PR_SET_NO_NEW_PRIVS: u64 = 38;
const PR_GET_NO_NEW_PRIVS: u64 = 39;
const PR_GET_TID_ADDRESS: u64 = 40;
const PR_GET_TIMERSLACK: u64 = 30;
const PR_SET_TIMERSLACK: u64 = 29;
const PR_GET_CHILD_SUBREAPER: u64 = 37;
const PR_SET_CHILD_SUBREAPER: u64 = 36;
const PR_GET_THP_DISABLE: u64 = 42;
const PR_SET_THP_DISABLE: u64 = 41;
const PR_GET_TAGGED_ADDR_CTRL: u64 = 56;
const PR_SET_TAGGED_ADDR_CTRL: u64 = 55;
const PR_GET_SPECULATION_CTRL: u64 = 52;
const PR_SET_SPECULATION_CTRL: u64 = 53;
const PR_GET_TIMING: u64 = 13;
const PR_SET_TIMING: u64 = 14;
const PR_GET_SECCOMP: u64 = 21;
const PR_SET_SECCOMP: u64 = 22;
const PR_GET_KEEPCAPS: u64 = 7;
const PR_SET_KEEPCAPS: u64 = 8;
const PR_CAP_AMBIENT: u64 = 47;

const RLIMIT_CPU: u64 = 0;
const RLIMIT_FSIZE: u64 = 1;
const RLIMIT_DATA: u64 = 2;
const RLIMIT_STACK: u64 = 3;
const RLIMIT_CORE: u64 = 4;
const RLIMIT_RSS: u64 = 5;
const RLIMIT_NPROC: u64 = 6;
const RLIMIT_NOFILE: u64 = 7;
const RLIMIT_MEMLOCK: u64 = 8;
const RLIMIT_AS: u64 = 9;
const RLIMIT_LOCKS: u64 = 10;
const RLIMIT_SIGPENDING: u64 = 11;
const RLIMIT_MSGQUEUE: u64 = 12;
const RLIMIT_NICE: u64 = 13;
const RLIMIT_RTPRIO: u64 = 14;
const RLIMIT_RTTIME: u64 = 15;

const CLOCK_REALTIME: u64 = 0;
const CLOCK_MONOTONIC: u64 = 1;
const CLOCK_PROCESS_CPUTIME_ID: u64 = 2;
const CLOCK_THREAD_CPUTIME_ID: u64 = 3;
const CLOCK_MONOTONIC_RAW: u64 = 4;
const CLOCK_REALTIME_COARSE: u64 = 5;
const CLOCK_MONOTONIC_COARSE: u64 = 6;
const CLOCK_BOOTTIME: u64 = 7;
const CLOCK_REALTIME_ALARM: u64 = 8;
const CLOCK_BOOTTIME_ALARM: u64 = 9;
const CLOCK_TAI: u64 = 11;
const TIMER_ABSTIME: u64 = 1;

const MREMAP_MAYMOVE: u64 = 1;
const MREMAP_FIXED: u64 = 2;
const MREMAP_DONTUNMAP: u64 = 4;

const GRND_NONBLOCK: u64 = 1;
const GRND_RANDOM: u64 = 2;
const GRND_INSECURE: u64 = 4;

const STATX_TYPE: u32 = 0x0001;
const STATX_MODE: u32 = 0x0002;
const STATX_NLINK: u32 = 0x0004;
const STATX_UID: u32 = 0x0008;
const STATX_GID: u32 = 0x0010;
const STATX_ATIME: u32 = 0x0020;
const STATX_MTIME: u32 = 0x0040;
const STATX_CTIME: u32 = 0x0080;
const STATX_INO: u32 = 0x0100;
const STATX_SIZE: u32 = 0x0200;
const STATX_BLOCKS: u32 = 0x0400;
const STATX_BASIC_STATS: u32 = 0x07FF;
const STATX_BTIME: u32 = 0x0800;
const STATX_ALL: u32 = 0x0FFF;

const EPOLL_CLOEXEC: u64 = O_CLOEXEC;
const EPOLL_CTL_ADD: u64 = 1;
const EPOLL_CTL_DEL: u64 = 2;
const EPOLL_CTL_MOD: u64 = 3;
const EFD_SEMAPHORE: u64 = 1;
const EFD_CLOEXEC: u64 = O_CLOEXEC;
const EFD_NONBLOCK: u64 = O_NONBLOCK;
const TFD_CLOEXEC: u64 = O_CLOEXEC;
const TFD_NONBLOCK: u64 = O_NONBLOCK;
const TFD_TIMER_ABSTIME: u64 = 1;
const TFD_TIMER_CANCEL_ON_SET: u64 = 2;
const SFD_CLOEXEC: u64 = O_CLOEXEC;
const SFD_NONBLOCK: u64 = O_NONBLOCK;
const SPLICE_F_MOVE: u64 = 1;
const SPLICE_F_NONBLOCK: u64 = 2;
const SPLICE_F_MORE: u64 = 4;
const SPLICE_F_GIFT: u64 = 8;
const FALLOC_FL_KEEP_SIZE: u64 = 0x01;
const FALLOC_FL_PUNCH_HOLE: u64 = 0x02;
const FALLOC_FL_NO_HIDE_STALE: u64 = 0x04;
const FALLOC_FL_COLLAPSE_RANGE: u64 = 0x08;
const FALLOC_FL_ZERO_RANGE: u64 = 0x10;
const FALLOC_FL_INSERT_RANGE: u64 = 0x20;
const FALLOC_FL_UNSHARE_RANGE: u64 = 0x40;

const MADV_NORMAL: u64 = 0;
const MADV_RANDOM: u64 = 1;
const MADV_SEQUENTIAL: u64 = 2;
const MADV_WILLNEED: u64 = 3;
const MADV_DONTNEED: u64 = 4;
const MADV_FREE: u64 = 8;
const MADV_REMOVE: u64 = 9;
const MADV_DONTFORK: u64 = 10;
const MADV_DOFORK: u64 = 11;
const MADV_HWPOISON: u64 = 100;
const MADV_MERGEABLE: u64 = 12;
const MADV_UNMERGEABLE: u64 = 13;
const MADV_HUGEPAGE: u64 = 14;
const MADV_NOHUGEPAGE: u64 = 15;
const MADV_DONTDUMP: u64 = 16;
const MADV_DODUMP: u64 = 17;
const MADV_WIPEONFORK: u64 = 18;
const MADV_KEEPONFORK: u64 = 19;
const MADV_COLD: u64 = 20;
const MADV_PAGEOUT: u64 = 21;
const MADV_POPULATE_READ: u64 = 22;
const MADV_POPULATE_WRITE: u64 = 23;
const MADV_DONTNEED_LOCKED: u64 = 24;

const POSIX_FADV_NORMAL: u64 = 0;
const POSIX_FADV_RANDOM: u64 = 1;
const POSIX_FADV_SEQUENTIAL: u64 = 2;
const POSIX_FADV_WILLNEED: u64 = 3;
const POSIX_FADV_DONTNEED: u64 = 4;
const POSIX_FADV_NOREUSE: u64 = 5;

const PROT_READ: u64 = 1;
const PROT_WRITE: u64 = 2;
const PROT_EXEC: u64 = 4;
const PROT_SEM: u64 = 8;
const PROT_NONE: u64 = 0;
const PROT_GROWSDOWN: u64 = 0x0100_0000;
const PROT_GROWSUP: u64 = 0x0200_0000;

const MAP_SHARED: u64 = 0x01;
const MAP_PRIVATE: u64 = 0x02;
const MAP_SHARED_VALIDATE: u64 = 0x03;
const MAP_TYPE: u64 = 0x0F;
const MAP_FIXED: u64 = 0x10;
const MAP_ANONYMOUS: u64 = 0x20;
const MAP_GROWSDOWN: u64 = 0x0100;
const MAP_DENYWRITE: u64 = 0x0800;
const MAP_EXECUTABLE: u64 = 0x1000;
const MAP_LOCKED: u64 = 0x2000;
const MAP_NORESERVE: u64 = 0x4000;
const MAP_POPULATE: u64 = 0x8000;
const MAP_NONBLOCK: u64 = 0x10000;
const MAP_STACK: u64 = 0x20000;
const MAP_HUGETLB: u64 = 0x40000;
const MAP_SYNC: u64 = 0x80000;
const MAP_FIXED_NOREPLACE: u64 = 0x100000;
const MAP_FILE: u64 = 0;

const MS_ASYNC: u64 = 1;
const MS_INVALIDATE: u64 = 2;
const MS_SYNC: u64 = 4;
const MS_LAZYTIME: u64 = 0x0200000;
const MCL_CURRENT: u64 = 1;
const MCL_FUTURE: u64 = 2;
const MCL_ONFAULT: u64 = 4;

const AF_UNSPEC: u64 = 0;
const AF_UNIX: u64 = 1;
const AF_LOCAL: u64 = 1;
const AF_INET: u64 = 2;
const AF_INET6: u64 = 10;
const AF_NETLINK: u64 = 16;
const AF_PACKET: u64 = 17;
const SOCK_STREAM: u64 = 1;
const SOCK_DGRAM: u64 = 2;
const SOCK_RAW: u64 = 3;
const SOCK_RDM: u64 = 4;
const SOCK_SEQPACKET: u64 = 5;
const SOCK_DCCP: u64 = 6;
const SOCK_PACKET: u64 = 10;
const SOCK_CLOEXEC: u64 = O_CLOEXEC;
const SOCK_NONBLOCK: u64 = O_NONBLOCK;
const SOL_SOCKET: u64 = 1;
const SO_REUSEADDR: u64 = 2;
const SO_TYPE: u64 = 3;
const SO_ERROR: u64 = 4;
const SO_DONTROUTE: u64 = 5;
const SO_BROADCAST: u64 = 6;
const SO_SNDBUF: u64 = 7;
const SO_RCVBUF: u64 = 8;
const SO_KEEPALIVE: u64 = 9;
const SO_OOBINLINE: u64 = 10;
const SO_LINGER: u64 = 13;
const SO_REUSEPORT: u64 = 15;
const SO_RCVLOWAT: u64 = 18;
const SO_SNDLOWAT: u64 = 19;
const SO_RCVTIMEO: u64 = 20;
const SO_SNDTIMEO: u64 = 21;
const SO_ACCEPTCONN: u64 = 30;
const SO_PROTOCOL: u64 = 38;
const SO_DOMAIN: u64 = 39;
const SCM_RIGHTS: u64 = 1;
const SCM_CREDENTIALS: u64 = 2;
const MSG_OOB: u64 = 1;
const MSG_PEEK: u64 = 2;
const MSG_DONTROUTE: u64 = 4;
const MSG_CTRUNC: u64 = 8;
const MSG_PROXY: u64 = 0x10;
const MSG_TRUNC: u64 = 0x20;
const MSG_DONTWAIT: u64 = 0x40;
const MSG_EOR: u64 = 0x80;
const MSG_WAITALL: u64 = 0x100;
const MSG_FIN: u64 = 0x200;
const MSG_SYN: u64 = 0x400;
const MSG_CONFIRM: u64 = 0x800;
const MSG_RST: u64 = 0x1000;
const MSG_ERRQUEUE: u64 = 0x2000;
const MSG_NOSIGNAL: u64 = 0x4000;
const MSG_MORE: u64 = 0x8000;
const MSG_WAITFORONE: u64 = 0x10000;
const MSG_BATCH: u64 = 0x40000;
const MSG_ZEROCOPY: u64 = 0x4000000;
const MSG_FASTOPEN: u64 = 0x20000000;
const MSG_CMSG_CLOEXEC: u64 = 0x40000000;

const USER_LIMIT: u64 = 0x0000_8000_0000_0000;
const PAGE_SIZE: u64 = 4096;

const TCGETS: u64 = 0x5401;
const TCSETS: u64 = 0x5402;
const TCSETSW: u64 = 0x5403;
const TCSETSF: u64 = 0x5404;
const TCGETS2: u64 = 0x802C_542A;
const TCSETS2: u64 = 0x402C_542B;
const TCSETSW2: u64 = 0x402C_542C;
const TCSETSF2: u64 = 0x402C_542D;
const TIOCGPGRP: u64 = 0x540F;
const TIOCSPGRP: u64 = 0x5410;
const TIOCOUTQ: u64 = 0x5411;
const TIOCSTI: u64 = 0x5412;
const TIOCGWINSZ: u64 = 0x5413;
const TIOCSWINSZ: u64 = 0x5414;
const TIOCMGET: u64 = 0x5415;
const TIOCMBIS: u64 = 0x5416;
const TIOCMBIC: u64 = 0x5417;
const TIOCMSET: u64 = 0x5418;
const FIONBIO: u64 = 0x5421;
const TIOCNOTTY: u64 = 0x5422;
const TIOCSETD: u64 = 0x5423;
const TIOCGETD: u64 = 0x5424;
const TCSBRKP: u64 = 0x5425;
const TIOCSCTTY: u64 = 0x540E;
const TIOCGSID: u64 = 0x5429;
const TIOCGPTN: u64 = 0x80045430;
const TIOCSPTLCK: u64 = 0x40045431;
const FIONREAD: u64 = 0x541B;
const TIOCGDEV: u64 = 0x80045432;
const TCFLSH: u64 = 0x540B;
const TIOCGICOUNT: u64 = 0x545D;
const FIOASYNC: u64 = 0x5452;
const FIOSETOWN: u64 = 0x8901;
const FIOGETOWN: u64 = 0x8903;
const BLKGETSIZE64: u64 = 0x80081272;
const BLKSSZGET: u64 = 0x1268;
const BLKRRPART: u64 = 0x125F;
const BLKFLSBUF: u64 = 0x1261;
const FICLONE: u64 = 0x40049409;
const FICLONERANGE: u64 = 0x4020940D;

const FILE_FD_MAX: usize = 128;
const RAM_FILE_MAX: usize = 16;
const RAM_FILE_CAPACITY: usize = 65_536;
const SOCKET_MAX: usize = 8;
const FILE_KIND_STDIN: u8 = 1;
const FILE_KIND_STDOUT: u8 = 2;
const FILE_KIND_NULL: u8 = 3;
const FILE_KIND_ZERO: u8 = 4;
const FILE_KIND_BINARY: u8 = 5;
const FILE_KIND_TEXT: u8 = 6;
const FILE_KIND_TTY: u8 = 7;
const FILE_KIND_PIPE_READ: u8 = 8;
const FILE_KIND_PIPE_WRITE: u8 = 9;
const FILE_KIND_EVENTFD: u8 = 10;
const FILE_KIND_TIMERFD: u8 = 11;
const FILE_KIND_SIGNALFD: u8 = 12;
const FILE_KIND_EPOLL: u8 = 13;
const FILE_KIND_INOTIFY: u8 = 14;
const FILE_KIND_SOCKET: u8 = 15;
const FILE_KIND_RAMFILE: u8 = 16;

const PIPE_MAX: usize = 32;
const PIPE_CAPACITY: usize = 8192;
const EVENTFD_MAX: usize = 16;
const TIMERFD_MAX: usize = 16;
const SIGNALFD_MAX: usize = 16;
const EPOLL_MAX: usize = 8;
const EPOLL_EVENTS_MAX: usize = 64;

#[derive(Clone, Copy)]
struct RamFile {
    used: bool,
    path: [u8; 160],
    path_len: usize,
    len: usize,
    mode: u32,
    data: [u8; RAM_FILE_CAPACITY],
}
impl RamFile {
    const fn empty() -> Self {
        Self { used: false, path: [0; 160], path_len: 0, len: 0, mode: 0o100644, data: [0; RAM_FILE_CAPACITY] }
    }
}

#[derive(Clone, Copy)]
struct SocketPair {
    used: bool,
    a_to_b: u8,
    b_to_a: u8,
}
impl SocketPair {
    const fn empty() -> Self {
        Self { used: false, a_to_b: u8::MAX, b_to_a: u8::MAX }
    }
}

static mut RAM_FILES: [RamFile; RAM_FILE_MAX] = [RamFile::empty(); RAM_FILE_MAX];
static mut SOCKET_PAIRS: [SocketPair; SOCKET_MAX] = [SocketPair::empty(); SOCKET_MAX];

#[derive(Clone, Copy)]
struct PipeBuffer {
    used: bool,
    data: [u8; PIPE_CAPACITY],
    read_pos: usize,
    write_pos: usize,
    len: usize,
}
impl PipeBuffer {
    const fn empty() -> Self {
        Self {
            used: false,
            data: [0; PIPE_CAPACITY],
            read_pos: 0,
            write_pos: 0,
            len: 0,
        }
    }
}
#[derive(Clone, Copy)]
struct EventFdState {
    used: bool,
    counter: u64,
    semaphore: bool,
}
impl EventFdState {
    const fn empty() -> Self {
        Self {
            used: false,
            counter: 0,
            semaphore: false,
        }
    }
}
#[derive(Clone, Copy)]
struct TimerFdState {
    used: bool,
    expires_ns: u64,
    interval_ns: u64,
    last_fire_ns: u64,
    armed: bool,
    absolute: bool,
}
impl TimerFdState {
    const fn empty() -> Self {
        Self {
            used: false,
            expires_ns: 0,
            interval_ns: 0,
            last_fire_ns: 0,
            armed: false,
            absolute: false,
        }
    }
}
#[derive(Clone, Copy)]
struct SignalFdState {
    used: bool,
    mask: u64,
    blocked: bool,
}
impl SignalFdState {
    const fn empty() -> Self {
        Self {
            used: false,
            mask: 0,
            blocked: false,
        }
    }
}
#[derive(Clone, Copy)]
struct EpollState {
    used: bool,
    watched_fds: [i32; EPOLL_EVENTS_MAX],
    watched_events: [u32; EPOLL_EVENTS_MAX],
    watched_data: [u64; EPOLL_EVENTS_MAX],
    count: usize,
}
impl EpollState {
    const fn empty() -> Self {
        Self {
            used: false,
            watched_fds: [0; EPOLL_EVENTS_MAX],
            watched_events: [0; EPOLL_EVENTS_MAX],
            watched_data: [0; EPOLL_EVENTS_MAX],
            count: 0,
        }
    }
}

static mut PIPE_BUFFERS: [PipeBuffer; PIPE_MAX] = [PipeBuffer::empty(); PIPE_MAX];
static mut EVENTFDS: [EventFdState; EVENTFD_MAX] = [EventFdState::empty(); EVENTFD_MAX];
static mut TIMERFDS: [TimerFdState; TIMERFD_MAX] = [TimerFdState::empty(); TIMERFD_MAX];
static mut SIGNALFDS: [SignalFdState; SIGNALFD_MAX] = [SignalFdState::empty(); SIGNALFD_MAX];
static mut EPOLLS: [EpollState; EPOLL_MAX] = [EpollState::empty(); EPOLL_MAX];

fn pipe_slot_referenced(id: usize) -> bool {
    unsafe {
        let state = state_mut();
        for fd in state.fds.iter() {
            if (fd.kind == FILE_KIND_PIPE_READ || fd.kind == FILE_KIND_PIPE_WRITE)
                && fd.path_len == 1
                && fd.path[0] as usize == id
            {
                return true;
            }
        }
    }
    false
}
fn pipe_alloc_slot() -> Option<u8> {
    unsafe {
        state_mut().init();
        for id in 0..PIPE_MAX {
            if pipe_slot_referenced(id) {
                continue;
            }
            PIPE_BUFFERS[id] = PipeBuffer::empty();
            PIPE_BUFFERS[id].used = true;
            return Some(id as u8);
        }
    }
    None
}
fn pipe_has_reader(id: usize) -> bool {
    unsafe {
        let state = state_mut();
        for fd in state.fds.iter() {
            if fd.kind == FILE_KIND_PIPE_READ && fd.path_len == 1 && fd.path[0] as usize == id {
                return true;
            }
        }
    }
    false
}
fn pipe_has_writer(id: usize) -> bool {
    unsafe {
        let state = state_mut();
        for fd in state.fds.iter() {
            if fd.kind == FILE_KIND_PIPE_WRITE && fd.path_len == 1 && fd.path[0] as usize == id {
                return true;
            }
        }
    }
    false
}
fn pipe_available(id: usize) -> Option<usize> {
    if id >= PIPE_MAX {
        return None;
    }
    unsafe {
        if PIPE_BUFFERS[id].used {
            Some(PIPE_BUFFERS[id].len)
        } else {
            None
        }
    }
}

fn create_pipe(user_address: u64, flags: u64) -> u64 {
    if flags & !(O_CLOEXEC | O_NONBLOCK | O_DIRECT) != 0 {
        return neg_errno(EINVAL);
    }
    let Some(pipe_id) = pipe_alloc_slot() else {
        return neg_errno(EMFILE);
    };
    let mut read_path = [0u8; 160];
    read_path[0] = pipe_id;
    let mut write_path = [0u8; 160];
    write_path[0] = pipe_id;
    let read_fd = unsafe {
        state_mut().alloc_fd(Fd {
            used: true,
            kind: FILE_KIND_PIPE_READ,
            flags,
            position: 0,
            path: read_path,
            path_len: 1,
        })
    };
    let Some(read_fd) = read_fd else {
        unsafe {
            PIPE_BUFFERS[pipe_id as usize].used = false;
        }
        return neg_errno(EMFILE);
    };
    let write_fd = unsafe {
        state_mut().alloc_fd(Fd {
            used: true,
            kind: FILE_KIND_PIPE_WRITE,
            flags: flags | O_WRONLY,
            position: 0,
            path: write_path,
            path_len: 1,
        })
    };
    let Some(write_fd) = write_fd else {
        unsafe {
            state_mut().fds[read_fd as usize] = Fd::empty();
            PIPE_BUFFERS[pipe_id as usize].used = false;
        }
        return neg_errno(EMFILE);
    };
    let mut pair = [0u8; 8];
    pair[..4].copy_from_slice(&(read_fd as i32).to_le_bytes());
    pair[4..].copy_from_slice(&(write_fd as i32).to_le_bytes());
    if copy_user_out(user_address, &pair).is_err() {
        unsafe {
            state_mut().fds[read_fd as usize] = Fd::empty();
            state_mut().fds[write_fd as usize] = Fd::empty();
            PIPE_BUFFERS[pipe_id as usize].used = false;
        }
        return neg_errno(EFAULT);
    }
    0
}

fn pipe_read_fd(fd_number: i32, address: u64, count: usize) -> u64 {
    if count == 0 {
        return 0;
    }
    let fd = match fd_snapshot(fd_number) {
        Ok(fd) => fd,
        Err(code) => return code,
    };
    if fd.kind != FILE_KIND_PIPE_READ || fd.path_len != 1 {
        return neg_errno(EBADF);
    }
    let id = fd.path[0] as usize;
    let Some(available) = pipe_available(id) else {
        return neg_errno(EBADF);
    };
    if available == 0 {
        if !pipe_has_writer(id) {
            return 0;
        }
        return neg_errno(EAGAIN);
    }
    let amount = core::cmp::min(count, available);
    let mut buffer = [0u8; PIPE_CAPACITY];
    unsafe {
        let pipe = &PIPE_BUFFERS[id];
        let first = core::cmp::min(amount, PIPE_CAPACITY - pipe.read_pos);
        buffer[..first].copy_from_slice(&pipe.data[pipe.read_pos..pipe.read_pos + first]);
        let second = amount - first;
        if second != 0 {
            buffer[first..amount].copy_from_slice(&pipe.data[..second]);
        }
    }
    if copy_user_out(address, &buffer[..amount]).is_err() {
        return neg_errno(EFAULT);
    }
    unsafe {
        let pipe = &mut PIPE_BUFFERS[id];
        pipe.read_pos = (pipe.read_pos + amount) % PIPE_CAPACITY;
        pipe.len -= amount;
    }
    amount as u64
}

fn pipe_write_fd(fd_number: i32, address: u64, count: usize) -> u64 {
    if count == 0 {
        return 0;
    }
    let fd = match fd_snapshot(fd_number) {
        Ok(fd) => fd,
        Err(code) => return code,
    };
    if fd.kind != FILE_KIND_PIPE_WRITE || fd.path_len != 1 {
        return neg_errno(EBADF);
    }
    let id = fd.path[0] as usize;
    let Some(available) = pipe_available(id) else {
        return neg_errno(EBADF);
    };
    if !pipe_has_reader(id) {
        return neg_errno(EPIPE);
    }
    let free = PIPE_CAPACITY.saturating_sub(available);
    if free == 0 {
        return neg_errno(EAGAIN);
    }
    let amount = core::cmp::min(count, free);
    let mut buffer = [0u8; PIPE_CAPACITY];
    if copy_user_in(address, &mut buffer[..amount]).is_err() {
        return neg_errno(EFAULT);
    }
    unsafe {
        let pipe = &mut PIPE_BUFFERS[id];
        let first = core::cmp::min(amount, PIPE_CAPACITY - pipe.write_pos);
        pipe.data[pipe.write_pos..pipe.write_pos + first].copy_from_slice(&buffer[..first]);
        let second = amount - first;
        if second != 0 {
            pipe.data[..second].copy_from_slice(&buffer[first..amount]);
        }
        pipe.write_pos = (pipe.write_pos + amount) % PIPE_CAPACITY;
        pipe.len += amount;
    }
    amount as u64
}

fn eventfd_alloc(init: u64, flags: u64, semaphore: bool) -> i64 {
    if flags & !(EFD_CLOEXEC | EFD_NONBLOCK) != 0 {
        return -1;
    }
    unsafe {
        state_mut().init();
        for id in 0..EVENTFD_MAX {
            if !EVENTFDS[id].used {
                EVENTFDS[id] = EventFdState {
                    used: true,
                    counter: init,
                    semaphore,
                };
                let mut path = [0u8; 160];
                path[0] = id as u8;
                match state_mut().alloc_fd(Fd {
                    used: true,
                    kind: FILE_KIND_EVENTFD,
                    flags,
                    position: 0,
                    path,
                    path_len: 1,
                }) {
                    Some(fd) => return fd as i64,
                    None => {
                        EVENTFDS[id].used = false;
                        return -1;
                    }
                }
            }
        }
    }
    -1
}

fn timerfd_alloc(clock_id: u64, flags: u64) -> i64 {
    if flags & !(TFD_CLOEXEC | TFD_NONBLOCK) != 0 {
        return -1;
    }
    if clock_id != CLOCK_REALTIME && clock_id != CLOCK_MONOTONIC && clock_id != CLOCK_BOOTTIME {
        return -1;
    }
    unsafe {
        state_mut().init();
        for id in 0..TIMERFD_MAX {
            if !TIMERFDS[id].used {
                TIMERFDS[id] = TimerFdState::empty();
                TIMERFDS[id].used = true;
                let mut path = [0u8; 160];
                path[0] = id as u8;
                match state_mut().alloc_fd(Fd {
                    used: true,
                    kind: FILE_KIND_TIMERFD,
                    flags,
                    position: 0,
                    path,
                    path_len: 1,
                }) {
                    Some(fd) => return fd as i64,
                    None => {
                        TIMERFDS[id].used = false;
                        return -1;
                    }
                }
            }
        }
    }
    -1
}

fn signalfd_alloc(mask: u64, flags: u64) -> i64 {
    if flags & !(SFD_CLOEXEC | SFD_NONBLOCK) != 0 {
        return -1;
    }
    unsafe {
        state_mut().init();
        for id in 0..SIGNALFD_MAX {
            if !SIGNALFDS[id].used {
                SIGNALFDS[id] = SignalFdState {
                    used: true,
                    mask,
                    blocked: false,
                };
                let mut path = [0u8; 160];
                path[0] = id as u8;
                match state_mut().alloc_fd(Fd {
                    used: true,
                    kind: FILE_KIND_SIGNALFD,
                    flags,
                    position: 0,
                    path,
                    path_len: 1,
                }) {
                    Some(fd) => return fd as i64,
                    None => {
                        SIGNALFDS[id].used = false;
                        return -1;
                    }
                }
            }
        }
    }
    -1
}

fn epoll_alloc(flags: u64) -> i64 {
    if flags & !EPOLL_CLOEXEC != 0 {
        return -1;
    }
    unsafe {
        state_mut().init();
        for id in 0..EPOLL_MAX {
            if !EPOLLS[id].used {
                EPOLLS[id] = EpollState::empty();
                EPOLLS[id].used = true;
                let mut path = [0u8; 160];
                path[0] = id as u8;
                match state_mut().alloc_fd(Fd {
                    used: true,
                    kind: FILE_KIND_EPOLL,
                    flags,
                    position: 0,
                    path,
                    path_len: 1,
                }) {
                    Some(fd) => return fd as i64,
                    None => {
                        EPOLLS[id].used = false;
                        return -1;
                    }
                }
            }
        }
    }
    -1
}

#[repr(C)]
pub(crate) struct SyscallFrame {
    pub(crate) r15: u64,
    pub(crate) r14: u64,
    pub(crate) r13: u64,
    pub(crate) r12: u64,
    pub(crate) r10: u64,
    pub(crate) r9: u64,
    pub(crate) r8: u64,
    pub(crate) rdi: u64,
    pub(crate) rsi: u64,
    pub(crate) rbp: u64,
    pub(crate) rbx: u64,
    pub(crate) rdx: u64,
    pub(crate) rax: u64,
    pub(crate) syscall_r11: u64,
    pub(crate) syscall_rcx: u64,
}

#[derive(Clone, Copy)]
struct Fd {
    used: bool,
    kind: u8,
    flags: u64,
    position: u64,
    path: [u8; 160],
    path_len: usize,
}
impl Fd {
    const fn empty() -> Self {
        Self {
            used: false,
            kind: 0,
            flags: 0,
            position: 0,
            path: [0; 160],
            path_len: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct SignalAction {
    handler: u64,
    flags: u64,
    restorer: u64,
    mask: u64,
}
impl SignalAction {
    const fn empty() -> Self {
        Self {
            handler: 0,
            flags: 0,
            restorer: 0,
            mask: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct AltStack {
    sp: u64,
    size: u64,
    flags: u64,
}
impl AltStack {
    const fn empty() -> Self {
        Self {
            sp: 0,
            size: 0,
            flags: SS_DISABLE,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ProcessState {
    initialized: bool,
    fds: [Fd; FILE_FD_MAX],
    next_mmap: u64,
    brk: u64,
    fs_base: u64,
    cwd: [u8; 160],
    cwd_len: usize,
    exit_requested: bool,
    exit_code: i32,
    uid: u32,
    gid: u32,
    euid: u32,
    egid: u32,
    suid: u32,
    sgid: u32,
    fsuid: u32,
    fsgid: u32,
    pid: u64,
    ppid: u64,
    pgid: u64,
    sid: u64,
    umask: u32,
    signal_mask: u64,
    pending_signals: u64,
    signal_actions: [SignalAction; 64],
    altstack: AltStack,
    tid_address: u64,
    robust_list: u64,
    robust_len: u64,
    rseq: u64,
    rseq_len: u32,
    pdeath_signal: u32,
    dumpable: u32,
    no_new_privs: bool,
    personality: u64,
    process_name: [u8; 16],
    rng_state: u64,
    nice: i32,
    priority: i32,
    scheduler: u32,
    timerslack_ns: u64,
    clear_child_tid: u64,
}

impl ProcessState {
    const fn new() -> Self {
        Self {
            initialized: false,
            fds: [Fd::empty(); FILE_FD_MAX],
            next_mmap: MMAP_BASE,
            brk: HEAP_BASE,
            fs_base: 0,
            cwd: [0; 160],
            cwd_len: 0,
            exit_requested: false,
            exit_code: 0,
            uid: 1000,
            gid: 1000,
            euid: 1000,
            egid: 1000,
            suid: 1000,
            sgid: 1000,
            fsuid: 1000,
            fsgid: 1000,
            pid: 1,
            ppid: 0,
            pgid: 1,
            sid: 1,
            umask: 0o022,
            signal_mask: 0,
            pending_signals: 0,
            signal_actions: [SignalAction::empty(); 64],
            altstack: AltStack::empty(),
            tid_address: 0,
            robust_list: 0,
            robust_len: 0,
            rseq: 0,
            rseq_len: 0,
            pdeath_signal: 0,
            dumpable: 1,
            no_new_privs: false,
            personality: 0,
            process_name: *b"bash\0\0\0\0\0\0\0\0\0\0\0\0",
            rng_state: 0x6A09_E667_F3BC_C909,
            nice: 0,
            priority: 0,
            scheduler: 0,
            timerslack_ns: 50_000,
            clear_child_tid: 0,
        }
    }
    fn init(&mut self) {
        if self.initialized {
            return;
        }
        self.initialized = true;
        for i in 0..3 {
            self.fds[i] = Fd {
                used: true,
                kind: FILE_KIND_TTY,
                flags: if i == 0 { O_RDONLY } else { O_WRONLY },
                position: 0,
                path: [0; 160],
                path_len: 8,
            };
        }
        let cwd = b"/home/boi";
        self.cwd[..cwd.len()].copy_from_slice(cwd);
        self.cwd_len = cwd.len();
    }
    fn alloc_fd(&mut self, fd: Fd) -> Option<i32> {
        for index in 3..FILE_FD_MAX {
            if !self.fds[index].used {
                self.fds[index] = fd;
                return Some(index as i32);
            }
        }
        None
    }
}

static mut STATE: ProcessState = ProcessState::new();
static mut USER_RETURN_RSP: u64 = 0;
static mut USER_HOST_RSP: u64 = 0;
#[unsafe(no_mangle)]
pub static mut SYSCALL_SAVED_RSP: u64 = 0;
static PENDING_SIGNALS: AtomicU64 = AtomicU64::new(0);

pub(crate) fn snapshot_process_state() -> ProcessState {
    unsafe {
        let state = state_mut();
        state.pending_signals = PENDING_SIGNALS.load(Ordering::Acquire);
        *state
    }
}
pub(crate) fn snapshot_process_state_into(destination: *mut ProcessState) {
    unsafe {
        let state = state_mut();
        state.pending_signals = PENDING_SIGNALS.load(Ordering::Acquire);
        destination.write(*state);
    }
}
pub(crate) fn restore_process_state(snapshot: ProcessState) {
    unsafe {
        *state_mut() = snapshot;
        PENDING_SIGNALS.store(snapshot.pending_signals, Ordering::Release);
    }
}
pub(crate) fn saved_user_rsp() -> u64 {
    unsafe { SYSCALL_SAVED_RSP }
}
pub(crate) fn child_saved_user_rsp() -> u64 {
    unsafe { core::ptr::read_volatile(core::ptr::addr_of!(vibux_child_syscall_saved_rsp)) }
}
pub(crate) fn process_physical_offset() -> u64 {
    crate::velf::process_physical_offset()
}
pub(crate) fn fs_base() -> u64 {
    unsafe { rdmsr(IA32_FS_BASE) }
}
pub(crate) fn set_fs_base(value: u64) {
    unsafe {
        wrmsr(IA32_FS_BASE, value);
    }
}
pub(crate) fn set_pid(pid: u64) {
    unsafe {
        state_mut().pid = pid;
    }
}
pub(crate) fn pid() -> u64 {
    unsafe { state_mut().pid }
}

#[inline(always)]
unsafe fn state_mut() -> &'static mut ProcessState {
    unsafe { &mut *core::ptr::addr_of_mut!(STATE) }
}
#[inline(always)]
unsafe fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    unsafe {
        asm!("rdmsr", in("ecx") msr, lateout("eax") low, lateout("edx") high, options(nostack, preserves_flags));
    }
    ((high as u64) << 32) | low as u64
}
#[inline(always)]
unsafe fn wrmsr(msr: u32, value: u64) {
    unsafe {
        asm!("wrmsr", in("ecx") msr, in("eax") value as u32, in("edx") (value >> 32) as u32, options(nostack, preserves_flags));
    }
}
#[inline(always)]
unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        asm!("in al, dx", in("dx") port, lateout("al") value, options(nostack, preserves_flags));
    }
    value
}
#[inline(always)]
unsafe fn outb(port: u16, value: u8) {
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") value, options(nostack, preserves_flags));
    }
}
#[inline(always)]
fn serial_ready() -> bool {
    unsafe { inb(0x3FD) & 1 != 0 }
}
#[inline(always)]
fn serial_read() -> u8 {
    unsafe { inb(0x3F8) }
}
#[inline(always)]
#[cfg(debug_assertions)]
fn serial_write(byte: u8) {
    unsafe {
        while inb(0x3FD) & 0x20 == 0 {
            core::hint::spin_loop();
        }
        outb(0x3F8, byte);
    }
}
#[inline(always)]
#[cfg(not(debug_assertions))]
fn serial_write(_: u8) {}
#[inline(always)]
#[cfg(debug_assertions)]
fn serial_write_all(bytes: &[u8]) {
    for &byte in bytes {
        serial_write(byte);
    }
}
#[inline(always)]
#[cfg(not(debug_assertions))]
fn serial_write_all(_: &[u8]) {}
#[allow(dead_code)]
fn wait_for_serial() {
    while !serial_ready() {
        unsafe {
            core::hint::spin_loop();
        }
    }
}

static KEYBOARD_SHIFT: AtomicBool = AtomicBool::new(false);
static KEYBOARD_CTRL: AtomicBool = AtomicBool::new(false);
static KEYBOARD_CAPS: AtomicBool = AtomicBool::new(false);
#[inline(always)]
fn keyboard_letter(base: u8, caps: bool, shifted: bool) -> u8 {
    if caps ^ shifted {
        base.to_ascii_uppercase()
    } else {
        base
    }
}
fn scancode_to_ascii(scancode: u8) -> Option<u8> {
    let released = scancode & 0x80 != 0;
    let code = scancode & 0x7f;
    match code {
        0x2a | 0x36 => {
            KEYBOARD_SHIFT.store(!released, Ordering::Relaxed);
            return None;
        }
        0x1d => {
            KEYBOARD_CTRL.store(!released, Ordering::Relaxed);
            return None;
        }
        0x3a if !released => {
            let caps = KEYBOARD_CAPS.load(Ordering::Relaxed);
            KEYBOARD_CAPS.store(!caps, Ordering::Relaxed);
            return None;
        }
        _ if released => return None,
        _ => {}
    }
    let shifted = KEYBOARD_SHIFT.load(Ordering::Relaxed);
    let ctrl = KEYBOARD_CTRL.load(Ordering::Relaxed);
    let caps = KEYBOARD_CAPS.load(Ordering::Relaxed);
    if ctrl {
        match code {
            0x2e => return Some(0x03),
            0x20 => return Some(0x04),
            0x26 => return Some(0x0c),
            _ => {}
        }
    }
    let byte = match code {
        0x01 => 0x1b,
        0x02 => {
            if shifted {
                b'!'
            } else {
                b'1'
            }
        }
        0x03 => {
            if shifted {
                b'@'
            } else {
                b'2'
            }
        }
        0x04 => {
            if shifted {
                b'#'
            } else {
                b'3'
            }
        }
        0x05 => {
            if shifted {
                b'$'
            } else {
                b'4'
            }
        }
        0x06 => {
            if shifted {
                b'%'
            } else {
                b'5'
            }
        }
        0x07 => {
            if shifted {
                b'^'
            } else {
                b'6'
            }
        }
        0x08 => {
            if shifted {
                b'&'
            } else {
                b'7'
            }
        }
        0x09 => {
            if shifted {
                b'*'
            } else {
                b'8'
            }
        }
        0x0a => {
            if shifted {
                b'('
            } else {
                b'9'
            }
        }
        0x0b => {
            if shifted {
                b')'
            } else {
                b'0'
            }
        }
        0x0c => {
            if shifted {
                b'_'
            } else {
                b'-'
            }
        }
        0x0d => {
            if shifted {
                b'+'
            } else {
                b'='
            }
        }
        0x0e => 0x7f,
        0x0f => 0x09,
        0x10 => keyboard_letter(b'q', caps, shifted),
        0x11 => keyboard_letter(b'w', caps, shifted),
        0x12 => keyboard_letter(b'e', caps, shifted),
        0x13 => keyboard_letter(b'r', caps, shifted),
        0x14 => keyboard_letter(b't', caps, shifted),
        0x15 => keyboard_letter(b'y', caps, shifted),
        0x16 => keyboard_letter(b'u', caps, shifted),
        0x17 => keyboard_letter(b'i', caps, shifted),
        0x18 => keyboard_letter(b'o', caps, shifted),
        0x19 => keyboard_letter(b'p', caps, shifted),
        0x1a => {
            if shifted {
                b'{'
            } else {
                b'['
            }
        }
        0x1b => {
            if shifted {
                b'}'
            } else {
                b']'
            }
        }
        0x1c => 0x0a,
        0x1e => keyboard_letter(b'a', caps, shifted),
        0x1f => keyboard_letter(b's', caps, shifted),
        0x20 => keyboard_letter(b'd', caps, shifted),
        0x21 => keyboard_letter(b'f', caps, shifted),
        0x22 => keyboard_letter(b'g', caps, shifted),
        0x23 => keyboard_letter(b'h', caps, shifted),
        0x24 => keyboard_letter(b'j', caps, shifted),
        0x25 => keyboard_letter(b'k', caps, shifted),
        0x26 => keyboard_letter(b'l', caps, shifted),
        0x27 => {
            if shifted {
                b':'
            } else {
                b';'
            }
        }
        0x28 => {
            if shifted {
                b'"'
            } else {
                0x27
            }
        }
        0x29 => {
            if shifted {
                b'~'
            } else {
                b'`'
            }
        }
        0x2b => {
            if shifted {
                b'|'
            } else {
                0x5c
            }
        }
        0x2c => keyboard_letter(b'z', caps, shifted),
        0x2d => keyboard_letter(b'x', caps, shifted),
        0x2e => keyboard_letter(b'c', caps, shifted),
        0x2f => keyboard_letter(b'v', caps, shifted),
        0x30 => keyboard_letter(b'b', caps, shifted),
        0x31 => keyboard_letter(b'n', caps, shifted),
        0x32 => keyboard_letter(b'm', caps, shifted),
        0x33 => {
            if shifted {
                b'<'
            } else {
                b','
            }
        }
        0x34 => {
            if shifted {
                b'>'
            } else {
                b'.'
            }
        }
        0x35 => {
            if shifted {
                b'?'
            } else {
                b'/'
            }
        }
        0x39 => b' ',
        _ => return None,
    };
    Some(byte)
}

#[allow(dead_code)]
fn next_input_byte() -> u8 {
    loop {
        if serial_ready() {
            return serial_read();
        }
        if let Some(scancode) = crate::arch::x86_64::pop_keyboard_scancode() {
            if let Some(byte) = scancode_to_ascii(scancode) {
                return byte;
            }
            continue;
        }
        unsafe {
            asm!("sti; hlt", options(nomem, nostack));
        }
    }
}

#[inline(always)]
fn current_ticks() -> u64 {
    crate::arch::x86_64::timer_ticks()
}
#[inline(always)]
fn ticks_to_ns(ticks: u64) -> u64 {
    ticks.saturating_mul(4_000_000)
}

pub(crate) fn copy_user_in(address: u64, output: &mut [u8]) -> Result<(), u64> {
    if output.is_empty() {
        return Ok(());
    }
    let end = address.checked_add(output.len() as u64).ok_or(EFAULT)?;
    if address >= USER_LIMIT || end > USER_LIMIT || end < address {
        return Err(EFAULT);
    }
    let paging =
        crate::arch::x86_64::paging::PageTableManager::new(crate::velf::process_physical_offset());
    let mut offset = 0usize;
    while offset < output.len() {
        let virtual_address = address + offset as u64;
        if paging.translate(virtual_address).is_none() {
            return Err(EFAULT);
        }
        let page_left = (PAGE_SIZE - (virtual_address & (PAGE_SIZE - 1))) as usize;
        let amount = core::cmp::min(page_left, output.len() - offset);
        unsafe {
            core::ptr::copy_nonoverlapping(
                virtual_address as *const u8,
                output.as_mut_ptr().add(offset),
                amount,
            );
        }
        offset += amount;
    }
    Ok(())
}

pub(crate) fn copy_user_out(address: u64, input: &[u8]) -> Result<(), u64> {
    if input.is_empty() {
        return Ok(());
    }
    let end = address.checked_add(input.len() as u64).ok_or(EFAULT)?;
    if address >= USER_LIMIT || end > USER_LIMIT || end < address {
        return Err(EFAULT);
    }
    let paging =
        crate::arch::x86_64::paging::PageTableManager::new(crate::velf::process_physical_offset());
    let mut offset = 0usize;
    while offset < input.len() {
        let virtual_address = address + offset as u64;
        if paging.translate(virtual_address).is_none() {
            return Err(EFAULT);
        }
        let page_left = (PAGE_SIZE - (virtual_address & (PAGE_SIZE - 1))) as usize;
        let amount = core::cmp::min(page_left, input.len() - offset);
        unsafe {
            core::ptr::copy_nonoverlapping(
                input.as_ptr().add(offset),
                virtual_address as *mut u8,
                amount,
            );
        }
        offset += amount;
    }
    Ok(())
}

#[inline(always)]
fn copy_user_zero(address: u64, len: usize) -> u64 {
    if len == 0 { return 0; }
    let end = match address.checked_add(len as u64) { Some(v) => v, None => return neg_errno(EFAULT) };
    if address >= USER_LIMIT || end > USER_LIMIT || end < address { return neg_errno(EFAULT); }
    let paging = PageTableManager::new(crate::velf::process_physical_offset());
    let mut offset = 0usize;
    while offset < len {
        let va = address + offset as u64;
        if paging.translate(va).is_none() { return neg_errno(EFAULT); }
        let page_left = (PAGE_SIZE - (va & (PAGE_SIZE - 1))) as usize;
        let n = core::cmp::min(page_left, len - offset);
        unsafe { write_bytes(va as *mut u8, 0, n); }
        offset += n;
    }
    0
}
pub(crate) fn read_c_string(address: u64, output: &mut [u8]) -> Result<usize, u64> {
    if output.is_empty() || address >= USER_LIMIT { return Err(EINVAL); }
    let paging = PageTableManager::new(crate::velf::process_physical_offset());
    let limit = output.len() - 1;
    let mut index = 0usize;
    while index < limit {
        let current = address.checked_add(index as u64).ok_or(EFAULT)?;
        let chunk = core::cmp::min((PAGE_SIZE - (current & (PAGE_SIZE - 1))) as usize, limit - index);
        if paging.translate(current).is_none() { return Err(EFAULT); }
        unsafe { core::ptr::copy_nonoverlapping(current as *const u8, output.as_mut_ptr().add(index), chunk); }
        if let Some(zero) = output[index..index + chunk].iter().position(|&b| b == 0) { return Ok(index + zero); }
        index += chunk;
    }
    Err(ENAMETOOLONG)
}

fn is_known_binary(path: &[u8]) -> bool {
    crate::velf::store::find(path).is_some()
}

fn text_file(path: &[u8]) -> Option<&'static [u8]> {
    match path {
        b"/etc/passwd" => {
            Some(b"root:x:0:0:root:/root:/bin/sh\nboi:x:1000:1000:Vibux User:/home/boi:/bin/sh\n")
        }
        b"/etc/group" => Some(b"root:x:0:\nboi:x:1000:\n"),
        b"/etc/hostname" => Some(b"vibux\n"),
        b"/etc/profile" => Some(b"# /etc/profile\nPATH=/bin:/usr/bin\nexport PATH\n"),
        b"/etc/bash.bashrc" => Some(b"# /etc/bash.bashrc\nPS1='vibux$ '\n"),
        b"/etc/bashrc" => Some(b"# /etc/bashrc\n"),
        b"/etc/mtab" => Some(b"/dev/nvme0n1p2 / vibuxfs rw 0 0\n"),
        b"/etc/fstab" => Some(b"# /etc/fstab\n"),
        b"/etc/resolv.conf" => Some(b"nameserver 10.0.2.3\n"),
        b"/etc/hosts" => Some(b"127.0.0.1 localhost\n::1 localhost\n"),
        b"/home/boi/.bashrc" => Some(b"# ~/.bashrc\n"),
        b"/home/boi/.bash_profile" => Some(b"# ~/.bash_profile\n"),
        b"/home/boi/.profile" => Some(b"# ~/.profile\n"),
        b"/root/.bashrc" => Some(b"# ~/.bashrc\n"),
        b"/root/.bash_profile" => Some(b"# ~/.bash_profile\n"),
        b"/root/.profile" => Some(b"# ~/.profile\n"),
        _ => None,
    }
}

#[inline(always)]
fn ram_file_index(path: &[u8]) -> Option<usize> {
    unsafe {
        let mut i = 0usize;
        while i < RAM_FILE_MAX {
            let f = &RAM_FILES[i];
            if f.used && f.path_len == path.len() && &f.path[..f.path_len] == path { return Some(i); }
            i += 1;
        }
    }
    None
}

fn ram_file_create(path: &[u8], mode: u32) -> Option<usize> {
    if path.is_empty() || path.len() > 160 { return None; }
    unsafe {
        let mut i = 0usize;
        while i < RAM_FILE_MAX {
            if !RAM_FILES[i].used {
                RAM_FILES[i] = RamFile::empty();
                RAM_FILES[i].used = true;
                RAM_FILES[i].mode = mode;
                RAM_FILES[i].path[..path.len()].copy_from_slice(path);
                RAM_FILES[i].path_len = path.len();
                return Some(i);
            }
            i += 1;
        }
    }
    None
}

#[inline(always)]
fn ram_file_open_fd(index: usize, flags: u64) -> u64 {
    unsafe {
        let file = &RAM_FILES[index];
        let mut path = [0u8; 160];
        path[..file.path_len].copy_from_slice(&file.path[..file.path_len]);
        state_mut().alloc_fd(Fd { used: true, kind: FILE_KIND_RAMFILE, flags, position: 0, path, path_len: file.path_len })
            .map(|fd| fd as u64).unwrap_or(neg_errno(EMFILE))
    }
}

#[inline(always)]
fn ram_file_size(fd: &Fd) -> Option<usize> { ram_file_index(&fd.path[..fd.path_len]).map(|i| unsafe { RAM_FILES[i].len }) }

fn ram_file_truncate(path: &[u8], size: usize) -> u64 {
    let Some(index) = ram_file_index(path) else { return neg_errno(ENOENT); };
    if size > RAM_FILE_CAPACITY { return neg_errno(EFBIG); }
    unsafe {
        let file = &mut RAM_FILES[index];
        if size > file.len { write_bytes(file.data.as_mut_ptr().add(file.len), 0, size - file.len); }
        file.len = size;
    }
    0
}

fn ram_file_read(fd_number: i32, address: u64, count: usize) -> u64 {
    let Ok(fd) = fd_snapshot(fd_number) else { return neg_errno(EBADF); };
    let Some(index) = ram_file_index(&fd.path[..fd.path_len]) else { return neg_errno(ENOENT); };
    let len = unsafe { RAM_FILES[index].len };
    let start = core::cmp::min(fd.position as usize, len);
    let amount = core::cmp::min(count, len.saturating_sub(start));
    if amount == 0 { return 0; }
    let mut done = 0usize;
    while done < amount {
        let n = core::cmp::min(amount - done, 4096);
        let data = unsafe { &RAM_FILES[index].data[start + done..start + done + n] };
        if copy_user_out(address + done as u64, data).is_err() { return if done == 0 { neg_errno(EFAULT) } else { done as u64 }; }
        done += n;
    }
    unsafe { state_mut().fds[fd_number as usize].position = (start + done) as u64; }
    done as u64
}

fn ram_file_write(fd_number: i32, address: u64, count: usize) -> u64 {
    let Ok(fd) = fd_snapshot(fd_number) else { return neg_errno(EBADF); };
    let Some(index) = ram_file_index(&fd.path[..fd.path_len]) else { return neg_errno(ENOENT); };
    let position = if fd.flags & O_APPEND != 0 { unsafe { RAM_FILES[index].len } } else { fd.position as usize };
    if position >= RAM_FILE_CAPACITY && count != 0 { return neg_errno(ENOSPC); }
    let amount = core::cmp::min(count, RAM_FILE_CAPACITY.saturating_sub(position));
    if amount == 0 { return 0; }
    let mut done = 0usize;
    while done < amount {
        let n = core::cmp::min(amount - done, 4096);
        let mut buf = [0u8; 4096];
        if copy_user_in(address + done as u64, &mut buf[..n]).is_err() { return if done == 0 { neg_errno(EFAULT) } else { done as u64 }; }
        unsafe { RAM_FILES[index].data[position + done..position + done + n].copy_from_slice(&buf[..n]); }
        done += n;
    }
    unsafe {
        let end = position + done;
        state_mut().fds[fd_number as usize].position = end as u64;
        if end > RAM_FILES[index].len { RAM_FILES[index].len = end; }
    }
    done as u64
}

#[inline(always)]
fn ram_file_remove(path: &[u8]) -> u64 {
    let Some(index) = ram_file_index(path) else { return neg_errno(ENOENT); };
    unsafe { RAM_FILES[index] = RamFile::empty(); }
    0
}

fn ram_file_rename(old: &[u8], new: &[u8]) -> u64 {
    if new.len() > 160 { return neg_errno(ENAMETOOLONG); }
    let Some(index) = ram_file_index(old) else { return neg_errno(ENOENT); };
    if let Some(existing) = ram_file_index(new) { unsafe { RAM_FILES[existing] = RamFile::empty(); } }
    unsafe {
        RAM_FILES[index].path = [0; 160];
        RAM_FILES[index].path[..new.len()].copy_from_slice(new);
        RAM_FILES[index].path_len = new.len();
    }
    0
}

#[inline(always)]
fn socket_pair_alloc() -> Option<usize> {
    unsafe {
        let mut s = 0usize;
        while s < SOCKET_MAX {
            if !SOCKET_PAIRS[s].used {
                let mut a = None;
                let mut i = 0usize;
                while i < PIPE_MAX { if !PIPE_BUFFERS[i].used { PIPE_BUFFERS[i] = PipeBuffer::empty(); PIPE_BUFFERS[i].used = true; a = Some(i as u8); break; } i += 1; }
                let Some(a_to_b) = a else { return None; };
                let mut b = None;
                let mut j = 0usize;
                while j < PIPE_MAX { if !PIPE_BUFFERS[j].used { PIPE_BUFFERS[j] = PipeBuffer::empty(); PIPE_BUFFERS[j].used = true; b = Some(j as u8); break; } j += 1; }
                let Some(b_to_a) = b else { PIPE_BUFFERS[a_to_b as usize] = PipeBuffer::empty(); return None; };
                SOCKET_PAIRS[s] = SocketPair { used: true, a_to_b, b_to_a };
                return Some(s);
            }
            s += 1;
        }
    }
    None
}

#[inline(always)]
fn socket_pipe_for(socket: usize, side: u8, write: bool) -> Option<usize> {
    unsafe {
        if socket >= SOCKET_MAX || !SOCKET_PAIRS[socket].used || side > 1 { return None; }
        let p = SOCKET_PAIRS[socket];
        Some(if side == 0 { if write { p.a_to_b } else { p.b_to_a } } else { if write { p.b_to_a } else { p.a_to_b } } as usize)
    }
}

fn socket_read_fd(fd: i32, address: u64, count: usize) -> u64 {
    let Ok(d) = fd_snapshot(fd) else { return neg_errno(EBADF); };
    if d.path_len != 2 { return neg_errno(ENOTCONN); }
    let Some(id) = socket_pipe_for(d.path[0] as usize, d.path[1], false) else { return neg_errno(ENOTCONN); };
    let available = unsafe { PIPE_BUFFERS[id].len };
    if available == 0 { return if d.flags & O_NONBLOCK != 0 { neg_errno(EAGAIN) } else { 0 }; }
    let amount = core::cmp::min(count, available);
    let mut buf = [0u8; PIPE_CAPACITY];
    unsafe {
        let p = &PIPE_BUFFERS[id];
        let first = core::cmp::min(amount, PIPE_CAPACITY - p.read_pos);
        buf[..first].copy_from_slice(&p.data[p.read_pos..p.read_pos + first]);
        if amount > first { buf[first..amount].copy_from_slice(&p.data[..amount - first]); }
    }
    if copy_user_out(address, &buf[..amount]).is_err() { return neg_errno(EFAULT); }
    unsafe { let p = &mut PIPE_BUFFERS[id]; p.read_pos = (p.read_pos + amount) % PIPE_CAPACITY; p.len -= amount; }
    amount as u64
}

fn socket_write_fd(fd: i32, address: u64, count: usize) -> u64 {
    let Ok(d) = fd_snapshot(fd) else { return neg_errno(EBADF); };
    if d.path_len != 2 { return neg_errno(ENOTCONN); }
    let Some(id) = socket_pipe_for(d.path[0] as usize, d.path[1], true) else { return neg_errno(ENOTCONN); };
    let free = PIPE_CAPACITY.saturating_sub(unsafe { PIPE_BUFFERS[id].len });
    if free == 0 { return neg_errno(EAGAIN); }
    let amount = core::cmp::min(count, free);
    let mut buf = [0u8; PIPE_CAPACITY];
    if copy_user_in(address, &mut buf[..amount]).is_err() { return neg_errno(EFAULT); }
    unsafe {
        let p = &mut PIPE_BUFFERS[id];
        let first = core::cmp::min(amount, PIPE_CAPACITY - p.write_pos);
        p.data[p.write_pos..p.write_pos + first].copy_from_slice(&buf[..first]);
        if amount > first { p.data[..amount - first].copy_from_slice(&buf[first..amount]); }
        p.write_pos = (p.write_pos + amount) % PIPE_CAPACITY;
        p.len += amount;
    }
    amount as u64
}

fn socket_pair_from_user(address: u64, flags: u64) -> u64 {
    if address == 0 { return neg_errno(EFAULT); }
    let Some(pair) = socket_pair_alloc() else { return neg_errno(EMFILE); };
    let a = unsafe { state_mut().alloc_fd(Fd { used: true, kind: FILE_KIND_SOCKET, flags: O_RDWR | (flags & (O_NONBLOCK | O_CLOEXEC)), position: 0, path: [0;160], path_len: 2 }) };
    let b = unsafe { state_mut().alloc_fd(Fd { used: true, kind: FILE_KIND_SOCKET, flags: O_RDWR | (flags & (O_NONBLOCK | O_CLOEXEC)), position: 0, path: [0;160], path_len: 2 }) };
    let (Some(a), Some(b)) = (a,b) else {
        if let Some(a) = a { unsafe { state_mut().fds[a as usize] = Fd::empty(); } }
        unsafe { SOCKET_PAIRS[pair] = SocketPair::empty(); }
        return neg_errno(EMFILE);
    };
    unsafe {
        state_mut().fds[a as usize].path[0] = pair as u8;
        state_mut().fds[a as usize].path[1] = 0;
        state_mut().fds[b as usize].path[0] = pair as u8;
        state_mut().fds[b as usize].path[1] = 1;
    }
    let mut out = [0u8;8];
    out[..4].copy_from_slice(&(a as i32).to_le_bytes());
    out[4..].copy_from_slice(&(b as i32).to_le_bytes());
    if copy_user_out(address, &out).is_err() {
        unsafe { state_mut().fds[a as usize] = Fd::empty(); state_mut().fds[b as usize] = Fd::empty(); SOCKET_PAIRS[pair] = SocketPair::empty(); }
        return neg_errno(EFAULT);
    }
    0
}

fn fd_for_binary(fd: &Fd) -> Option<&'static [u8]> {
    if fd.kind != FILE_KIND_BINARY {
        return None;
    }
    crate::velf::store::find(&fd.path[..fd.path_len]).map(|x| x.bytes)
}
fn fd_for_text(fd: &Fd) -> Option<&'static [u8]> {
    if fd.kind != FILE_KIND_TEXT {
        return None;
    }
    text_file(&fd.path[..fd.path_len])
}

fn file_identity(path: &[u8]) -> (u64, u64) {
    let mut hash = 0xcbf29ce484222325u64;
    for &byte in path {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3u64);
    }
    if hash == 0 {
        hash = 1;
    }
    (0x5649_4255_5800_0001u64, hash)
}

fn fill_stat(output_address: u64, mode: u32, size: u64, identity_path: &[u8]) -> u64 {
    let (dev, ino) = file_identity(identity_path);
    let mut stat = [0u8; 144];
    stat[0..8].copy_from_slice(&dev.to_le_bytes());
    stat[8..16].copy_from_slice(&ino.to_le_bytes());
    stat[16..24].copy_from_slice(&1u64.to_le_bytes());
    stat[24..28].copy_from_slice(&mode.to_le_bytes());
    stat[28..32].copy_from_slice(&1000u32.to_le_bytes());
    stat[32..36].copy_from_slice(&1000u32.to_le_bytes());
    stat[40..48].copy_from_slice(&0u64.to_le_bytes());
    stat[48..56].copy_from_slice(&size.to_le_bytes());
    stat[56..64].copy_from_slice(&4096u64.to_le_bytes());
    stat[64..72].copy_from_slice(&((size + 511) / 512).to_le_bytes());
    if copy_user_out(output_address, &stat).is_err() {
        return neg_errno(EFAULT);
    }
    0
}

fn is_directory_path(path: &[u8]) -> bool {
    matches!(
        path,
        b"/" | b"/home"
            | b"/home/boi"
            | b"/tmp"
            | b"/etc"
            | b"/usr"
            | b"/var"
            | b"/bin"
            | b"/dev"
            | b"/proc"
            | b"/proc/self"
            | b"/proc/1"
            | b"/root"
            | b"/sys"
            | b"/run"
    )
}

fn open_path(path: &[u8], flags: u64) -> u64 {
    unsafe {
        let state = state_mut();
        state.init();
        if path == b"." || path == b".." || is_directory_path(path) {
            if flags & O_WRONLY != 0 && flags & O_RDWR == 0 {
                return neg_errno(EISDIR);
            }
            let effective = if path == b"." {
                &state.cwd[..state.cwd_len]
            } else if path == b".." {
                b"/".as_slice()
            } else {
                path
            };
            let mut stored = [0u8; 160];
            let count = core::cmp::min(effective.len(), stored.len());
            stored[..count].copy_from_slice(&effective[..count]);
            return state
                .alloc_fd(Fd {
                    used: true,
                    kind: FILE_KIND_TEXT,
                    flags,
                    position: 0,
                    path: stored,
                    path_len: count,
                })
                .map(|x| x as u64)
                .unwrap_or(neg_errno(EMFILE));
        }
        if path == b"/dev/null" {
            return state
                .alloc_fd(Fd {
                    used: true,
                    kind: FILE_KIND_NULL,
                    flags,
                    position: 0,
                    path: [0; 160],
                    path_len: 0,
                })
                .map(|x| x as u64)
                .unwrap_or(neg_errno(EMFILE));
        }
        if path == b"/dev/zero" {
            return state
                .alloc_fd(Fd {
                    used: true,
                    kind: FILE_KIND_ZERO,
                    flags,
                    position: 0,
                    path: [0; 160],
                    path_len: 0,
                })
                .map(|x| x as u64)
                .unwrap_or(neg_errno(EMFILE));
        }
        if path == b"/dev/nvme0n1" {
            return state
                .alloc_fd(Fd {
                    used: true,
                    kind: FILE_KIND_BLOCK,
                    flags,
                    position: 0,
                    path: [0; 160],
                    path_len: 0,
                })
                .map(|x| x as u64)
                .unwrap_or(neg_errno(EMFILE));
        }
        if path == b"/dev/tty"
            || path == b"/dev/console"
            || path == b"/dev/stdin"
            || path == b"/dev/stdout"
            || path == b"/dev/stderr"
        {
            return state
                .alloc_fd(Fd {
                    used: true,
                    kind: FILE_KIND_TTY,
                    flags,
                    position: 0,
                    path: [0; 160],
                    path_len: 0,
                })
                .map(|x| x as u64)
                .unwrap_or(neg_errno(EMFILE));
        }
        if path == b"/dev/random" || path == b"/dev/urandom" {
            return state
                .alloc_fd(Fd {
                    used: true,
                    kind: FILE_KIND_ZERO,
                    flags,
                    position: 0,
                    path: [0; 160],
                    path_len: 0,
                })
                .map(|x| x as u64)
                .unwrap_or(neg_errno(EMFILE));
        }
        if path == b"/proc/self/exe" || path == b"/proc/1/exe" {
            let mut stored = [0u8; 160];
            let source = b"/bin/bash";
            stored[..source.len()].copy_from_slice(source);
            return state
                .alloc_fd(Fd {
                    used: true,
                    kind: FILE_KIND_BINARY,
                    flags,
                    position: 0,
                    path: stored,
                    path_len: source.len(),
                })
                .map(|x| x as u64)
                .unwrap_or(neg_errno(EMFILE));
        }
        if let Some(index) = ram_file_index(path) {
            if flags & O_EXCL != 0 && flags & O_CREAT != 0 { return neg_errno(EEXIST); }
            if flags & O_TRUNC != 0 && (flags & (O_WRONLY | O_RDWR)) != 0 { RAM_FILES[index].len = 0; }
            return ram_file_open_fd(index, flags);
        }
        if let Some(binary) = crate::velf::store::find(path) {
            if flags & (O_WRONLY | O_RDWR) != 0 && flags & O_TRUNC != 0 {
                return neg_errno(EROFS);
            }
            let mut stored = [0u8; 160];
            let count = core::cmp::min(binary.path.len(), stored.len());
            stored[..count].copy_from_slice(&binary.path[..count]);
            return state
                .alloc_fd(Fd {
                    used: true,
                    kind: FILE_KIND_BINARY,
                    flags,
                    position: 0,
                    path: stored,
                    path_len: count,
                })
                .map(|x| x as u64)
                .unwrap_or(neg_errno(EMFILE));
        }
        if text_file(path).is_some() {
            if flags & (O_WRONLY | O_RDWR) != 0 {
                return neg_errno(EROFS);
            }
            let mut stored = [0u8; 160];
            let count = core::cmp::min(path.len(), stored.len());
            stored[..count].copy_from_slice(&path[..count]);
            return state
                .alloc_fd(Fd {
                    used: true,
                    kind: FILE_KIND_TEXT,
                    flags,
                    position: 0,
                    path: stored,
                    path_len: count,
                })
                .map(|x| x as u64)
                .unwrap_or(neg_errno(EMFILE));
        }
        if flags & O_CREAT != 0 {
            let mode = 0o100000 | (0o777 & (flags >> 6) as u32);
            let Some(index) = ram_file_create(path, if mode & 0o777 == 0 { 0o100644 } else { mode }) else { return neg_errno(ENOSPC); };
            return ram_file_open_fd(index, flags);
        }
        neg_errno(ENOENT)
    }
}

fn read_fd(fd_number: i32, address: u64, count: usize) -> u64 {
    if fd_number < 0 || fd_number as usize >= FILE_FD_MAX {
        return neg_errno(EBADF);
    }
    if count == 0 {
        return 0;
    }
    let fd = match fd_snapshot(fd_number) {
        Ok(fd) => fd,
        Err(code) => return code,
    };
    if !fd_readable(&fd) {
        return neg_errno(EBADF);
    }
    if fd.kind == FILE_KIND_PIPE_READ {
        return pipe_read_fd(fd_number, address, count);
    }
    if fd.kind == FILE_KIND_EVENTFD {
        let id = fd.path[0] as usize;
        unsafe {
            if !EVENTFDS[id].used {
                return neg_errno(EBADF);
            }
            if EVENTFDS[id].counter == 0 {
                if fd.flags & O_NONBLOCK != 0 {
                    return neg_errno(EAGAIN);
                }
                return 0;
            }
            let value = if EVENTFDS[id].semaphore {
                EVENTFDS[id].counter -= 1;
                1u64
            } else {
                let v = EVENTFDS[id].counter;
                EVENTFDS[id].counter = 0;
                v
            };
            if count < 8 {
                return neg_errno(EINVAL);
            }
            if copy_user_out(address, &value.to_le_bytes()).is_err() {
                return neg_errno(EFAULT);
            }
            return 8;
        }
    }
    if fd.kind == FILE_KIND_TIMERFD {
        let id = fd.path[0] as usize;
        unsafe {
            if !TIMERFDS[id].used {
                return neg_errno(EBADF);
            }
            if !TIMERFDS[id].armed {
                if fd.flags & O_NONBLOCK != 0 {
                    return neg_errno(EAGAIN);
                }
                return 0;
            }
            let now = ticks_to_ns(current_ticks());
            if now < TIMERFDS[id].expires_ns {
                return neg_errno(EAGAIN);
            }
            let mut expirations = 1u64;
            if TIMERFDS[id].interval_ns != 0 {
                let elapsed = now - TIMERFDS[id].expires_ns;
                expirations = 1 + elapsed / TIMERFDS[id].interval_ns;
                TIMERFDS[id].expires_ns += expirations * TIMERFDS[id].interval_ns;
            } else {
                TIMERFDS[id].armed = false;
            }
            if count < 8 {
                return neg_errno(EINVAL);
            }
            if copy_user_out(address, &expirations.to_le_bytes()).is_err() {
                return neg_errno(EFAULT);
            }
            return 8;
        }
    }
    if fd.kind == FILE_KIND_SOCKET { return socket_read_fd(fd_number, address, count); }
    if fd.kind == FILE_KIND_RAMFILE { return ram_file_read(fd_number, address, count); }
    if fd.kind == FILE_KIND_SIGNALFD {
        let id = fd.path[0] as usize;
        unsafe {
            if !SIGNALFDS[id].used {
                return neg_errno(EBADF);
            }
            let mask = SIGNALFDS[id].mask;
            let pending = PENDING_SIGNALS.load(Ordering::Acquire) & mask;
            if pending == 0 {
                if fd.flags & O_NONBLOCK != 0 {
                    return neg_errno(EAGAIN);
                }
                return 0;
            }
            let signum = pending.trailing_zeros() as u64 + 1;
            PENDING_SIGNALS.fetch_and(!signal_bit(signum), Ordering::AcqRel);
            if count < 128 {
                return neg_errno(EINVAL);
            }
            let mut info = [0u8; 128];
            info[0..4].copy_from_slice(&(signum as u32).to_le_bytes());
            info[4..8].copy_from_slice(&0i32.to_le_bytes());
            info[8..12].copy_from_slice(&(pid() as i32).to_le_bytes());
            info[12..16].copy_from_slice(&1000u32.to_le_bytes());
            if copy_user_out(address, &info).is_err() {
                return neg_errno(EFAULT);
            }
            return 128;
        }
    }
    match fd.kind {
        FILE_KIND_TTY => {
            if fd.flags & O_NONBLOCK != 0 && !serial_ready() {
                return neg_errno(EAGAIN);
            }
            let byte = crate::tty::read_byte();
            if byte == 0 {
                if signal_pending_unblocked() {
                    return neg_errno(EINTR);
                }
                return 0;
            }
            if copy_user_out(address, &[byte]).is_err() {
                return neg_errno(EFAULT);
            }
            1
        }
        FILE_KIND_ZERO => {
            let zero = [0u8; 512];
            let amount = core::cmp::min(count, zero.len());
            if copy_user_out(address, &zero[..amount]).is_err() {
                return neg_errno(EFAULT);
            }
            amount as u64
        }
        FILE_KIND_NULL => 0,
        FILE_KIND_BINARY | FILE_KIND_TEXT => {
            let data = if fd.kind == FILE_KIND_BINARY {
                fd_for_binary(&fd)
            } else {
                fd_for_text(&fd)
            };
            let Some(data) = data else {
                return neg_errno(ENOENT);
            };
            let start = core::cmp::min(fd.position as usize, data.len());
            let amount = core::cmp::min(count, data.len().saturating_sub(start));
            if amount != 0 {
                if copy_user_out(address, &data[start..start + amount]).is_err() {
                    return neg_errno(EFAULT);
                }
            }
            unsafe {
                state_mut().fds[fd_number as usize].position = (start + amount) as u64;
            }
            amount as u64
        }
        FILE_KIND_BLOCK => {
            let offset = fd.position;
            let mut buf = [0u8; 512];
            let mut written = 0usize;
            let mut cur = offset;
            while written < count {
                let chunk = core::cmp::min(512, count - written);
                let r = crate::storage::vfs::with_raw(|nvme| {
                    nvme.read_range(cur, &mut buf[..chunk]).map(|_| chunk)
                });
                match r {
                    Some(Ok(n)) => {
                        if copy_user_out(address + written as u64, &buf[..n]).is_err() {
                            return neg_errno(EFAULT);
                        }
                        written += n;
                        cur += n as u64;
                    }
                    _ => break,
                }
            }
            unsafe {
                state_mut().fds[fd_number as usize].position = cur;
            }
            written as u64
        }
        _ => neg_errno(EBADF),
    }
}

fn write_fd(fd_number: i32, address: u64, count: usize) -> u64 {
    if fd_number < 0 || fd_number as usize >= FILE_FD_MAX {
        return neg_errno(EBADF);
    }
    if count == 0 {
        return 0;
    }
    let fd = match fd_snapshot(fd_number) {
        Ok(fd) => fd,
        Err(code) => return code,
    };
    if !fd_writable(&fd) && fd.kind != FILE_KIND_TTY {
        return neg_errno(EBADF);
    }
    if fd.kind == FILE_KIND_PIPE_WRITE {
        return pipe_write_fd(fd_number, address, count);
    }
    if fd.kind == FILE_KIND_SOCKET { return socket_write_fd(fd_number, address, count); }
    if fd.kind == FILE_KIND_RAMFILE { return ram_file_write(fd_number, address, count); }
    if fd.kind == FILE_KIND_EVENTFD {
        let id = fd.path[0] as usize;
        unsafe {
            if !EVENTFDS[id].used {
                return neg_errno(EBADF);
            }
            if count < 8 {
                return neg_errno(EINVAL);
            }
            let mut raw = [0u8; 8];
            if copy_user_in(address, &mut raw).is_err() {
                return neg_errno(EFAULT);
            }
            let value = u64::from_le_bytes(raw);
            if value == u64::MAX {
                return neg_errno(EINVAL);
            }
            if EVENTFDS[id].counter.checked_add(value).is_none() {
                return neg_errno(EAGAIN);
            }
            EVENTFDS[id].counter += value;
            return 8;
        }
    }

    match fd.kind {
        FILE_KIND_TTY => {
            let mut written = 0usize;
            let mut buffer = [0u8; 1024];
            while written < count {
                let amount = core::cmp::min(count - written, buffer.len());
                let current = match address.checked_add(written as u64) {
                    Some(value) => value,
                    None => {
                        return if written == 0 {
                            neg_errno(EFAULT)
                        } else {
                            written as u64
                        };
                    }
                };
                if copy_user_in(current, &mut buffer[..amount]).is_err() {
                    return if written == 0 {
                        neg_errno(EFAULT)
                    } else {
                        written as u64
                    };
                }
                crate::tty::write(&buffer[..amount]);
                written += amount;
            }
            written as u64
        }
        FILE_KIND_NULL => count as u64,
        FILE_KIND_BINARY | FILE_KIND_TEXT => neg_errno(EROFS),
        _ => neg_errno(EBADF),
    }
}

fn map_memory(address: u64, length: u64, prot: u64, flags: u64, fd: i32, offset: u64) -> u64 {
    if length == 0 {
        return neg_errno(EINVAL);
    }
    if offset & (PAGE_SIZE - 1) != 0 {
        return neg_errno(EINVAL);
    }
    let aligned = length
        .checked_add(PAGE_SIZE - 1)
        .map(|x| x & !(PAGE_SIZE - 1));
    let Some(length) = aligned else {
        return neg_errno(EINVAL);
    };
    let target = if flags & MAP_FIXED != 0 {
        address
    } else {
        let hint = address & !(PAGE_SIZE - 1);
        unsafe {
            let state = state_mut();
            let target = if hint != 0 { hint } else { state.next_mmap };
            if hint == 0 {
                state.next_mmap = state.next_mmap.checked_add(length).unwrap_or(USER_LIMIT);
            }
            target
        }
    };
    if target & (PAGE_SIZE - 1) != 0 {
        return neg_errno(EINVAL);
    }
    let end = target.checked_add(length);
    let Some(end) = end else {
        return neg_errno(ENOMEM);
    };
    if target >= USER_LIMIT || end > USER_LIMIT || end <= target {
        return neg_errno(ENOMEM);
    }
    let map_type = flags & MAP_TYPE;
    if map_type == 0 {
        return neg_errno(EINVAL);
    }
    let anonymous = flags & MAP_ANONYMOUS != 0;
    let file_data = if anonymous {
        None
    } else {
        if fd < 0 {
            return neg_errno(EBADF);
        }
        match fd_snapshot(fd) {
            Ok(descriptor) => match descriptor.kind {
                FILE_KIND_BINARY => fd_for_binary(&descriptor),
                FILE_KIND_TEXT => fd_for_text(&descriptor),
                _ => return neg_errno(ENODEV),
            },
            Err(code) => return code,
        }
    };
    map_with_runtime_allocator(target, length, prot, flags, fd, offset, file_data)
}

fn map_with_runtime_allocator(
    target: u64,
    length: u64,
    prot: u64,
    flags: u64,
    _fd: i32,
    offset: u64,
    file_data: Option<&'static [u8]>,
) -> u64 {
    crate::velf::with_process_allocator(|allocator, physical_offset| {
        let paging = crate::arch::x86_64::paging::PageTableManager::new(physical_offset);
        let fixed = flags & MAP_FIXED != 0;
        let mut actual_target = target;
        if !fixed {
            let mut candidate = target & !(PAGE_SIZE - 1);
            loop {
                let mut free = true;
                let mut page = candidate;
                while page < candidate + length {
                    if paging.translate(page).is_some() {
                        free = false;
                        break;
                    }
                    page += PAGE_SIZE;
                }
                if free {
                    actual_target = candidate;
                    break;
                }
                unsafe {
                    let state = state_mut();
                    let next = state.next_mmap;
                    if next >= USER_LIMIT {
                        return Err(neg_errno(ENOMEM));
                    }
                    state.next_mmap = next.checked_add(length).unwrap_or(USER_LIMIT);
                    candidate = next & !(PAGE_SIZE - 1);
                }
            }
        }
        let mut page = actual_target;
        let mut file_offset = offset as usize;
        while page < actual_target + length {
            if paging.translate(page).is_some() {
                if fixed {
                    let _ = paging.unmap_4k(page).map_err(|_| neg_errno(ENOMEM))?;
                } else {
                    return Err(neg_errno(ENOMEM));
                }
            }
            let frame = allocator.allocate().ok_or(neg_errno(ENOMEM))?;
            let mut page_flags = PageFlags::USER;
            if prot & PROT_WRITE != 0 {
                page_flags = page_flags.union(PageFlags::WRITABLE);
            }
            if prot & PROT_EXEC == 0 {
                page_flags = page_flags.union(PageFlags::NO_EXECUTE);
            }
            paging
                .map_4k(page, frame.addr(), page_flags, || {
                    allocator.allocate().map(|x| x.addr())
                })
                .map_err(|_| neg_errno(ENOMEM))?;
            unsafe {
                let destination = (physical_offset + frame.addr()) as *mut u8;
                core::ptr::write_bytes(destination, 0, PAGE_SIZE as usize);
                if let Some(data) = file_data {
                    if file_offset < data.len() {
                        let amount = core::cmp::min(PAGE_SIZE as usize, data.len() - file_offset);
                        core::ptr::copy_nonoverlapping(
                            data.as_ptr().add(file_offset),
                            destination,
                            amount,
                        );
                    }
                }
            }
            page += PAGE_SIZE;
            file_offset += PAGE_SIZE as usize;
        }
        Ok(actual_target)
    })
    .unwrap_or(Err(neg_errno(ENOMEM)))
    .map(|value| value)
    .unwrap_or_else(|x| x)
}

fn change_protection(address: u64, length: u64, prot: u64) -> u64 {
    if address & (PAGE_SIZE - 1) != 0 {
        return neg_errno(EINVAL);
    }
    let length = match length.checked_add(PAGE_SIZE - 1) {
        Some(value) => value & !(PAGE_SIZE - 1),
        None => return neg_errno(EINVAL),
    };
    crate::velf::with_process_allocator(|_allocator, physical_offset| {
        let paging = PageTableManager::new(physical_offset);
        let mut page = address;
        while page < address + length {
            let mut flags = PageFlags::USER;
            if prot & PROT_WRITE != 0 {
                flags = flags.union(PageFlags::WRITABLE);
            }
            if prot & PROT_EXEC == 0 {
                flags = flags.union(PageFlags::NO_EXECUTE);
            }
            paging
                .protect_4k(page, flags)
                .map_err(|_| neg_errno(ENOMEM))?;
            page += PAGE_SIZE;
        }
        Ok(())
    })
    .unwrap_or(Err(neg_errno(ENOMEM)))
    .map(|_| 0)
    .unwrap_or_else(|x| x)
}

fn unmap_memory(address: u64, length: u64) -> u64 {
    if address & (PAGE_SIZE - 1) != 0 {
        return neg_errno(EINVAL);
    }
    let length = match length.checked_add(PAGE_SIZE - 1) {
        Some(value) => value & !(PAGE_SIZE - 1),
        None => return neg_errno(EINVAL),
    };
    crate::velf::with_process_allocator(|_allocator, physical_offset| {
        let paging = PageTableManager::new(physical_offset);
        let mut page = address;
        while page < address + length {
            paging.unmap_4k(page).map_err(|_| neg_errno(EINVAL))?;
            page += PAGE_SIZE;
        }
        Ok(())
    })
    .unwrap_or(Err(neg_errno(EINVAL)))
    .map(|_| 0)
    .unwrap_or_else(|x| x)
}

fn handle_ioctl(fd: i32, request: u64, arg: u64) -> u64 {
    match request {
        TCGETS | TCSETS | TCSETSW | TCSETSF | TCGETS2 | TCSETS2 | TCSETSW2 | TCSETSF2 => {
            let is_tty = unsafe {
                let state = state_mut();
                state.init();
                fd >= 0
                    && (fd as usize) < FILE_FD_MAX
                    && state.fds[fd as usize].used
                    && state.fds[fd as usize].kind == FILE_KIND_TTY
            };
            if !is_tty {
                return neg_errno(ENOTTY);
            }
            if matches!(
                request,
                TCSETS | TCSETSW | TCSETSF | TCSETS2 | TCSETSW2 | TCSETSF2
            ) {
                let mut termios = [0u8; 44];
                if copy_user_in(arg, &mut termios).is_err() {
                    return neg_errno(EFAULT);
                }
                let lflag = u32::from_le_bytes(termios[12..16].try_into().unwrap());
                crate::tty::set_lflag(lflag);
                return 0;
            }
            let mut termios = [0u8; 44];
            termios[0..4].copy_from_slice(&0u32.to_le_bytes());
            termios[4..8].copy_from_slice(&0u32.to_le_bytes());
            termios[8..12].copy_from_slice(&0xBFu32.to_le_bytes());
            termios[12..16].copy_from_slice(&crate::tty::lflag().to_le_bytes());
            termios[16] = 0;
            termios[36..40].copy_from_slice(&38400u32.to_le_bytes());
            termios[40..44].copy_from_slice(&38400u32.to_le_bytes());
            if copy_user_out(arg, &termios).is_ok() {
                0
            } else {
                neg_errno(EFAULT)
            }
        }
        TIOCGPGRP => {
            let value = 1u32;
            if copy_user_out(arg, &value.to_le_bytes()).is_ok() {
                0
            } else {
                neg_errno(EFAULT)
            }
        }
        TIOCSPGRP => 0,
        TIOCGWINSZ => {
            let winsize = [
                25u16.to_le_bytes()[0],
                25u16.to_le_bytes()[1],
                80u16.to_le_bytes()[0],
                80u16.to_le_bytes()[1],
                0,
                0,
                0,
                0,
            ];
            if copy_user_out(arg, &winsize).is_ok() {
                0
            } else {
                neg_errno(EFAULT)
            }
        }
        FIONREAD => {
            let available = if crate::tty::input_available() {
                1u32
            } else {
                0u32
            };
            if copy_user_out(arg, &available.to_le_bytes()).is_ok() {
                0
            } else {
                neg_errno(EFAULT)
            }
        }
        TIOCGSID => {
            let sid = unsafe { state_mut().sid as u32 };
            if copy_user_out(arg, &sid.to_le_bytes()).is_ok() {
                0
            } else {
                neg_errno(EFAULT)
            }
        }
        TCFLSH => 0,
        TIOCNOTTY => 0,
        _ => neg_errno(ENOTTY),
    }
}

fn fill_uname(address: u64) -> u64 {
    let mut data = [0u8; 390];
    fn put(target: &mut [u8], offset: usize, value: &[u8]) {
        let count = core::cmp::min(
            value.len(),
            target.len().saturating_sub(offset).saturating_sub(1),
        );
        target[offset..offset + count].copy_from_slice(&value[..count]);
    }
    put(&mut data, 0, b"VibuxOS");
    put(&mut data, 65, b"vibux");
    put(&mut data, 130, b"0.1");
    put(&mut data, 195, b"#1 Vibux");
    put(&mut data, 260, b"x86_64");
    if copy_user_out(address, &data).is_ok() {
        0
    } else {
        neg_errno(EFAULT)
    }
}

fn fill_timespec(address: u64) -> u64 {
    let ticks = current_ticks();
    let seconds = ticks / 250;
    let nanos = (ticks % 250) * 4_000_000;
    let mut data = [0u8; 16];
    data[..8].copy_from_slice(&(seconds as i64).to_le_bytes());
    data[8..].copy_from_slice(&(nanos as i64).to_le_bytes());
    if copy_user_out(address, &data).is_ok() {
        0
    } else {
        neg_errno(EFAULT)
    }
}
fn fill_timeval(address: u64) -> u64 {
    let ticks = current_ticks();
    let seconds = ticks / 250;
    let micros = ((ticks % 250) * 4_000) as i64;
    let mut data = [0u8; 16];
    data[..8].copy_from_slice(&(seconds as i64).to_le_bytes());
    data[8..].copy_from_slice(&micros.to_le_bytes());
    if copy_user_out(address, &data).is_ok() {
        0
    } else {
        neg_errno(EFAULT)
    }
}

#[inline(always)]
fn fd_snapshot(fd_number: i32) -> Result<Fd, u64> {
    if fd_number < 0 || fd_number as usize >= FILE_FD_MAX {
        return Err(neg_errno(EBADF));
    }
    unsafe {
        let state = state_mut();
        state.init();
        let fd = state.fds[fd_number as usize];
        if fd.used {
            Ok(fd)
        } else {
            Err(neg_errno(EBADF))
        }
    }
}
#[inline(always)]
fn fd_is_directory(fd: &Fd) -> bool {
    if fd.kind != FILE_KIND_TEXT {
        return false;
    }
    if fd.path_len == 0 {
        return true;
    }
    is_directory_path(&fd.path[..fd.path_len])
}
#[inline(always)]
fn fd_readable(fd: &Fd) -> bool {
    (fd.flags & O_ACCMODE) != O_WRONLY
}
#[inline(always)]
fn fd_writable(fd: &Fd) -> bool {
    (fd.flags & O_ACCMODE) != O_RDONLY
}

#[inline(always)]
fn fd_duplicate(old_fd: i32, min_fd: i32, cloexec: bool) -> u64 {
    if old_fd < 0 || min_fd < 0 {
        return neg_errno(EBADF);
    }
    unsafe {
        let state = state_mut();
        state.init();
        if old_fd as usize >= FILE_FD_MAX || !state.fds[old_fd as usize].used {
            return neg_errno(EBADF);
        }
        let mut target = min_fd as usize;
        if target < 3 {
            target = 3;
        }
        while target < FILE_FD_MAX && state.fds[target].used {
            target += 1;
        }
        if target >= FILE_FD_MAX {
            return neg_errno(EMFILE);
        }
        let mut copy = state.fds[old_fd as usize];
        if cloexec {
            copy.flags |= O_CLOEXEC;
        } else {
            copy.flags &= !O_CLOEXEC;
        }
        state.fds[target] = copy;
        target as u64
    }
}
#[inline(always)]
fn fd_duplicate_exact(old_fd: i32, new_fd: i32, cloexec: bool) -> u64 {
    if old_fd < 0 || new_fd < 0 {
        return neg_errno(EBADF);
    }
    if old_fd == new_fd {
        return if cloexec {
            neg_errno(EINVAL)
        } else {
            new_fd as u64
        };
    }
    unsafe {
        let state = state_mut();
        state.init();
        if old_fd as usize >= FILE_FD_MAX || new_fd as usize >= FILE_FD_MAX {
            return neg_errno(EBADF);
        }
        if !state.fds[old_fd as usize].used {
            return neg_errno(EBADF);
        }
        state.fds[new_fd as usize] = state.fds[old_fd as usize];
        if cloexec {
            state.fds[new_fd as usize].flags |= O_CLOEXEC;
        } else {
            state.fds[new_fd as usize].flags &= !O_CLOEXEC;
        }
        new_fd as u64
    }
}

#[inline(always)]
fn path_mode(path: &[u8]) -> u32 {
    if let Some(index) = ram_file_index(path) { return unsafe { RAM_FILES[index].mode }; }
    if is_directory_path(path) || path == b"." || path == b".." {
        0o040755
    } else if is_known_binary(path) {
        0o100755
    } else {
        0o100644
    }
}
#[inline(always)]
fn path_exists(path: &[u8]) -> bool {
    ram_file_index(path).is_some()
        || is_known_binary(path)
        || text_file(path).is_some()
        || matches!(
            path,
            b"/" | b"."
                | b".."
                | b"/home"
                | b"/home/boi"
                | b"/tmp"
                | b"/etc"
                | b"/usr"
                | b"/var"
                | b"/bin"
                | b"/dev"
                | b"/proc"
                | b"/proc/self"
                | b"/proc/1"
                | b"/root"
                | b"/sys"
                | b"/run"
                | b"/dev/null"
                | b"/dev/zero"
                | b"/dev/tty"
                | b"/dev/console"
                | b"/dev/random"
                | b"/dev/urandom"
                | b"/dev/stdin"
                | b"/dev/stdout"
                | b"/dev/stderr"
                | b"/proc/self/exe"
                | b"/proc/1/exe"
        )
}
#[inline(always)]
fn access_path(path: &[u8], mode: u64) -> u64 {
    if mode & !7 != 0 {
        return neg_errno(EINVAL);
    }
    if !path_exists(path) {
        return neg_errno(ENOENT);
    }
    0
}
#[inline(always)]
fn normalize_fd_path(fd: &Fd) -> &[u8] {
    if fd.path_len == 0 {
        b"/"
    } else {
        &fd.path[..fd.path_len]
    }
}

#[inline(always)]
fn poll_fd_revents(fd_number: i32, events: u16) -> u16 {
    let Ok(fd) = fd_snapshot(fd_number) else {
        return POLLNVAL;
    };
    let mut result = 0u16;
    match fd.kind {
        FILE_KIND_TTY => {
            if events & (POLLIN | POLLRDNORM) != 0 && crate::tty::input_available() {
                result |= POLLIN | POLLRDNORM;
            }
            if events & (POLLOUT | POLLWRNORM) != 0 {
                result |= POLLOUT | POLLWRNORM;
            }
        }
        FILE_KIND_PIPE_READ => {
            let id = fd.path[0] as usize;
            if let Some(avail) = pipe_available(id) {
                if avail > 0 && events & (POLLIN | POLLRDNORM) != 0 {
                    result |= POLLIN | POLLRDNORM;
                }
                if !pipe_has_writer(id) {
                    result |= POLLHUP;
                }
            }
        }
        FILE_KIND_PIPE_WRITE => {
            let id = fd.path[0] as usize;
            if let Some(avail) = pipe_available(id) {
                if avail < PIPE_CAPACITY && events & (POLLOUT | POLLWRNORM) != 0 {
                    result |= POLLOUT | POLLWRNORM;
                }
                if !pipe_has_reader(id) {
                    result |= POLLERR;
                }
            }
        }
        FILE_KIND_EVENTFD => {
            let id = fd.path[0] as usize;
            unsafe {
                if EVENTFDS[id].used
                    && EVENTFDS[id].counter > 0
                    && events & (POLLIN | POLLRDNORM) != 0
                {
                    result |= POLLIN | POLLRDNORM;
                }
            }
            if events & (POLLOUT | POLLWRNORM) != 0 {
                result |= POLLOUT | POLLWRNORM;
            }
        }
        FILE_KIND_TIMERFD => {
            let id = fd.path[0] as usize;
            unsafe {
                if TIMERFDS[id].used {
                    let now = ticks_to_ns(current_ticks());
                    if TIMERFDS[id].armed
                        && now >= TIMERFDS[id].expires_ns
                        && events & (POLLIN | POLLRDNORM) != 0
                    {
                        result |= POLLIN | POLLRDNORM;
                    }
                }
            }
        }
        FILE_KIND_SIGNALFD => {
            let id = fd.path[0] as usize;
            unsafe {
                if SIGNALFDS[id].used {
                    let pending = PENDING_SIGNALS.load(Ordering::Acquire) & SIGNALFDS[id].mask;
                    if pending != 0 && events & (POLLIN | POLLRDNORM) != 0 {
                        result |= POLLIN | POLLRDNORM;
                    }
                }
            }
        }
        _ => {
            if events & (POLLIN | POLLRDNORM) != 0 {
                result |= POLLIN | POLLRDNORM;
            }
            if events & (POLLOUT | POLLWRNORM) != 0 && fd_writable(&fd) {
                result |= POLLOUT | POLLWRNORM;
            }
        }
    }
    result
}

#[inline(always)]
fn poll_scan(user_fds: u64, count: usize) -> Result<usize, u64> {
    if count > 256 {
        return Err(neg_errno(EINVAL));
    }
    let mut ready = 0usize;
    let mut index = 0usize;
    while index < count {
        let base = user_fds
            .checked_add((index * 8) as u64)
            .ok_or(neg_errno(EFAULT))?;
        let mut raw = [0u8; 8];
        copy_user_in(base, &mut raw)?;
        let fd = i32::from_le_bytes(raw[0..4].try_into().unwrap());
        let events = u16::from_le_bytes(raw[4..6].try_into().unwrap());
        let revents = poll_fd_revents(fd, events);
        if revents != 0 {
            ready += 1;
        }
        let revents_bytes = revents.to_le_bytes();
        copy_user_out(base + 6, &revents_bytes)?;
        index += 1;
    }
    Ok(ready)
}

#[inline(always)]
fn timeout_ticks_from_timespec(pointer: u64) -> Result<Option<u64>, u64> {
    if pointer == 0 {
        return Ok(None);
    }
    let mut raw = [0u8; 16];
    copy_user_in(pointer, &mut raw)?;
    let sec = i64::from_le_bytes(raw[0..8].try_into().unwrap());
    let nsec = i64::from_le_bytes(raw[8..16].try_into().unwrap());
    if sec < 0 || nsec < 0 || nsec >= 1_000_000_000 {
        return Err(neg_errno(EINVAL));
    }
    let sec_ticks = (sec as u64).saturating_mul(250).min(u64::MAX - 250);
    let ns_ticks = ((nsec as u64) + 3_999_999) / 4_000_000;
    Ok(Some(sec_ticks.saturating_add(ns_ticks)))
}
#[inline(always)]
fn timeout_ticks_from_timeval(pointer: u64) -> Result<Option<u64>, u64> {
    if pointer == 0 {
        return Ok(None);
    }
    let mut raw = [0u8; 16];
    copy_user_in(pointer, &mut raw)?;
    let sec = i64::from_le_bytes(raw[0..8].try_into().unwrap());
    let usec = i64::from_le_bytes(raw[8..16].try_into().unwrap());
    if sec < 0 || usec < 0 || usec >= 1_000_000 {
        return Err(neg_errno(EINVAL));
    }
    let sec_ticks = (sec as u64).saturating_mul(250);
    let usec_ticks = ((usec as u64) + 3_999) / 4_000;
    Ok(Some(sec_ticks.saturating_add(usec_ticks)))
}

fn select_impl(nfds: u64, readfds: u64, writefds: u64, exceptfds: u64, timeout: u64) -> u64 {
    let nfds = core::cmp::min(nfds as usize, 1024);
    let mut read_requested = [0u8; 128];
    let mut write_requested = [0u8; 128];
    let mut except_requested = [0u8; 128];
    if readfds != 0 && copy_user_in(readfds, &mut read_requested).is_err() {
        return neg_errno(EFAULT);
    }
    if writefds != 0 && copy_user_in(writefds, &mut write_requested).is_err() {
        return neg_errno(EFAULT);
    }
    if exceptfds != 0 && copy_user_in(exceptfds, &mut except_requested).is_err() {
        return neg_errno(EFAULT);
    }
    let start = current_ticks();
    let deadline = match timeout_ticks_from_timeval(timeout) {
        Ok(Some(value)) => Some(start.saturating_add(value)),
        Ok(None) => None,
        Err(code) => return code,
    };
    loop {
        let mut read_ready = [0u8; 128];
        let mut write_ready = [0u8; 128];
        let mut except_ready = [0u8; 128];
        let mut count = 0usize;
        let mut fd = 0usize;
        while fd < nfds {
            let byte = fd >> 3;
            let bit = 1u8 << (fd & 7);
            if read_requested[byte] & bit != 0 && poll_fd_revents(fd as i32, POLLIN) != 0 {
                read_ready[byte] |= bit;
                count += 1;
            }
            if write_requested[byte] & bit != 0 && poll_fd_revents(fd as i32, POLLOUT) != 0 {
                write_ready[byte] |= bit;
                count += 1;
            }
            if except_requested[byte] & bit != 0 && poll_fd_revents(fd as i32, POLLPRI) != 0 {
                except_ready[byte] |= bit;
                count += 1;
            }
            fd += 1;
        }
        if count != 0 {
            if readfds != 0 && copy_user_out(readfds, &read_ready).is_err() {
                return neg_errno(EFAULT);
            }
            if writefds != 0 && copy_user_out(writefds, &write_ready).is_err() {
                return neg_errno(EFAULT);
            }
            if exceptfds != 0 && copy_user_out(exceptfds, &except_ready).is_err() {
                return neg_errno(EFAULT);
            }
            return count as u64;
        }
        if let Some(deadline) = deadline {
            if current_ticks() >= deadline {
                let zero = [0u8; 128];
                if readfds != 0 && copy_user_out(readfds, &zero).is_err() {
                    return neg_errno(EFAULT);
                }
                if writefds != 0 && copy_user_out(writefds, &zero).is_err() {
                    return neg_errno(EFAULT);
                }
                if exceptfds != 0 && copy_user_out(exceptfds, &zero).is_err() {
                    return neg_errno(EFAULT);
                }
                return 0;
            }
        }
        if signal_pending_unblocked() {
            return neg_errno(EINTR);
        }
        unsafe {
            asm!("sti; hlt", options(nomem, nostack, preserves_flags));
        }
    }
}

fn pselect_impl(nfds: u64, readfds: u64, writefds: u64, exceptfds: u64, timeout: u64) -> u64 {
    let nfds = core::cmp::min(nfds as usize, 1024);
    let mut read_requested = [0u8; 128];
    let mut write_requested = [0u8; 128];
    let mut except_requested = [0u8; 128];
    if readfds != 0 && copy_user_in(readfds, &mut read_requested).is_err() {
        return neg_errno(EFAULT);
    }
    if writefds != 0 && copy_user_in(writefds, &mut write_requested).is_err() {
        return neg_errno(EFAULT);
    }
    if exceptfds != 0 && copy_user_in(exceptfds, &mut except_requested).is_err() {
        return neg_errno(EFAULT);
    }
    let start = current_ticks();
    let deadline = match timeout_ticks_from_timespec(timeout) {
        Ok(Some(value)) => Some(start.saturating_add(value)),
        Ok(None) => None,
        Err(code) => return code,
    };
    loop {
        let mut read_ready = [0u8; 128];
        let mut write_ready = [0u8; 128];
        let mut except_ready = [0u8; 128];
        let mut count = 0usize;
        let mut fd = 0usize;
        while fd < nfds {
            let byte = fd >> 3;
            let bit = 1u8 << (fd & 7);
            if read_requested[byte] & bit != 0 && poll_fd_revents(fd as i32, POLLIN) != 0 {
                read_ready[byte] |= bit;
                count += 1;
            }
            if write_requested[byte] & bit != 0 && poll_fd_revents(fd as i32, POLLOUT) != 0 {
                write_ready[byte] |= bit;
                count += 1;
            }
            fd += 1;
        }
        if count != 0 {
            if readfds != 0 && copy_user_out(readfds, &read_ready).is_err() {
                return neg_errno(EFAULT);
            }
            if writefds != 0 && copy_user_out(writefds, &write_ready).is_err() {
                return neg_errno(EFAULT);
            }
            if exceptfds != 0 && copy_user_out(exceptfds, &except_ready).is_err() {
                return neg_errno(EFAULT);
            }
            return count as u64;
        }
        if let Some(deadline) = deadline {
            if current_ticks() >= deadline {
                let zero = [0u8; 128];
                if readfds != 0 && copy_user_out(readfds, &zero).is_err() {
                    return neg_errno(EFAULT);
                }
                if writefds != 0 && copy_user_out(writefds, &zero).is_err() {
                    return neg_errno(EFAULT);
                }
                if exceptfds != 0 && copy_user_out(exceptfds, &zero).is_err() {
                    return neg_errno(EFAULT);
                }
                return 0;
            }
        }
        if signal_pending_unblocked() {
            return neg_errno(EINTR);
        }
        unsafe {
            asm!("sti; hlt", options(nomem, nostack, preserves_flags));
        }
    }
}

#[allow(dead_code)]
fn ppoll_impl(user_fds: u64, count: usize, timeout: u64) -> u64 {
    if count > 256 {
        return neg_errno(EINVAL);
    }
    let deadline = match timeout_ticks_from_timespec(timeout) {
        Ok(Some(value)) => Some(current_ticks().saturating_add(value)),
        Ok(None) => None,
        Err(code) => return code,
    };
    loop {
        match poll_scan(user_fds, count) {
            Ok(value) if value != 0 => return value as u64,
            Ok(_) => {}
            Err(code) => return code,
        }
        if let Some(deadline) = deadline {
            if current_ticks() >= deadline {
                return 0;
            }
        }
        if signal_pending_unblocked() {
            return neg_errno(EINTR);
        }
        unsafe {
            asm!("sti; hlt", options(nomem, nostack, preserves_flags));
        }
    }
}

fn futex_impl(uaddr: u64, op: u64, value: u64, timeout: u64) -> u64 {
    let command = op & 0x7f;
    let _private = (op & FUTEX_PRIVATE_FLAG) != 0;
    if uaddr == 0 {
        return neg_errno(EFAULT);
    }
    let mut word = [0u8; 4];
    if copy_user_in(uaddr, &mut word).is_err() {
        return neg_errno(EFAULT);
    }
    let observed = u32::from_le_bytes(word);
    match command {
        FUTEX_WAIT => {
            if observed != value as u32 {
                return neg_errno(EAGAIN);
            }
            let _ = timeout;
            neg_errno(EAGAIN)
        }
        FUTEX_WAKE => 0,
        _ => neg_errno(ENOSYS),
    }
}

fn fill_rlimit(address: u64, resource: u64) -> u64 {
    let (soft, hard) = match resource {
        RLIMIT_CPU => (RLIM_INFINITY, RLIM_INFINITY),
        RLIMIT_FSIZE => (RLIM_INFINITY, RLIM_INFINITY),
        RLIMIT_DATA => (RLIM_INFINITY, RLIM_INFINITY),
        RLIMIT_STACK => (8 * 1024 * 1024, 8 * 1024 * 1024),
        RLIMIT_CORE => (0, 0),
        RLIMIT_RSS => (RLIM_INFINITY, RLIM_INFINITY),
        RLIMIT_NPROC => (64, 64),
        RLIMIT_NOFILE => (FILE_FD_MAX as u64, FILE_FD_MAX as u64),
        RLIMIT_MEMLOCK => (64 * 1024, 64 * 1024),
        RLIMIT_AS => (USER_LIMIT, USER_LIMIT),
        RLIMIT_LOCKS => (RLIM_INFINITY, RLIM_INFINITY),
        RLIMIT_SIGPENDING => (64, 64),
        RLIMIT_MSGQUEUE => (819200, 819200),
        RLIMIT_NICE => (0, 0),
        RLIMIT_RTPRIO => (0, 0),
        RLIMIT_RTTIME => (RLIM_INFINITY, RLIM_INFINITY),
        _ => (RLIM_INFINITY, RLIM_INFINITY),
    };
    let mut raw = [0u8; 16];
    raw[0..8].copy_from_slice(&soft.to_le_bytes());
    raw[8..16].copy_from_slice(&hard.to_le_bytes());
    if copy_user_out(address, &raw).is_ok() {
        0
    } else {
        neg_errno(EFAULT)
    }
}

fn fill_rusage(address: u64) -> u64 {
    let mut data = [0u8; 144];
    let ticks = current_ticks();
    let sec = ticks / 250;
    let usec = ((ticks % 250) * 4000) as i64;
    data[0..8].copy_from_slice(&(sec as i64).to_le_bytes());
    data[8..16].copy_from_slice(&usec.to_le_bytes());
    if copy_user_out(address, &data).is_ok() {
        0
    } else {
        neg_errno(EFAULT)
    }
}

fn fill_sysinfo(address: u64) -> u64 {
    let mut data = [0u8; 112];
    let uptime = current_ticks() / 250;
    data[0..8].copy_from_slice(&(uptime as i64).to_le_bytes());
    let total = 512u64 * 1024 * 1024;
    let free = 128u64 * 1024 * 1024;
    data[20..24].copy_from_slice(&1u32.to_le_bytes());
    data[32..40].copy_from_slice(&total.to_le_bytes());
    data[40..48].copy_from_slice(&free.to_le_bytes());
    data[48..56].copy_from_slice(&0u64.to_le_bytes());
    data[56..64].copy_from_slice(&0u64.to_le_bytes());
    if copy_user_out(address, &data).is_ok() {
        0
    } else {
        neg_errno(EFAULT)
    }
}

fn fill_statfs(address: u64) -> u64 {
    let mut data = [0u8; 120];
    data[0..8].copy_from_slice(&0x5655_4258_u64.to_le_bytes());
    data[8..12].copy_from_slice(&4096u32.to_le_bytes());
    data[12..16].copy_from_slice(&1u32.to_le_bytes());
    data[16..24].copy_from_slice(&16384u64.to_le_bytes());
    data[24..32].copy_from_slice(&12288u64.to_le_bytes());
    data[32..40].copy_from_slice(&16384u64.to_le_bytes());
    data[40..48].copy_from_slice(&1024u64.to_le_bytes());
    data[48..52].copy_from_slice(&1000u32.to_le_bytes());
    if copy_user_out(address, &data).is_ok() {
        0
    } else {
        neg_errno(EFAULT)
    }
}

fn clock_now_ns(clock_id: u64) -> Option<u64> {
    let ticks = current_ticks();
    match clock_id {
        CLOCK_MONOTONIC | CLOCK_MONOTONIC_RAW | CLOCK_BOOTTIME | CLOCK_MONOTONIC_COARSE => {
            Some(ticks.saturating_mul(4_000_000))
        }
        CLOCK_PROCESS_CPUTIME_ID | CLOCK_THREAD_CPUTIME_ID => Some(ticks.saturating_mul(4_000_000)),
        CLOCK_REALTIME | CLOCK_REALTIME_COARSE | CLOCK_TAI => None,
        _ => None,
    }
}
fn write_timespec(address: u64, ns: u64) -> u64 {
    let sec = ns / 1_000_000_000;
    let nsec = ns % 1_000_000_000;
    let mut data = [0u8; 16];
    data[0..8].copy_from_slice(&(sec as i64).to_le_bytes());
    data[8..16].copy_from_slice(&(nsec as i64).to_le_bytes());
    if copy_user_out(address, &data).is_ok() {
        0
    } else {
        neg_errno(EFAULT)
    }
}
fn clock_gettime_impl(clock_id: u64, address: u64) -> u64 {
    if address == 0 {
        return neg_errno(EFAULT);
    }
    match clock_now_ns(clock_id) {
        Some(ns) => write_timespec(address, ns),
        None if clock_id == CLOCK_REALTIME
            || clock_id == CLOCK_REALTIME_COARSE
            || clock_id == CLOCK_TAI =>
        {
            fill_timespec(address)
        }
        None => neg_errno(EINVAL),
    }
}
fn clock_getres_impl(clock_id: u64, address: u64) -> u64 {
    match clock_id {
        CLOCK_REALTIME
        | CLOCK_MONOTONIC
        | CLOCK_MONOTONIC_RAW
        | CLOCK_BOOTTIME
        | CLOCK_PROCESS_CPUTIME_ID
        | CLOCK_THREAD_CPUTIME_ID
        | CLOCK_REALTIME_COARSE
        | CLOCK_MONOTONIC_COARSE
        | CLOCK_TAI => {
            if address == 0 {
                0
            } else {
                let mut data = [0u8; 16];
                data[0..8].copy_from_slice(&0i64.to_le_bytes());
                data[8..16].copy_from_slice(&4_000_000i64.to_le_bytes());
                if copy_user_out(address, &data).is_ok() {
                    0
                } else {
                    neg_errno(EFAULT)
                }
            }
        }
        _ => neg_errno(EINVAL),
    }
}

static RDRAND_STATE: AtomicU64 = AtomicU64::new(0);

#[inline(always)]
fn rdrand_available() -> bool {
    match RDRAND_STATE.load(Ordering::Relaxed) {
        1 => false,
        2 => true,
        _ => {
            let ok = (unsafe { core::arch::x86_64::__cpuid(1) }.ecx & (1 << 30)) != 0;
            RDRAND_STATE.store(if ok { 2 } else { 1 }, Ordering::Relaxed);
            ok
        }
    }
}

fn next_random_u64() -> u64 {
    unsafe {
        let state = state_mut();
        state.init();
        let mut value = 0u64;
        if rdrand_available() {
            let mut random = 0u64;
            let ok: u8;
            unsafe {
                core::arch::asm!("rdrand {value}", "setc {ok}", value = lateout(reg) random, ok = lateout(reg_byte) ok, options(nostack, preserves_flags));
            }
            if ok != 0 {
                value = random;
            }
        }
        if value == 0 {
            value = state.rng_state ^ current_ticks().rotate_left(17) ^ (state.pid << 32);
        }
        let mut x = value ^ state.rng_state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        state.rng_state = x;
        x
    }
}

fn getrandom_impl(address: u64, length: usize, flags: u64) -> u64 {
    if flags & !(GRND_NONBLOCK | GRND_RANDOM | GRND_INSECURE) != 0 {
        return neg_errno(EINVAL);
    }
    if length == 0 {
        return 0;
    }
    let mut written = 0usize;
    while written < length {
        let random = next_random_u64().to_ne_bytes();
        let amount = core::cmp::min(random.len(), length - written);
        if copy_user_out(address.saturating_add(written as u64), &random[..amount]).is_err() {
            return if written == 0 {
                neg_errno(EFAULT)
            } else {
                written as u64
            };
        }
        written += amount;
    }
    written as u64
}

fn signal_action_get(number: usize) -> SignalAction {
    unsafe {
        let state = state_mut();
        state.init();
        state.signal_actions[number]
    }
}
fn signal_action_set(number: usize, action: SignalAction) {
    unsafe {
        let state = state_mut();
        state.init();
        state.signal_actions[number] = action;
    }
}

fn rt_sigaction_impl(signum: u64, new_action: u64, old_action: u64, sigsetsize: usize) -> u64 {
    if signum == 0 || signum > 64 || sigsetsize != 8 || signum == SIGKILL || signum == SIGSTOP {
        return neg_errno(EINVAL);
    }
    let index = (signum - 1) as usize;
    if old_action != 0 {
        let action = signal_action_get(index);
        let mut raw = [0u8; 32];
        raw[0..8].copy_from_slice(&action.handler.to_le_bytes());
        raw[8..16].copy_from_slice(&action.flags.to_le_bytes());
        raw[16..24].copy_from_slice(&action.restorer.to_le_bytes());
        raw[24..32].copy_from_slice(&action.mask.to_le_bytes());
        if copy_user_out(old_action, &raw).is_err() {
            return neg_errno(EFAULT);
        }
    }
    if new_action != 0 {
        let mut raw = [0u8; 32];
        if copy_user_in(new_action, &mut raw).is_err() {
            return neg_errno(EFAULT);
        }
        let action = SignalAction {
            handler: u64::from_le_bytes(raw[0..8].try_into().unwrap()),
            flags: u64::from_le_bytes(raw[8..16].try_into().unwrap()),
            restorer: u64::from_le_bytes(raw[16..24].try_into().unwrap()),
            mask: u64::from_le_bytes(raw[24..32].try_into().unwrap()),
        };
        signal_action_set(index, action);
    }
    0
}

fn rt_sigprocmask_impl(how: u64, set: u64, oldset: u64, sigsetsize: usize) -> u64 {
    if sigsetsize != 8 {
        return neg_errno(EINVAL);
    }
    if oldset != 0 {
        let mask = unsafe {
            let state = state_mut();
            state.init();
            state.signal_mask
        };
        if copy_user_out(oldset, &mask.to_le_bytes()).is_err() {
            return neg_errno(EFAULT);
        }
    }
    if set != 0 {
        let mut raw = [0u8; 8];
        if copy_user_in(set, &mut raw).is_err() {
            return neg_errno(EFAULT);
        }
        let value = u64::from_le_bytes(raw) & !signal_bit(SIGKILL) & !signal_bit(SIGSTOP);
        unsafe {
            let state = state_mut();
            state.init();
            match how {
                0 => state.signal_mask |= value,
                1 => state.signal_mask &= !value,
                2 => state.signal_mask = value,
                _ => return neg_errno(EINVAL),
            }
        }
    }
    0
}

fn signal_default_ignored(signum: u64) -> bool {
    matches!(signum, SIGCHLD | SIGCONT | SIGURG | SIGWINCH)
}
#[inline]
fn signal_bit(signum: u64) -> u64 {
    1u64 << (signum - 1)
}
pub(crate) fn raise_signal(signum: u64) -> bool {
    if signum == 0 || signum > 64 {
        return false;
    }
    PENDING_SIGNALS.fetch_or(signal_bit(signum), Ordering::AcqRel);
    true
}
pub(crate) fn raise_signal_from_tty(signum: u64) {
    let _ = raise_signal(signum);
}

pub(crate) fn signal_pending_unblocked() -> bool {
    let mask = unsafe {
        let state = state_mut();
        state.init();
        state.signal_mask
    };
    let mut pending = PENDING_SIGNALS.load(Ordering::Acquire) & !mask;
    loop {
        if pending == 0 {
            return false;
        }
        let index = pending.trailing_zeros() as usize;
        let signum = index as u64 + 1;
        let action = signal_action_get(index);
        if action.handler == SIG_IGN
            || (action.handler == SIG_DFL && signal_default_ignored(signum))
        {
            let bit = signal_bit(signum);
            PENDING_SIGNALS.fetch_and(!bit, Ordering::AcqRel);
            pending &= !bit;
            continue;
        }
        return true;
    }
}

fn take_deliverable_signal() -> Option<u64> {
    loop {
        let mask = unsafe {
            let state = state_mut();
            state.init();
            state.signal_mask
        };
        let pending = PENDING_SIGNALS.load(Ordering::Acquire) & !mask;
        if pending == 0 {
            return None;
        }
        let index = pending.trailing_zeros() as usize;
        let signum = index as u64 + 1;
        let bit = signal_bit(signum);
        let action = signal_action_get(index);
        if action.handler == SIG_IGN
            || (action.handler == SIG_DFL && signal_default_ignored(signum))
        {
            PENDING_SIGNALS.fetch_and(!bit, Ordering::AcqRel);
            continue;
        }
        PENDING_SIGNALS.fetch_and(!bit, Ordering::AcqRel);
        return Some(signum);
    }
}

fn terminate_for_signal(signum: u64) {
    let code = (128u64 + signum.min(127)) as i32;
    unsafe {
        let state = state_mut();
        state.init();
        state.exit_code = code;
        state.exit_requested = true;
        core::ptr::addr_of_mut!(vibux_exit_code).write(code);
        core::ptr::addr_of_mut!(vibux_exit_requested).write(1);
    }
}

#[inline]
fn put_u64(dst: &mut [u8], offset: usize, value: u64) {
    dst[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
#[inline]
fn put_u32(dst: &mut [u8], offset: usize, value: u32) {
    dst[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
#[inline]
fn get_u64(src: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(src[offset..offset + 8].try_into().unwrap())
}

const RT_SIGFRAME_SIZE: usize = 560;
const SIGFPSTATE_SIZE: usize = 512;
const UC_MCONTEXT: usize = 48;
const UC_SIGMASK: usize = 304;
const SIGINFO_OFFSET: usize = 432;

fn set_saved_user_rsp(value: u64) {
    unsafe {
        SYSCALL_SAVED_RSP = value;
        if crate::process::vibux_vfork_child_active != 0 {
            vibux_child_syscall_saved_rsp = value;
        } else {
            vibux_syscall_saved_rsp = value;
        }
    }
}

fn build_signal_frame(
    frame: &SyscallFrame,
    signum: u64,
    action: SignalAction,
    old_mask: u64,
    old_rsp: u64,
) -> Result<u64, u64> {
    if action.restorer == 0 {
        return Err(EINVAL);
    }
    let total = RT_SIGFRAME_SIZE + SIGFPSTATE_SIZE;
    let raw = old_rsp.checked_sub((total + 128) as u64).ok_or(EFAULT)?;
    let frame_base = (raw & !15u64).checked_sub(8).ok_or(EFAULT)?;
    let end = frame_base.checked_add(total as u64).ok_or(EFAULT)?;
    if frame_base < 0x1000 || end > USER_LIMIT {
        return Err(EFAULT);
    }
    let mut data = [0u8; RT_SIGFRAME_SIZE];
    put_u64(&mut data, 0, action.restorer);
    put_u64(&mut data, 8, 0);
    put_u64(&mut data, 16, 0);
    let m = UC_MCONTEXT;
    put_u64(&mut data, m + 0, frame.r8);
    put_u64(&mut data, m + 8, frame.r9);
    put_u64(&mut data, m + 16, frame.r10);
    put_u64(&mut data, m + 24, frame.syscall_r11);
    put_u64(&mut data, m + 32, frame.r12);
    put_u64(&mut data, m + 40, frame.r13);
    put_u64(&mut data, m + 48, frame.r14);
    put_u64(&mut data, m + 56, frame.r15);
    put_u64(&mut data, m + 64, frame.rdi);
    put_u64(&mut data, m + 72, frame.rsi);
    put_u64(&mut data, m + 80, frame.rbp);
    put_u64(&mut data, m + 88, frame.rbx);
    put_u64(&mut data, m + 96, frame.rdx);
    put_u64(&mut data, m + 104, frame.rax);
    put_u64(&mut data, m + 112, frame.syscall_rcx);
    put_u64(&mut data, m + 120, old_rsp);
    put_u64(&mut data, m + 128, frame.syscall_rcx);
    put_u64(&mut data, m + 136, frame.syscall_r11);
    put_u64(&mut data, m + 144, USER_CS | (USER_SS << 48));
    put_u64(&mut data, m + 152, 0);
    put_u64(&mut data, m + 160, 0);
    put_u64(&mut data, m + 168, old_mask);
    put_u64(&mut data, m + 176, 0);
    put_u64(&mut data, m + 184, frame_base + RT_SIGFRAME_SIZE as u64);
    put_u64(&mut data, m + 192, fs_base());
    put_u64(&mut data, UC_SIGMASK, old_mask);
    put_u32(&mut data, SIGINFO_OFFSET, signum as u32);
    put_u32(&mut data, SIGINFO_OFFSET + 4, 0);
    put_u32(&mut data, SIGINFO_OFFSET + 8, 0);
    put_u32(&mut data, SIGINFO_OFFSET + 16, pid() as u32);
    put_u32(&mut data, SIGINFO_OFFSET + 20, 1000);
    if copy_user_out(frame_base, &data).is_err() {
        return Err(EFAULT);
    }
    let fpstate = [0u8; SIGFPSTATE_SIZE];
    if copy_user_out(frame_base + RT_SIGFRAME_SIZE as u64, &fpstate).is_err() {
        return Err(EFAULT);
    }
    Ok(frame_base)
}

fn rt_sigreturn_impl(frame: &mut SyscallFrame) -> u64 {
    let user_rsp = saved_user_rsp();
    let Some(frame_base) = user_rsp.checked_sub(8) else {
        return neg_errno(EFAULT);
    };
    let end = match frame_base.checked_add(RT_SIGFRAME_SIZE as u64) {
        Some(value) => value,
        None => return neg_errno(EFAULT),
    };
    if frame_base < 0x1000 || end > USER_LIMIT {
        return neg_errno(EFAULT);
    }
    let mut data = [0u8; RT_SIGFRAME_SIZE];
    if copy_user_in(frame_base, &mut data).is_err() {
        return neg_errno(EFAULT);
    }
    let m = UC_MCONTEXT;
    let r8 = get_u64(&data, m + 0);
    let r9 = get_u64(&data, m + 8);
    let r10 = get_u64(&data, m + 16);
    let r11 = get_u64(&data, m + 24);
    let r12 = get_u64(&data, m + 32);
    let r13 = get_u64(&data, m + 40);
    let r14 = get_u64(&data, m + 48);
    let r15 = get_u64(&data, m + 56);
    let rdi = get_u64(&data, m + 64);
    let rsi = get_u64(&data, m + 72);
    let rbp = get_u64(&data, m + 80);
    let rbx = get_u64(&data, m + 88);
    let rdx = get_u64(&data, m + 96);
    let rax = get_u64(&data, m + 104);
    let _rcx = get_u64(&data, m + 112);
    let rsp = get_u64(&data, m + 120);
    let rip = get_u64(&data, m + 128);
    let mut rflags = get_u64(&data, m + 136);
    let csgsfs = get_u64(&data, m + 144);
    let fs = get_u64(&data, m + 192);
    let saved_mask = get_u64(&data, UC_SIGMASK);
    let restored_cs = csgsfs & 0xFFFF;
    let restored_ss = (csgsfs >> 48) & 0xFFFF;
    if restored_cs != USER_CS
        || restored_ss != USER_SS
        || rip == 0
        || rsp == 0
        || rip >= USER_LIMIT
        || rsp >= USER_LIMIT
        || (rflags & (1u64 << 13)) != 0
        || (rflags & (1u64 << 14)) != 0
        || (rflags & (1u64 << 17)) != 0
    {
        return neg_errno(EFAULT);
    }
    rflags |= RFLAGS_USER;
    frame.r8 = r8;
    frame.r9 = r9;
    frame.r10 = r10;
    frame.syscall_r11 = rflags;
    frame.r12 = r12;
    frame.r13 = r13;
    frame.r14 = r14;
    frame.r15 = r15;
    frame.rdi = rdi;
    frame.rsi = rsi;
    frame.rbp = rbp;
    frame.rbx = rbx;
    frame.rdx = rdx;
    frame.rax = rax;
    frame.syscall_rcx = rip;
    set_saved_user_rsp(rsp);
    set_fs_base(fs);
    unsafe {
        state_mut().signal_mask = saved_mask;
        state_mut().pending_signals = PENDING_SIGNALS.load(Ordering::Acquire);
    }
    rax
}

fn deliver_pending_signal(frame: &mut SyscallFrame) {
    let Some(signum) = take_deliverable_signal() else {
        return;
    };
    let index = (signum - 1) as usize;
    let action = signal_action_get(index);
    if action.handler == SIG_DFL {
        terminate_for_signal(signum);
        return;
    }
    if action.handler == SIG_IGN {
        return;
    }
    let old_mask = unsafe {
        let state = state_mut();
        state.init();
        state.signal_mask
    };
    let old_rsp = unsafe {
        if crate::process::vibux_vfork_child_active != 0 {
            child_saved_user_rsp()
        } else {
            saved_user_rsp()
        }
    };
    let frame_base = match build_signal_frame(frame, signum, action, old_mask, old_rsp) {
        Ok(value) => value,
        Err(_) => {
            terminate_for_signal(SIGSEGV);
            return;
        }
    };
    let mut new_mask = old_mask | action.mask;
    if action.flags & SA_NODEFER == 0 {
        new_mask |= signal_bit(signum);
    }
    new_mask &= !signal_bit(SIGKILL);
    new_mask &= !signal_bit(SIGSTOP);
    unsafe {
        state_mut().signal_mask = new_mask;
        if action.flags & SA_RESETHAND != 0 {
            state_mut().signal_actions[index] = SignalAction::empty();
        }
    }
    frame.rdi = signum;
    frame.rsi = frame_base + SIGINFO_OFFSET as u64;
    frame.rdx = frame_base + 8;
    frame.syscall_rcx = action.handler;
    frame.syscall_r11 = RFLAGS_USER;
    set_saved_user_rsp(frame_base);
}

fn setgroups_impl(size: usize, list: u64) -> u64 {
    if size == 0 {
        return 0;
    }
    if size > 1 {
        return neg_errno(EPERM);
    }
    let mut gid = [0u8; 4];
    if copy_user_in(list, &mut gid).is_err() {
        return neg_errno(EFAULT);
    }
    if u32::from_le_bytes(gid) != unsafe { state_mut().gid } {
        return neg_errno(EPERM);
    }
    0
}
fn getgroups_impl(size: usize, list: u64) -> u64 {
    if size == 0 {
        return 1;
    }
    if size < 1 || list == 0 {
        return neg_errno(EINVAL);
    }
    let gid = unsafe { state_mut().gid };
    if copy_user_out(list, &gid.to_le_bytes()).is_err() {
        return neg_errno(EFAULT);
    }
    1
}

fn fill_getdents64(fd_number: i32, address: u64, count: usize) -> u64 {
    if count < 24 {
        return neg_errno(EINVAL);
    }
    let fd = match fd_snapshot(fd_number) {
        Ok(fd) => fd,
        Err(code) => return code,
    };
    if !fd_is_directory(&fd) {
        return neg_errno(ENOTDIR);
    }
    let path = normalize_fd_path(&fd);
    let names: &[&[u8]] = match path {
        b"/" => &[
            b".", b"..", b"bin", b"dev", b"etc", b"home", b"proc", b"tmp", b"usr", b"var", b"root",
            b"sys", b"run",
        ],
        b"/home" => &[b".", b"..", b"boi"],
        b"/home/boi" => &[b".", b"..", b"README"],
        b"/etc" => &[
            b".",
            b"..",
            b"passwd",
            b"group",
            b"hostname",
            b"profile",
            b"bash.bashrc",
            b"bashrc",
            b"mtab",
            b"fstab",
            b"resolv.conf",
            b"hosts",
        ],
        _ => &[b".", b".."],
    };
    let index = unsafe { state_mut().fds[fd_number as usize].position as usize };
    if index >= names.len() {
        return 0;
    }
    let mut written = 0usize;
    let mut current = index;
    while current < names.len() {
        let name = names[current];
        let reclen = (19 + name.len() + 1 + 7) & !7;
        if written + reclen > count {
            break;
        }
        let mut entry = [0u8; 280];
        let ino = file_identity(name).1;
        entry[0..8].copy_from_slice(&ino.to_le_bytes());
        entry[8..16].copy_from_slice(&((current + 1) as i64).to_le_bytes());
        entry[16..18].copy_from_slice(&(reclen as u16).to_le_bytes());
        let dtype = if name == b"." || name == b".." {
            4u8
        } else if path == b"/home/boi" && name == b"README" {
            8u8
        } else {
            4u8
        };
        entry[18] = dtype;
        entry[19..19 + name.len()].copy_from_slice(name);
        entry[19 + name.len()] = 0;
        if copy_user_out(address + written as u64, &entry[..reclen]).is_err() {
            return if written == 0 {
                neg_errno(EFAULT)
            } else {
                written as u64
            };
        }
        written += reclen;
        current += 1;
    }
    unsafe {
        state_mut().fds[fd_number as usize].position = current as u64;
    }
    written as u64
}

fn fill_statx(address: u64, path: &[u8], mode: u32, size: u64) -> u64 {
    let mut statx = [0u8; 256];
    statx[0..4].copy_from_slice(&STATX_BASIC_STATS.to_le_bytes());
    statx[4..8].copy_from_slice(&4096u32.to_le_bytes());
    statx[16..20].copy_from_slice(&1u32.to_le_bytes());
    statx[20..24].copy_from_slice(&1000u32.to_le_bytes());
    statx[24..28].copy_from_slice(&1000u32.to_le_bytes());
    let mode16 = (mode & 0xFFFF) as u16;
    statx[28..30].copy_from_slice(&mode16.to_le_bytes());
    let ino = file_identity(path).1;
    statx[32..40].copy_from_slice(&ino.to_le_bytes());
    statx[40..48].copy_from_slice(&size.to_le_bytes());
    statx[48..56].copy_from_slice(&((size + 511) / 512).to_le_bytes());
    if copy_user_out(address, &statx).is_ok() {
        0
    } else {
        neg_errno(EFAULT)
    }
}

fn chmod_like(fd: i32, mode: u64, path_mode_flag: bool, path: &[u8]) -> u64 {
    if mode & !0o7777 != 0 {
        return neg_errno(EINVAL);
    }
    if path_mode_flag {
        if path_exists(path) {
            0
        } else {
            neg_errno(ENOENT)
        }
    } else {
        match fd_snapshot(fd) {
            Ok(_) => 0,
            Err(code) => code,
        }
    }
}

macro_rules! return_frame_result {
    ($frame:expr, $result:expr) => {{
        $frame.rax = $result;
        return;
    }};
}

fn socket_alloc(domain: u64, kind: u64, full_flags: u64) -> u64 {
    let base = kind & 0xF;
    if !matches!(domain, 1 | 2 | 10) || !matches!(base, 1 | 2 | 3) { return neg_errno(EAFNOSUPPORT); }
    unsafe { state_mut().alloc_fd(Fd { used: true, kind: FILE_KIND_SOCKET, flags: O_RDWR | (full_flags & (O_NONBLOCK | O_CLOEXEC)), position: 0, path: [0;160], path_len: 0 }).map(|fd| fd as u64).unwrap_or(neg_errno(EMFILE)) }
}

fn splice_impl(fd_in: i32, off_in: u64, fd_out: i32, off_out: u64, len: usize, flags: u64) -> u64 {
    if fd_snapshot(fd_in).is_err() {
        return neg_errno(EBADF);
    }
    if fd_snapshot(fd_out).is_err() {
        return neg_errno(EBADF);
    }
    if flags & !(SPLICE_F_MOVE | SPLICE_F_NONBLOCK | SPLICE_F_MORE | SPLICE_F_GIFT) != 0 {
        return neg_errno(EINVAL);
    }
    let mut buf = [0u8; 4096];
    let mut total = 0usize;
    while total < len {
        let chunk = core::cmp::min(4096, len - total);
        let r = read_fd(fd_in, buf.as_ptr() as u64, chunk);
        if (r as i64) < 0 {
            return if total == 0 { r } else { total as u64 };
        }
        if r == 0 {
            break;
        }
        let w = write_fd(fd_out, buf.as_ptr() as u64, r as usize);
        if (w as i64) < 0 {
            return if total == 0 { w } else { total as u64 };
        }
        total += r as usize;
    }
    let _ = (off_in, off_out);
    total as u64
}

fn tee_impl(fd_in: i32, fd_out: i32, len: usize, flags: u64) -> u64 {
    let _ = (fd_in, fd_out, len, flags);
    neg_errno(EINVAL)
}
fn vmsplice_impl(fd: i32, iov: u64, nr: usize, flags: u64) -> u64 {
    let _ = (fd, iov, nr, flags);
    neg_errno(ENOSYS)
}

fn memfd_create_impl(name_ptr: u64, flags: u64) -> u64 {
    let mut name = [0u8; 160];
    if read_c_string(name_ptr, &mut name).is_err() {
        return neg_errno(EFAULT);
    }
    if flags & !(1 | 2 | 4) != 0 {
        return neg_errno(EINVAL);
    }
    unsafe {
        let state = state_mut();
        state.init();
        let mut stored = [0u8; 160];
        let n = core::cmp::min(name.iter().position(|&b| b == 0).unwrap_or(0), 160);
        stored[..n].copy_from_slice(&name[..n]);
        state
            .alloc_fd(Fd {
                used: true,
                kind: FILE_KIND_ZERO,
                flags: O_RDWR,
                position: 0,
                path: stored,
                path_len: n,
            })
            .map(|x| x as u64)
            .unwrap_or(neg_errno(EMFILE))
    }
}

fn pidfd_impl(target_pid: u64) -> u64 {
    unsafe {
        let state = state_mut();
        state.init();
        let mut stored = [0u8; 160];
        stored[0] = target_pid as u8;
        state
            .alloc_fd(Fd {
                used: true,
                kind: FILE_KIND_NULL,
                flags: O_RDONLY | O_CLOEXEC,
                position: 0,
                path: stored,
                path_len: 1,
            })
            .map(|x| x as u64)
            .unwrap_or(neg_errno(EMFILE))
    }
}

fn close_range_impl(first: u32, last: u32, flags: u64) -> u64 {
    if flags & !(1 | 2) != 0 {
        return neg_errno(EINVAL);
    }
    if first > last {
        return neg_errno(EINVAL);
    }
    unsafe {
        let state = state_mut();
        state.init();
        let hi = core::cmp::min(last, (FILE_FD_MAX as u32) - 1);
        for fd in first..=hi {
            if fd < 3 {
                continue;
            }
            state.fds[fd as usize] = Fd::empty();
        }
    }
    0
}

fn openat2_impl(_dirfd: i32, path_ptr: u64, how_ptr: u64) -> u64 {
    let mut path = [0u8; 160];
    if read_c_string(path_ptr, &mut path).is_err() {
        return neg_errno(EFAULT);
    }
    let mut how = [0u8; 24];
    if how_ptr != 0 && copy_user_in(how_ptr, &mut how).is_err() {
        return neg_errno(EFAULT);
    }
    let flags = if how_ptr != 0 {
        u64::from_le_bytes(how[0..8].try_into().unwrap())
    } else {
        0
    };
    let len = path.iter().position(|&b| b == 0).unwrap_or(path.len());
    open_path(&path[..len], flags)
}

fn execveat_impl(dirfd: i32, path_ptr: u64, argv: u64, envp: u64, flags: u64) -> u64 {
    if flags & !(0x1000) != 0 {
        return neg_errno(EINVAL);
    }
    let _ = dirfd;
    crate::process::execve(path_ptr, argv, envp)
}

fn copy_file_range_impl(
    fd_in: i32,
    off_in: u64,
    fd_out: i32,
    off_out: u64,
    len: usize,
    flags: u64,
) -> u64 {
    if flags != 0 {
        return neg_errno(EINVAL);
    }
    let _ = (off_in, off_out);
    if fd_snapshot(fd_in).is_err() {
        return neg_errno(EBADF);
    }
    if fd_snapshot(fd_out).is_err() {
        return neg_errno(EBADF);
    }
    let mut buf = [0u8; 4096];
    let mut total = 0usize;
    while total < len {
        let chunk = core::cmp::min(4096, len - total);
        let r = read_fd(fd_in, buf.as_ptr() as u64, chunk);
        if (r as i64) < 0 {
            return if total == 0 { r } else { total as u64 };
        }
        if r == 0 {
            break;
        }
        let w = write_fd(fd_out, buf.as_ptr() as u64, r as usize);
        if (w as i64) < 0 {
            return if total == 0 { w } else { total as u64 };
        }
        total += r as usize;
    }
    total as u64
}

fn process_vm_impl(
    pid_arg: i32,
    local_iov: u64,
    liovcnt: usize,
    remote_iov: u64,
    riovcnt: u64,
    _flags: usize,
    _read: bool,
) -> u64 {
    let _ = (local_iov, liovcnt, remote_iov, riovcnt);
    let self_pid = unsafe { state_mut().pid } as i32;
    if pid_arg != 0 && pid_arg != self_pid {
        return neg_errno(ESRCH);
    }
    0
}

fn futex_waitv_impl(waiters: u64, nr_futexes: u64, flags: u64, timeout: u64, clockid: u64) -> u64 {
    let _ = (waiters, flags, timeout, clockid);
    if nr_futexes == 0 || nr_futexes > 128 {
        return neg_errno(EINVAL);
    }
    neg_errno(EAGAIN)
}

fn timerfd_settime_impl(fd: i32, flags: u64, new_value: u64, old_value: u64) -> u64 {
    let Ok(descriptor) = fd_snapshot(fd) else {
        return neg_errno(EBADF);
    };
    if descriptor.kind != FILE_KIND_TIMERFD {
        return neg_errno(EINVAL);
    }
    let id = descriptor.path[0] as usize;
    if new_value == 0 {
        unsafe {
            if TIMERFDS[id].used {
                TIMERFDS[id].armed = false;
            }
        }
        return 0;
    }
    let mut raw = [0u8; 32];
    if copy_user_in(new_value, &mut raw).is_err() {
        return neg_errno(EFAULT);
    }
    let sec = i64::from_le_bytes(raw[0..8].try_into().unwrap());
    let nsec = i64::from_le_bytes(raw[8..16].try_into().unwrap());
    let isec = i64::from_le_bytes(raw[16..24].try_into().unwrap());
    let insec = i64::from_le_bytes(raw[24..32].try_into().unwrap());
    if sec < 0
        || nsec < 0
        || nsec >= 1_000_000_000
        || isec < 0
        || insec < 0
        || insec >= 1_000_000_000
    {
        return neg_errno(EINVAL);
    }
    let expires_ns = (sec as u64).saturating_mul(1_000_000_000) + nsec as u64;
    let interval_ns = (isec as u64).saturating_mul(1_000_000_000) + insec as u64;
    unsafe {
        if old_value != 0 {
            let mut old = [0u8; 32];
            old[0..8]
                .copy_from_slice(&((TIMERFDS[id].expires_ns / 1_000_000_000) as i64).to_le_bytes());
            old[8..16]
                .copy_from_slice(&((TIMERFDS[id].expires_ns % 1_000_000_000) as i64).to_le_bytes());
            old[16..24].copy_from_slice(
                &((TIMERFDS[id].interval_ns / 1_000_000_000) as i64).to_le_bytes(),
            );
            old[24..32].copy_from_slice(
                &((TIMERFDS[id].interval_ns % 1_000_000_000) as i64).to_le_bytes(),
            );
            if copy_user_out(old_value, &old).is_err() {
                return neg_errno(EFAULT);
            }
        }
        let now = ticks_to_ns(current_ticks());
        TIMERFDS[id].absolute = flags & TFD_TIMER_ABSTIME != 0;
        TIMERFDS[id].expires_ns = if TIMERFDS[id].absolute {
            expires_ns
        } else {
            now + expires_ns
        };
        TIMERFDS[id].interval_ns = interval_ns;
        TIMERFDS[id].armed = true;
    }
    0
}

fn timerfd_gettime_impl(fd: i32, curr_value: u64) -> u64 {
    let Ok(descriptor) = fd_snapshot(fd) else {
        return neg_errno(EBADF);
    };
    if descriptor.kind != FILE_KIND_TIMERFD {
        return neg_errno(EINVAL);
    }
    let id = descriptor.path[0] as usize;
    if curr_value == 0 {
        return 0;
    }
    let (expires, interval) = unsafe {
        if !TIMERFDS[id].used {
            return neg_errno(EBADF);
        }
        if !TIMERFDS[id].armed {
            (0u64, TIMERFDS[id].interval_ns)
        } else {
            let now = ticks_to_ns(current_ticks());
            let remaining = TIMERFDS[id].expires_ns.saturating_sub(now);
            (remaining, TIMERFDS[id].interval_ns)
        }
    };
    let mut raw = [0u8; 32];
    raw[0..8].copy_from_slice(&((expires / 1_000_000_000) as i64).to_le_bytes());
    raw[8..16].copy_from_slice(&((expires % 1_000_000_000) as i64).to_le_bytes());
    raw[16..24].copy_from_slice(&((interval / 1_000_000_000) as i64).to_le_bytes());
    raw[24..32].copy_from_slice(&((interval % 1_000_000_000) as i64).to_le_bytes());
    if copy_user_out(curr_value, &raw).is_ok() {
        0
    } else {
        neg_errno(EFAULT)
    }
}

fn clone3_impl(args_ptr: u64) -> u64 {
    if args_ptr == 0 {
        return neg_errno(EFAULT);
    }
    let mut raw = [0u8; 88];
    if copy_user_in(args_ptr, &mut raw).is_err() {
        return neg_errno(EFAULT);
    }
    let child_stack = u64::from_le_bytes(raw[8..16].try_into().unwrap());
    let flags = u64::from_le_bytes(raw[0..8].try_into().unwrap());

    unsafe {
        if flags & 0x0000_0100 != 0 {
            crate::process::do_vfork(core::ptr::null_mut(), flags, child_stack)
        } else {
            crate::process::do_fork(core::ptr::null_mut(), flags, child_stack)
        }
    }
}

fn epoll_wait_impl(epfd: i32, events_addr: u64, maxevents: i32, timeout_ms: i32) -> u64 {
    if maxevents <= 0 || maxevents > 4096 {
        return neg_errno(EINVAL);
    }
    let epfd_fd = match fd_snapshot(epfd) {
        Ok(fd) => fd,
        Err(code) => return code,
    };
    if epfd_fd.kind != FILE_KIND_EPOLL {
        return neg_errno(EINVAL);
    }
    let id = epfd_fd.path[0] as usize;
    let deadline = if timeout_ms < 0 {
        None
    } else {
        Some(current_ticks().saturating_add((timeout_ms as u64 + 3) / 4))
    };
    loop {
        unsafe {
            if !EPOLLS[id].used {
                return neg_errno(EBADF);
            }
            let count = EPOLLS[id].count;
            let mut written = 0i32;
            for i in 0..count {
                if written >= maxevents {
                    break;
                }
                let target_fd = EPOLLS[id].watched_fds[i];
                let revents = poll_fd_revents(target_fd, EPOLLS[id].watched_events[i] as u16);
                if revents == 0 {
                    continue;
                }
                let mut ev = [0u8; 12];
                ev[0..4].copy_from_slice(&(revents as u32).to_le_bytes());
                ev[4..12].copy_from_slice(&EPOLLS[id].watched_data[i].to_le_bytes());
                if copy_user_out(events_addr + (written as u64) * 12, &ev).is_err() {
                    return if written == 0 {
                        neg_errno(EFAULT)
                    } else {
                        written as u64
                    };
                }
                written += 1;
            }
            if written != 0 {
                return written as u64;
            }
        }
        if let Some(deadline) = deadline {
            if current_ticks() >= deadline {
                return 0;
            }
        }
        if signal_pending_unblocked() {
            return neg_errno(EINTR);
        }
        unsafe {
            asm!("sti; hlt", options(nomem, nostack, preserves_flags));
        }
    }
}

fn epoll_ctl_impl(epfd: i32, op: u64, target_fd: i32, event_addr: u64) -> u64 {
    let epfd_fd = match fd_snapshot(epfd) {
        Ok(fd) => fd,
        Err(code) => return code,
    };
    if epfd_fd.kind != FILE_KIND_EPOLL {
        return neg_errno(EINVAL);
    }
    let id = epfd_fd.path[0] as usize;
    if target_fd == epfd {
        return neg_errno(EINVAL);
    }
    if fd_snapshot(target_fd).is_err() {
        return neg_errno(EBADF);
    }
    let mut events = 0u32;
    let mut data = 0u64;
    if op == EPOLL_CTL_ADD || op == EPOLL_CTL_MOD {
        if event_addr == 0 {
            return neg_errno(EFAULT);
        }
        let mut raw = [0u8; 12];
        if copy_user_in(event_addr, &mut raw).is_err() {
            return neg_errno(EFAULT);
        }
        events = u32::from_le_bytes(raw[0..4].try_into().unwrap());
        data = u64::from_le_bytes(raw[4..12].try_into().unwrap());
    }
    unsafe {
        if !EPOLLS[id].used {
            return neg_errno(EBADF);
        }
        match op {
            EPOLL_CTL_ADD => {
                if EPOLLS[id].count >= EPOLL_EVENTS_MAX {
                    return neg_errno(ENOSPC);
                }
                for i in 0..EPOLLS[id].count {
                    if EPOLLS[id].watched_fds[i] == target_fd {
                        return neg_errno(EEXIST);
                    }
                }
                let idx = EPOLLS[id].count;
                EPOLLS[id].watched_fds[idx] = target_fd;
                EPOLLS[id].watched_events[idx] = events;
                EPOLLS[id].watched_data[idx] = data;
                EPOLLS[id].count += 1;
                0
            }
            EPOLL_CTL_DEL => {
                for i in 0..EPOLLS[id].count {
                    if EPOLLS[id].watched_fds[i] == target_fd {
                        for j in i..EPOLLS[id].count - 1 {
                            EPOLLS[id].watched_fds[j] = EPOLLS[id].watched_fds[j + 1];
                            EPOLLS[id].watched_events[j] = EPOLLS[id].watched_events[j + 1];
                            EPOLLS[id].watched_data[j] = EPOLLS[id].watched_data[j + 1];
                        }
                        EPOLLS[id].count -= 1;
                        return 0;
                    }
                }
                neg_errno(ENOENT)
            }
            EPOLL_CTL_MOD => {
                for i in 0..EPOLLS[id].count {
                    if EPOLLS[id].watched_fds[i] == target_fd {
                        EPOLLS[id].watched_events[i] = events;
                        EPOLLS[id].watched_data[i] = data;
                        return 0;
                    }
                }
                neg_errno(ENOENT)
            }
            _ => neg_errno(EINVAL),
        }
    }
}

#[inline(always)]
pub fn dump_recent_syscalls() {}

#[inline(always)]
pub(crate) fn debug_user_context(_: &[u8], _: u64, _: u64, _: u64) {}

#[inline(always)]
unsafe fn read_cr4_debug() -> u64 {
    let value: u64;
    unsafe {
        core::arch::asm!("mov {}, cr4", out(reg) value, options(nostack, preserves_flags));
    }
    value
}
static USERSPACE_SIMD_READY: AtomicBool = AtomicBool::new(false);

#[inline(never)]
fn enable_userspace_simd() {
    if USERSPACE_SIMD_READY.load(Ordering::Acquire) {
        return;
    }
    let info = unsafe { core::arch::x86_64::__cpuid(1) };
    const XSAVE: u32 = 1 << 26;
    const OSXSAVE: u32 = 1 << 27;
    const AVX: u32 = 1 << 28;
    if info.ecx & (XSAVE | OSXSAVE | AVX) != (XSAVE | OSXSAVE | AVX) {
        USERSPACE_SIMD_READY.store(true, Ordering::Release);
        return;
    }
    unsafe {
        let cr4 = read_cr4_debug() | (1 << 9) | (1 << 18);
        core::arch::asm!("mov cr4, {}", in(reg) cr4, options(nostack, preserves_flags));
        core::arch::asm!(
            "xsetbv",
            in("ecx") 0u32,
            in("eax") 0x7u32,
            in("edx") 0u32,
            options(nostack, preserves_flags)
        );
    }
    USERSPACE_SIMD_READY.store(true, Ordering::Release);
}

#[inline(always)]
fn align_initial_user_stack(stack: u64) -> u64 {
    let misalignment = stack & 0xF;
    if misalignment == 0 {
        return stack;
    }
    let argc = unsafe { core::ptr::read_volatile(stack as *const u64) };
    if argc > 4096 {
        return stack;
    }
    let mut cursor = stack + 8;
    for _ in 0..=argc {
        cursor = cursor.wrapping_add(8);
        if cursor.wrapping_sub(stack) > 0x10000 {
            return stack;
        }
    }
    let mut env_count = 0usize;
    loop {
        if env_count >= 4096 {
            return stack;
        }
        let value = unsafe { core::ptr::read_volatile(cursor as *const u64) };
        cursor = cursor.wrapping_add(8);
        if value == 0 {
            break;
        }
        env_count += 1;
    }
    let mut aux_count = 0usize;
    loop {
        if aux_count >= 256 {
            return stack;
        }
        let kind = unsafe { core::ptr::read_volatile(cursor as *const u64) };
        let _value = unsafe { core::ptr::read_volatile(cursor.wrapping_add(8) as *const u64) };
        cursor = cursor.wrapping_add(16);
        aux_count += 1;
        if kind == 0 {
            break;
        }
    }
    let metadata_end = cursor;
    let metadata_len = metadata_end.wrapping_sub(stack);
    if metadata_len == 0 || metadata_len > 0x10000 {
        return stack;
    }
    let aligned = stack & !0xF;
    unsafe {
        core::ptr::copy(
            stack as *const u8,
            aligned as *mut u8,
            metadata_len as usize,
        );
    }
    aligned
}

pub(crate) unsafe fn exec_failure_fast(code: u64) -> ! {
    vibux_exec_failure_fast(code)
}
pub(crate) fn enter_vfork_child(frame: *mut SyscallFrame, stack: u64) -> i32 {
    unsafe { vibux_enter_vfork_child(frame, stack) }
}
pub(crate) unsafe fn enter_vfork_exec(entry: u64, stack: u64) -> ! {
    vibux_enter_vfork_exec(entry, stack)
}
unsafe extern "C" {
    fn vibux_enter_vfork_child(frame: *mut SyscallFrame, stack: u64) -> i32;
    fn vibux_enter_vfork_exec(entry: u64, stack: u64) -> !;
    fn vibux_exec_failure_fast(code: u64) -> !;
}

pub fn enter_user(entry: u64, stack: u64) -> i32 {
    let stack = align_initial_user_stack(stack);
    enable_userspace_simd();
    unsafe {
        let state = state_mut();
        state.exit_requested = false;
        state.exit_code = 0;
        core::ptr::addr_of_mut!(vibux_exit_requested).write(0);
        core::ptr::addr_of_mut!(vibux_exit_code).write(0);
        let tls_base = crate::velf::with_process_allocator(|alloc, phys_off| {
            use crate::arch::x86_64::paging::{PageFlags as P, PageTableManager};
            let tls_virt: u64 = 0x0000_5FFF_0000_0000;
            let paging = PageTableManager::new(phys_off);
            if paging.translate(tls_virt).is_none() {
                let frame = alloc.allocate().expect("VELF TLS frame");
                paging
                    .map_4k(
                        tls_virt,
                        frame.addr(),
                        P::USER.union(P::WRITABLE).union(P::NO_EXECUTE),
                        || alloc.allocate().map(|f| f.addr()),
                    )
                    .expect("VELF TLS map");
                unsafe {
                    core::ptr::write_bytes((phys_off + frame.addr()) as *mut u8, 0, 4096);
                    core::ptr::write_unaligned((phys_off + frame.addr()) as *mut u64, tls_virt);
                }
            }
            tls_virt
        })
        .expect("VELF process allocator not installed");
        core::arch::asm!("wrmsr", in("ecx") IA32_FS_BASE, in("eax") tls_base as u32, in("edx") (tls_base >> 32) as u32, options(nostack, preserves_flags));
        state.fs_base = tls_base;
        asm!("mov rdi, {entry}", "mov rsi, {stack}", "call vibux_enter_user", entry = in(reg) entry, stack = in(reg) stack, clobber_abi("C"));
        state.exit_code
    }
}

pub fn init() {
    unsafe {
        let efer = rdmsr(IA32_EFER) | EFER_SCE;
        wrmsr(IA32_EFER, efer);
        let star = (((crate::arch::x86_64::gdt::USER_CODE_SELECTOR - 0x10) as u64) << 48)
            | ((crate::arch::x86_64::gdt::KERNEL_CODE_SELECTOR as u64) << 32);
        wrmsr(IA32_STAR, star);
        wrmsr(IA32_LSTAR, vibux_syscall_entry as usize as u64);
        wrmsr(IA32_FMASK, (1 << 8) | (1 << 9) | (1 << 10));
    }
}

#[unsafe(no_mangle)]
extern "C" fn syscall_dispatch(frame: *mut SyscallFrame) {
    unsafe {
        syscall(&mut *frame);
    }
}

pub(crate) fn clear_exit_state() {
    unsafe {
        let state = state_mut();
        state.exit_requested = false;
        state.exit_code = 0;
        core::ptr::addr_of_mut!(vibux_exit_requested).write(0);
        core::ptr::addr_of_mut!(vibux_exit_code).write(0);
    }
}
pub fn exit_requested() -> bool {
    unsafe { state_mut().exit_requested }
}
pub fn exit_code() -> i32 {
    unsafe { state_mut().exit_code }
}

pub fn reset_process_for_exec(
    main_start: u64,
    main_end: u64,
    rtld_start: u64,
    rtld_end: u64,
    stack_bottom: u64,
    stack_top: u64,
    tls_page: u64,
) {
    let old_mmap_end;
    let old_heap_end;
    unsafe {
        let state = state_mut();
        state.init();
        old_mmap_end = state.next_mmap;
        old_heap_end = state.brk.saturating_add(PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        state.next_mmap = MMAP_BASE;
        state.brk = HEAP_BASE;
        state.fs_base = 0;
        state.exit_requested = false;
        state.exit_code = 0;
        state.signal_actions = [SignalAction::empty(); 64];
        state.pending_signals = 0;
        PENDING_SIGNALS.store(0, Ordering::Release);
        state.tid_address = 0;
        state.robust_list = 0;
        state.robust_len = 0;
        state.rseq = 0;
        state.rseq_len = 0;
        let cwd = b"/home/boi";
        state.cwd = [0; 160];
        state.cwd[..cwd.len()].copy_from_slice(cwd);
        state.cwd_len = cwd.len();
        for fd in &mut state.fds[3..] {
            if fd.used && fd.flags & O_CLOEXEC != 0 {
                *fd = Fd::empty();
            }
        }
        core::ptr::addr_of_mut!(vibux_exit_requested).write(0);
        core::ptr::addr_of_mut!(vibux_exit_code).write(0);
    }
    let paging = PageTableManager::new(crate::velf::process_physical_offset());
    fn unmap_range(paging: &PageTableManager, start: u64, end: u64) {
        let mut page = start & !(PAGE_SIZE - 1);
        let end = end.saturating_add(PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        while page < end {
            let _ = paging.unmap_4k(page);
            page = match page.checked_add(PAGE_SIZE) {
                Some(value) => value,
                None => break,
            };
        }
    }
    unmap_range(&paging, main_start, main_end);
    unmap_range(&paging, rtld_start, rtld_end);
    unmap_range(&paging, stack_bottom, stack_top);
    unmap_range(&paging, tls_page, tls_page + PAGE_SIZE);
    let mmap_end = old_mmap_end.min(HEAP_BASE);
    if mmap_end > MMAP_BASE {
        unmap_range(&paging, MMAP_BASE, mmap_end);
    }
    let heap_end = old_heap_end.min(stack_bottom);
    if heap_end > HEAP_BASE {
        unmap_range(&paging, HEAP_BASE, heap_end);
    }
}

#[unsafe(no_mangle)]
extern "C" fn vibux_debug_return_stack_top() -> u64 {
    unsafe {
        let address: u64;
        core::arch::asm!("lea {0}, [rip + vibux_user_return_stack_top]", out(reg) address, options(nostack, preserves_flags));
        address
    }
}

#[unsafe(no_mangle)]
extern "C" fn vibux_debug_exit_fast(
    code: u64,
    _: u64,
    _: u64,
    _: u64,
    _: u64,
) {
    unsafe {
        let state = state_mut();
        state.exit_code = code as i32;
        state.exit_requested = true;
        core::ptr::addr_of_mut!(vibux_exit_requested).write(1);
        core::ptr::addr_of_mut!(vibux_exit_code).write(code as i32);
    }
}

#[inline(never)]
fn compat_syscall(number: u64, frame: &SyscallFrame) -> u64 {
    match number {
        36 => { if frame.rsi != 0 { let _ = copy_user_out(frame.rsi, &[0u8;32]); } 0 },
        37 => 0,
        38 => { if frame.rdx != 0 && copy_user_in(frame.rdx, &mut [0u8;32]).is_err() { neg_errno(EFAULT) } else { 0 } },
        74 => match fd_snapshot(frame.rdi as i32) { Ok(_) => 0, Err(code) => code },
        75 => match fd_snapshot(frame.rdi as i32) { Ok(_) => 0, Err(code) => code },
        133 => neg_errno(EROFS),
        137 => fill_statfs(frame.rsi),
        138 => match fd_snapshot(frame.rdi as i32) { Ok(_) => fill_statfs(frame.rsi), Err(code) => code },
        200 => if frame.rdi == unsafe { state_mut().pid } { if frame.rsi == 0 { 0 } else if frame.rsi <= 64 && raise_signal(frame.rsi) { 0 } else { neg_errno(EINVAL) } } else { neg_errno(ESRCH) },
        203 => 0,
        204 => { let bytes = core::cmp::min(frame.rsi as usize,128); if bytes == 0 { return neg_errno(EINVAL); } let mut mask=[0u8;128]; mask[0]=1; if copy_user_out(frame.rdx,&mask[..bytes]).is_err(){neg_errno(EFAULT)}else{(bytes*8) as u64} },
        212 => neg_errno(EOPNOTSUPP),
        214 => neg_errno(EOPNOTSUPP),
        215 => neg_errno(EOPNOTSUPP),
        219 => neg_errno(EINTR),
        221 => match fd_snapshot(frame.rdi as i32) { Ok(_) => 0, Err(code) => code },
        298 => neg_errno(EPERM),
        305 => neg_errno(EPERM),
        306 => match fd_snapshot(frame.rdi as i32) { Ok(_) => 0, Err(code) => code },
        312 => neg_errno(EINVAL),
        314 => 0,
        315 => 0,
        323 => neg_errno(ENOSYS),
        324 => 0,
        325 => 0,
        329 => change_protection(frame.rdi, frame.rsi, frame.rdx),
        330 => 0,
        331 => 0,
        29 => 1,
        30 | 31 | 64 | 65 | 66 | 67 | 68 | 69 | 70 | 71 => neg_errno(EOPNOTSUPP),
        127 => {
            let mask = unsafe { state_mut().pending_signals } & unsafe { state_mut().signal_mask };
            if frame.rdi == 0 || copy_user_out(frame.rdi, &mask.to_le_bytes()).is_err() { neg_errno(EFAULT) } else { 0 }
        }
        128 => {
            if frame.r10 != 8 { neg_errno(EINVAL) } else {
                let pending = unsafe { state_mut().pending_signals } & !unsafe { state_mut().signal_mask };
                if pending == 0 { neg_errno(EAGAIN) } else { pending.trailing_zeros() as u64 + 1 }
            }
        }
        129 => if frame.rdi == 0 || frame.rdi > 64 { neg_errno(EINVAL) } else if raise_signal(frame.rdi) { 0 } else { neg_errno(EINVAL) },
        130 => neg_errno(EINTR),
        131 => {
            let old = unsafe { state_mut().altstack };
            if frame.rsi != 0 {
                let mut raw=[0u8;24]; if copy_user_in(frame.rsi,&mut raw).is_err(){return neg_errno(EFAULT);}
                unsafe { state_mut().altstack=AltStack{sp:u64::from_le_bytes(raw[0..8].try_into().unwrap()),flags:u64::from_le_bytes(raw[8..16].try_into().unwrap()),size:u64::from_le_bytes(raw[16..24].try_into().unwrap())}; }
            }
            if frame.rdi != 0 {
                let mut raw=[0u8;24]; raw[..8].copy_from_slice(&old.sp.to_le_bytes()); raw[8..16].copy_from_slice(&old.flags.to_le_bytes()); raw[16..24].copy_from_slice(&old.size.to_le_bytes()); if copy_user_out(frame.rdi,&raw).is_err(){return neg_errno(EFAULT);}
            }
            0
        }
        139 => 0,
        141 => 0,
        142 => if frame.rsi == 0 || copy_user_out(frame.rsi,&[0u8;4]).is_ok(){0}else{neg_errno(EFAULT)},
        143 | 144 => 0,
        145 => unsafe { state_mut().scheduler as u64 },
        146 => 99,
        147 => 0,
        148 => write_timespec(frame.rsi,4_000_000),
        149 | 150 | 151 | 152 | 153 => 0,
        154 | 155 | 156 | 159 | 161 | 163 | 164 | 165 | 166 | 167 | 168 | 169 | 172 | 173 | 174 | 175 | 176 | 177 | 178 | 179 | 180 | 181 | 182 | 183 | 184 | 185 => neg_errno(EPERM),
        157 | 170 | 171 => 0,
        187 => 0,
        206 | 207 | 208 | 209 | 210 => neg_errno(EOPNOTSUPP),
        213 => epoll_alloc(0) as u64,
        216 => 0,
        220 => neg_errno(EOPNOTSUPP),
        222 | 223 | 224 | 225 | 226 => neg_errno(EOPNOTSUPP),
        227 => neg_errno(EPERM),
        230 => {
            if frame.rsi & !TIMER_ABSTIME != 0 { return neg_errno(EINVAL); }
            let ticks=match timeout_ticks_from_timespec(frame.rdx){Ok(Some(v))=>v,Ok(None)=>0,Err(e)=>return e}; let end=current_ticks().saturating_add(ticks); while current_ticks()<end{unsafe{asm!("sti; hlt",options(nomem,nostack,preserves_flags));}} 0
        }
        235 => 0,
        236 => neg_errno(ENOSYS),
        237 | 238 | 239 => 0,
        240 | 241 | 242 | 243 | 244 | 245 => neg_errno(EOPNOTSUPP),
        246 => neg_errno(EPERM),
        247 => crate::process::wait4(-1,frame.rsi,0,0),
        248 | 249 | 250 => neg_errno(EOPNOTSUPP),
        251 | 252 => 0,
        253 => unsafe { state_mut().alloc_fd(Fd{used:true,kind:FILE_KIND_INOTIFY,flags:frame.rdi,position:0,path:[0;160],path_len:0}).map(|f|f as u64).unwrap_or(neg_errno(EMFILE)) },
        254 | 255 | 256 => 0,
        297 => neg_errno(ESRCH),
        300 | 301 => neg_errno(EPERM),
        303 | 304 => neg_errno(EOPNOTSUPP),
        308 => neg_errno(EPERM),
        309 => { if frame.rdi!=0 && copy_user_out(frame.rdi,&0u32.to_le_bytes()).is_err(){return neg_errno(EFAULT);} if frame.rsi!=0 && copy_user_out(frame.rsi,&0u32.to_le_bytes()).is_err(){return neg_errno(EFAULT);} 0 }
        313 => neg_errno(EPERM),
        335 | 336 => neg_errno(ENOSYS),
        451 => { if frame.rdx != 0 { return neg_errno(EINVAL); } if fd_snapshot(frame.rdi as i32).is_err(){return neg_errno(EBADF);} copy_user_zero(frame.rsi,40) }
        452 => { let mut p=[0u8;160]; match read_c_string(frame.rsi,&mut p){Ok(n)=>chmod_like(frame.rdi as i32,frame.rdx,true,&p[..n]),Err(e)=>neg_errno(e)} }
        453 => neg_errno(EOPNOTSUPP),
        454 => futex_impl(frame.rdi,FUTEX_WAKE,frame.rdx,0),
        455 => futex_impl(frame.rdi,FUTEX_WAIT,frame.rdx,frame.r10),
        456 => if frame.rdi==0 {neg_errno(EFAULT)} else {0},
        457 | 458 | 459 | 461 => neg_errno(ENOSYS),
        460 => neg_errno(EPERM),
        462 => if frame.rdi&(PAGE_SIZE-1)!=0 || frame.rsi==0 {neg_errno(EINVAL)} else {0},
        463 | 464 | 465 | 466 => neg_errno(ENODATA),
        467 | 468 | 469 | 470 => neg_errno(EOPNOTSUPP),
        471 => 0,
        _ => neg_errno(ENOSYS),
    }
}

#[inline(never)]
fn syscall(frame: &mut SyscallFrame) {
    unsafe {
        state_mut().init();
    }
    let number = frame.rax;
    let result = match number {
        0 => read_fd(frame.rdi as i32, frame.rsi, frame.rdx as usize),
        1 => write_fd(frame.rdi as i32, frame.rsi, frame.rdx as usize),
        2 | 257 => {
            let path_address = if number == 2 { frame.rdi } else { frame.rsi };
            let flags = if number == 2 { frame.rsi } else { frame.rdx };
            let mut path = [0u8; 160];
            match read_c_string(path_address, &mut path) {
                Ok(length) => {
                    open_path(&path[..length], if number == 2 { frame.rdx } else { flags })
                }
                Err(code) => neg_errno(code),
            }
        }
        3 => unsafe {
            let fd = frame.rdi as i32;
            if fd < 0 || fd as usize >= FILE_FD_MAX || !state_mut().fds[fd as usize].used {
                neg_errno(EBADF)
            } else {
                state_mut().fds[fd as usize] = Fd::empty();
                0
            }
        },
        4 | 6 => {
            let mut path = [0u8; 160];
            match read_c_string(frame.rdi, &mut path) {
                Ok(length) => {
                    let p = &path[..length];
                    if !path_exists(p) {
                        neg_errno(ENOENT)
                    } else {
                        fill_stat(
                            frame.rsi,
                            path_mode(p),
                            crate::velf::store::find(p)
                                .map(|x| x.bytes.len() as u64)
                                .or_else(|| text_file(p).map(|x| x.len() as u64))
                                .unwrap_or(0),
                            p,
                        )
                    }
                }
                Err(code) => neg_errno(code),
            }
        }
        5 => unsafe {
            let fd = frame.rdi as i32;
            if fd < 0 || fd as usize >= FILE_FD_MAX || !state_mut().fds[fd as usize].used {
                neg_errno(EBADF)
            } else {
                let descriptor = state_mut().fds[fd as usize];
                let size = fd_for_binary(&descriptor)
                    .map(|x| x.len() as u64)
                    .or_else(|| fd_for_text(&descriptor).map(|x| x.len() as u64))
                    .or_else(|| ram_file_size(&descriptor).map(|x| x as u64))
                    .unwrap_or(0);
                let identity = if descriptor.path_len != 0 {
                    &descriptor.path[..descriptor.path_len]
                } else {
                    b"<special-file>"
                };
                let mode = match descriptor.kind {
                    FILE_KIND_TEXT => path_mode(&descriptor.path[..descriptor.path_len]),
                    FILE_KIND_BINARY => 0o100755,
                    FILE_KIND_RAMFILE => path_mode(&descriptor.path[..descriptor.path_len]),
                    FILE_KIND_TTY => 0o20666,
                    _ => 0o20666,
                };
                fill_stat(frame.rsi, mode, size, identity)
            }
        },
        7 => {
            let timeout = frame.rdx as i64;
            if timeout == 0 {
                match poll_scan(frame.rdi, frame.rsi as usize) {
                    Ok(value) => value as u64,
                    Err(code) => code,
                }
            } else {
                let start = current_ticks();
                let deadline = if timeout < 0 {
                    None
                } else {
                    Some(start.saturating_add(((timeout as u64) + 3) / 4))
                };
                loop {
                    match poll_scan(frame.rdi, frame.rsi as usize) {
                        Ok(value) if value != 0 => break value as u64,
                        Ok(_) => {}
                        Err(code) => break code,
                    }
                    if let Some(deadline) = deadline {
                        if current_ticks() >= deadline {
                            break 0;
                        }
                    }
                    unsafe {
                        asm!("sti; hlt", options(nomem, nostack, preserves_flags));
                    }
                }
            }
        }
        8 => unsafe {
            let fd = frame.rdi as i32;
            let offset = frame.rsi as i64;
            let whence = frame.rdx as i32;
            if fd < 0 || fd as usize >= FILE_FD_MAX || !state_mut().fds[fd as usize].used {
                neg_errno(EBADF)
            } else {
                let descriptor = state_mut().fds[fd as usize];
                if descriptor.kind == FILE_KIND_TTY
                    || descriptor.kind == FILE_KIND_PIPE_READ
                    || descriptor.kind == FILE_KIND_PIPE_WRITE
                    || descriptor.kind == FILE_KIND_SOCKET
                {
                    neg_errno(ESPIPE)
                } else {
                    let current = descriptor.position as i64;
                    let size = fd_for_binary(&descriptor)
                        .map(|x| x.len() as i64)
                        .or_else(|| fd_for_text(&descriptor).map(|x| x.len() as i64))
                        .or_else(|| ram_file_size(&descriptor).map(|x| x as i64))
                        .unwrap_or(0);
                    let next = match whence {
                        0 => offset,
                        1 => current.saturating_add(offset),
                        2 => size.saturating_add(offset),
                        3 => {
                            let _ = offset;
                            current
                        }
                        4 => {
                            let _ = offset;
                            size
                        }
                        _ => return_frame_result!(frame, neg_errno(EINVAL)),
                    };
                    if next < 0 {
                        neg_errno(EINVAL)
                    } else {
                        state_mut().fds[fd as usize].position = next as u64;
                        next as u64
                    }
                }
            }
        },
        9 => map_memory(
            frame.rdi,
            frame.rsi,
            frame.rdx,
            frame.r10,
            frame.r8 as i32,
            frame.r9,
        ),
        10 => change_protection(frame.rdi, frame.rsi, frame.rdx),
        11 => unmap_memory(frame.rdi, frame.rsi),
        12 => unsafe {
            let state = state_mut();
            state.init();
            let requested = frame.rdi;
            if requested == 0 {
                state.brk
            } else if requested > state.brk {
                crate::velf::with_process_allocator(|allocator, physical_offset| {
                    let paging = PageTableManager::new(physical_offset);
                    let mut page = (state.brk + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
                    let target = (requested + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
                    while page < target {
                        let frame_alloc = allocator.allocate().ok_or(neg_errno(ENOMEM))?;
                        paging
                            .map_4k(
                                page,
                                frame_alloc.addr(),
                                PageFlags::USER
                                    .union(PageFlags::WRITABLE)
                                    .union(PageFlags::NO_EXECUTE),
                                || allocator.allocate().map(|x| x.addr()),
                            )
                            .map_err(|_| neg_errno(ENOMEM))?;
                        core::ptr::write_bytes(
                            (physical_offset + frame_alloc.addr()) as *mut u8,
                            0,
                            PAGE_SIZE as usize,
                        );
                        page += PAGE_SIZE;
                    }
                    Ok::<(), u64>(())
                })
                .unwrap_or(Err(neg_errno(ENOMEM)))
                .map(|_| {
                    state.brk = requested;
                    state.brk
                })
                .unwrap_or_else(|x| x)
            } else {
                state.brk = requested;
                state.brk
            }
        },
        13 => rt_sigaction_impl(frame.rdi, frame.rsi, frame.rdx, frame.r10 as usize),
        14 => rt_sigprocmask_impl(frame.rdi, frame.rsi, frame.rdx, frame.r10 as usize),
        15 => rt_sigreturn_impl(frame),
        16 => handle_ioctl(frame.rdi as i32, frame.rsi, frame.rdx),
        17 => {
            let fd = frame.rdi as i32;
            let offset = frame.r10;
            let amount = frame.rdx as usize;
            let Ok(descriptor) = fd_snapshot(fd) else {
                return_frame_result!(frame, neg_errno(EBADF))
            };
            match descriptor.kind {
                FILE_KIND_BINARY | FILE_KIND_TEXT => {
                    let data = if descriptor.kind == FILE_KIND_BINARY {
                        fd_for_binary(&descriptor)
                    } else {
                        fd_for_text(&descriptor)
                    };
                    let Some(data) = data else {
                        return_frame_result!(frame, neg_errno(ENOENT))
                    };
                    let start = core::cmp::min(offset as usize, data.len());
                    let amount = core::cmp::min(amount, data.len().saturating_sub(start));
                    match copy_user_out(frame.rsi, &data[start..start + amount]) {
                        Ok(()) => amount as u64,
                        Err(code) => neg_errno(code),
                    }
                }
                FILE_KIND_NULL | FILE_KIND_ZERO | FILE_KIND_TTY | FILE_KIND_EVENTFD
                | FILE_KIND_TIMERFD | FILE_KIND_SIGNALFD => read_fd(fd, frame.rsi, amount),
                _ => neg_errno(EBADF),
            }
        }
        18 => neg_errno(EROFS),
        19 => {
            let fd = frame.rdi as i32;
            let iov_addr = frame.rsi;
            let iov_count = frame.rdx as usize;
            if iov_count > 1024 {
                neg_errno(EINVAL)
            } else {
                let mut total = 0usize;
                let mut i = 0usize;
                while i < iov_count {
                    let mut raw = [0u8; 16];
                    if copy_user_in(iov_addr + (i as u64) * 16, &mut raw).is_err() {
                        return_frame_result!(
                            frame,
                            if total == 0 {
                                neg_errno(EFAULT)
                            } else {
                                total as u64
                            }
                        )
                    }
                    let base = u64::from_le_bytes(raw[0..8].try_into().unwrap());
                    let len = u64::from_le_bytes(raw[8..16].try_into().unwrap());
                    let amount = core::cmp::min(len as usize, 1 << 20);
                    let result = read_fd(fd, base, amount);
                    if result > 0 {
                        total = total.saturating_add(result as usize);
                    } else if total == 0 {
                        return_frame_result!(frame, result)
                    } else {
                        break;
                    }
                    i += 1;
                }
                total as u64
            }
        }
        20 => {
            let fd = frame.rdi as i32;
            let iov_addr = frame.rsi;
            let iov_count = frame.rdx as usize;
            if iov_count > 1024 {
                neg_errno(EINVAL)
            } else {
                let mut total = 0usize;
                let mut i = 0usize;
                while i < iov_count {
                    let mut raw = [0u8; 16];
                    if copy_user_in(iov_addr + (i as u64) * 16, &mut raw).is_err() {
                        return_frame_result!(
                            frame,
                            if total == 0 {
                                neg_errno(EFAULT)
                            } else {
                                total as u64
                            }
                        )
                    }
                    let base = u64::from_le_bytes(raw[0..8].try_into().unwrap());
                    let len = u64::from_le_bytes(raw[8..16].try_into().unwrap());
                    let amount = core::cmp::min(len as usize, 1 << 20);
                    let result = write_fd(fd, base, amount);
                    if result > 0 {
                        total = total.saturating_add(result as usize);
                    } else if total == 0 {
                        return_frame_result!(frame, result)
                    } else {
                        break;
                    }
                    i += 1;
                }
                total as u64
            }
        }
        295 | 327 => {
            let fd = frame.rdi as i32;
            let iov_addr = frame.rsi;
            let iov_count = frame.rdx as usize;
            let offset = frame.r10;
            if iov_count > 1024 {
                neg_errno(EINVAL)
            } else {
                let Ok(descriptor) = fd_snapshot(fd) else {
                    return_frame_result!(frame, neg_errno(EBADF))
                };
                let mut total = 0usize;
                let mut i = 0usize;
                let mut current_offset = offset;
                while i < iov_count {
                    let mut raw = [0u8; 16];
                    if copy_user_in(iov_addr + (i as u64) * 16, &mut raw).is_err() {
                        return_frame_result!(
                            frame,
                            if total == 0 {
                                neg_errno(EFAULT)
                            } else {
                                total as u64
                            }
                        )
                    }
                    let base = u64::from_le_bytes(raw[0..8].try_into().unwrap());
                    let len = u64::from_le_bytes(raw[8..16].try_into().unwrap());
                    let amount = core::cmp::min(len as usize, 1 << 20);
                    let data = match descriptor.kind {
                        FILE_KIND_BINARY => fd_for_binary(&descriptor),
                        FILE_KIND_TEXT => fd_for_text(&descriptor),
                        _ => return_frame_result!(frame, neg_errno(EBADF)),
                    };
                    let Some(data) = data else {
                        return_frame_result!(frame, neg_errno(ENOENT))
                    };
                    let start = core::cmp::min(current_offset as usize, data.len());
                    let n = core::cmp::min(amount, data.len().saturating_sub(start));
                    if n > 0 {
                        if copy_user_out(base, &data[start..start + n]).is_err() {
                            return_frame_result!(
                                frame,
                                if total == 0 {
                                    neg_errno(EFAULT)
                                } else {
                                    total as u64
                                }
                            )
                        }
                    }
                    total += n;
                    current_offset += n as u64;
                    if n < amount {
                        break;
                    }
                    i += 1;
                }
                total as u64
            }
        }
        296 | 328 => {
            let fd = frame.rdi as i32;
            let iov_addr = frame.rsi;
            let iov_count = frame.rdx as usize;
            let offset = frame.r10;
            if iov_count > 1024 {
                neg_errno(EINVAL)
            } else {
                let Ok(descriptor) = fd_snapshot(fd) else {
                    return_frame_result!(frame, neg_errno(EBADF))
                };
                if matches!(descriptor.kind, FILE_KIND_BINARY | FILE_KIND_TEXT) {
                    return_frame_result!(frame, neg_errno(EROFS))
                }
                let mut total = 0usize;
                let mut i = 0usize;
                let mut _current_offset = offset;
                while i < iov_count {
                    let mut raw = [0u8; 16];
                    if copy_user_in(iov_addr + (i as u64) * 16, &mut raw).is_err() {
                        return_frame_result!(
                            frame,
                            if total == 0 {
                                neg_errno(EFAULT)
                            } else {
                                total as u64
                            }
                        )
                    }
                    let base = u64::from_le_bytes(raw[0..8].try_into().unwrap());
                    let len = u64::from_le_bytes(raw[8..16].try_into().unwrap());
                    let amount = core::cmp::min(len as usize, 1 << 20);
                    let result = write_fd(fd, base, amount);
                    if result > 0 {
                        total = total.saturating_add(result as usize);
                        _current_offset += result;
                    } else if total == 0 {
                        return_frame_result!(frame, result)
                    } else {
                        break;
                    }
                    i += 1;
                }
                total as u64
            }
        }
        21 | 269 => {
            let path_address = if number == 21 { frame.rdi } else { frame.rsi };
            let mut path = [0u8; 160];
            match read_c_string(path_address, &mut path) {
                Ok(length) => access_path(
                    &path[..length],
                    if number == 21 { frame.rsi } else { frame.rdx },
                ),
                Err(code) => neg_errno(code),
            }
        }
        22 | 293 => create_pipe(frame.rdi, if number == 293 { frame.rsi } else { 0 }),
        23 => select_impl(frame.rdi, frame.rsi, frame.rdx, frame.r10, frame.r8),
        24 => 0,
        25 => {
            let old_addr = frame.rdi;
            let old_size = frame.rsi;
            let new_size = frame.rdx;
            let flags = frame.r10;
            let new_address = frame.r8;
            if old_size == 0 || new_size == 0 {
                neg_errno(EINVAL)
            } else if new_size <= old_size && !(flags & MREMAP_FIXED != 0) {
                old_addr
            } else if flags & MREMAP_MAYMOVE == 0 {
                neg_errno(ENOMEM)
            } else {
                let target = if flags & MREMAP_FIXED != 0 {
                    new_address
                } else {
                    0
                };
                let mapped = map_memory(
                    target,
                    new_size,
                    PROT_READ | PROT_WRITE,
                    MAP_PRIVATE | MAP_ANONYMOUS | if target != 0 { MAP_FIXED } else { 0 },
                    -1,
                    0,
                );
                if mapped >= USER_LIMIT {
                    mapped
                } else {
                    let copy_len = core::cmp::min(old_size, new_size) as usize;
                    let mut copied = 0usize;
                    while copied < copy_len {
                        let amount = core::cmp::min(4096, copy_len - copied);
                        let mut buffer = [0u8; 4096];
                        if copy_user_in(old_addr + copied as u64, &mut buffer[..amount]).is_err() {
                            return_frame_result!(frame, neg_errno(EFAULT))
                        }
                        if copy_user_out(mapped + copied as u64, &buffer[..amount]).is_err() {
                            return_frame_result!(frame, neg_errno(EFAULT))
                        }
                        copied += amount;
                    }
                    let _ = unmap_memory(old_addr, old_size);
                    mapped
                }
            }
        }
        26 => 0,
        27 => {
            if frame.rsi == 0 {
                neg_errno(EINVAL)
            } else {
                let pages = ((frame.rsi + PAGE_SIZE - 1) / PAGE_SIZE) as usize;
                let mut vec = [0u8; 4096];
                let count = core::cmp::min((pages + 7) / 8, vec.len());
                let paging = PageTableManager::new(crate::velf::process_physical_offset());
                let mut page = 0usize;
                while page < pages {
                    if paging
                        .translate(frame.rdi + (page as u64) * PAGE_SIZE)
                        .is_some()
                    {
                        vec[page >> 3] |= 1 << (page & 7);
                    }
                    page += 1;
                }
                if copy_user_out(frame.rdx, &vec[..count]).is_ok() {
                    0
                } else {
                    neg_errno(EFAULT)
                }
            }
        }
        28 => 0,
        32 => fd_duplicate(frame.rdi as i32, 3, false),
        33 => fd_duplicate_exact(frame.rdi as i32, frame.rsi as i32, false),
        34 => neg_errno(EINTR),
        35 => {
            let request = frame.rdi;
            let remaining = frame.rsi;
            if request == 0 {
                neg_errno(EINVAL)
            } else {
                let ticks = match timeout_ticks_from_timespec(request) {
                    Ok(Some(value)) => value,
                    Ok(None) => 0,
                    Err(code) => return_frame_result!(frame, code),
                };
                let target = current_ticks().saturating_add(ticks);
                while current_ticks() < target {
                    unsafe {
                        asm!("sti; hlt", options(nomem, nostack, preserves_flags));
                    }
                }
                if remaining != 0 {
                    let _ = copy_user_out(remaining, &[0u8; 16]);
                }
                0
            }
        }
        36 => {
            if frame.rsi != 0 {
                let _ = fill_timeval(frame.rsi);
            }
            0
        }
        37 => 0,
        38 => {
            if frame.rdx != 0 {
                let _ = copy_user_out(frame.rdx, &[0u8; 32]);
            }
            0
        }
        39 => unsafe { state_mut().pid },
        40 => neg_errno(EINVAL),
        41 => socket_alloc(
            frame.rdi,
            frame.rsi & !(SOCK_CLOEXEC | SOCK_NONBLOCK),
            frame.rsi,
        ),
        42 => neg_errno(ECONNREFUSED),
        43 => neg_errno(EAGAIN),
        44 => { let fd=frame.rdi as i32; match fd_snapshot(fd) { Ok(d) if d.kind==FILE_KIND_SOCKET => socket_write_fd(fd,frame.rsi,frame.rdx as usize), Ok(_)=>neg_errno(EBADF), Err(e)=>e } }
        45 => { let fd=frame.rdi as i32; match fd_snapshot(fd) { Ok(d) if d.kind==FILE_KIND_SOCKET => socket_read_fd(fd,frame.rsi,frame.rdx as usize), Ok(_)=>neg_errno(EBADF), Err(e)=>e } }
        46 => neg_errno(ENOTCONN),
        47 => neg_errno(ENOTCONN),
        48 => 0,
        49 => 0,
        50 => neg_errno(ENOTCONN),
        51 => neg_errno(ENOTCONN),
        52 => neg_errno(EAFNOSUPPORT),
        53 => {
            if frame.rdi == 1 { socket_pair_from_user(frame.r10, frame.rdx) } else { neg_errno(EAFNOSUPPORT) }
        },
        54 => neg_errno(ENOPROTOOPT),
        55 => match frame.rdi {
            SO_TYPE => {
                let t = 1u32;
                if copy_user_out(frame.rsi, &t.to_le_bytes()).is_ok() {
                    0
                } else {
                    neg_errno(EFAULT)
                }
            }
            SO_ERROR => {
                let v = 0u32;
                if copy_user_out(frame.rsi, &v.to_le_bytes()).is_ok() {
                    0
                } else {
                    neg_errno(EFAULT)
                }
            }
            _ => neg_errno(ENOPROTOOPT),
        },
        56 => {
            let clone_flags = frame.rdi;
            if clone_flags & 0x0000_0100 != 0 {
                crate::process::do_vfork(frame as *mut SyscallFrame, clone_flags, frame.rsi)
            } else {
                crate::process::do_fork(frame as *mut SyscallFrame, clone_flags, frame.rsi)
            }
        }
        57 => crate::process::do_fork(frame as *mut SyscallFrame, 0, 0),
        58 => crate::process::do_vfork(frame as *mut SyscallFrame, 0x0000_4000, 0),
        59 => crate::process::execve(frame.rdi, frame.rsi, frame.rdx),
        60 | 231 => {
            unsafe {
                let state = state_mut();
                state.exit_code = frame.rdi as i32;
                state.exit_requested = true;
            }
            0
        }
        61 => crate::process::wait4(frame.rdi as i32, frame.rsi, frame.rdx, frame.r10),
        62 | 200 | 234 => {
            let pid = unsafe { state_mut().pid };
            let valid_target = match number {
                62 => {
                    let target = frame.rdi as i64;
                    target == 0 || target == pid as i64 || target == -1
                }
                200 => frame.rdi == pid,
                234 => frame.rdi == pid && frame.rsi == pid,
                _ => false,
            };
            if !valid_target {
                neg_errno(ESRCH)
            } else {
                let signal = match number {
                    62 | 200 => frame.rsi,
                    234 => frame.rdx,
                    _ => 0,
                };
                if signal > 64 {
                    neg_errno(EINVAL)
                } else if signal == 0 {
                    0
                } else if raise_signal(signal) {
                    0
                } else {
                    neg_errno(EINVAL)
                }
            }
        }
        63 => fill_uname(frame.rdi),
        64 | 65 | 66 | 67 | 68 | 69 | 70 | 71 => neg_errno(ENOSYS),
        72 => unsafe {
            let fd = frame.rdi as i32;
            if fd < 0 || fd as usize >= FILE_FD_MAX || !state_mut().fds[fd as usize].used {
                neg_errno(EBADF)
            } else {
                match frame.rsi {
                    F_DUPFD => fd_duplicate(fd, frame.rdx as i32, false),
                    F_DUPFD_CLOEXEC => fd_duplicate(fd, frame.rdx as i32, true),
                    F_GETFD => {
                        if state_mut().fds[fd as usize].flags & O_CLOEXEC != 0 {
                            1
                        } else {
                            0
                        }
                    }
                    F_SETFD => {
                        if frame.rdx & 1 != 0 {
                            state_mut().fds[fd as usize].flags |= O_CLOEXEC;
                        } else {
                            state_mut().fds[fd as usize].flags &= !O_CLOEXEC;
                        }
                        0
                    }
                    F_GETFL => state_mut().fds[fd as usize].flags,
                    F_SETFL => {
                        let preserved = state_mut().fds[fd as usize].flags & O_ACCMODE;
                        state_mut().fds[fd as usize].flags = preserved | (frame.rdx & !O_ACCMODE);
                        0
                    }
                    F_GETOWN => 0,
                    F_SETOWN => 0,
                    _ => neg_errno(EINVAL),
                }
            }
        },
        73 => 0,
        74 | 75 => 0,
        76 => {
            let mut path = [0u8;160];
            match read_c_string(frame.rdi, &mut path) { Ok(n) => ram_file_truncate(&path[..n], frame.rsi as usize), Err(e) => neg_errno(e) }
        }
        77 => {
            let fd = frame.rdi as i32;
            let Ok(d) = fd_snapshot(fd) else { return_frame_result!(frame, neg_errno(EBADF)) };
            if d.kind == FILE_KIND_RAMFILE { ram_file_truncate(&d.path[..d.path_len], frame.rsi as usize) } else { neg_errno(EROFS) }
        },
        78 | 217 => fill_getdents64(frame.rdi as i32, frame.rsi, frame.rdx as usize),
        79 => unsafe {
            let buffer = frame.rdi;
            let size = frame.rsi as usize;
            let length = state_mut().cwd_len + 1;
            if buffer == 0 || size == 0 {
                neg_errno(EINVAL)
            } else if length > size {
                neg_errno(34)
            } else {
                let mut data = [0u8; 161];
                data[..length - 1].copy_from_slice(&state_mut().cwd[..state_mut().cwd_len]);
                data[length - 1] = 0;
                if copy_user_out(buffer, &data[..length]).is_ok() {
                    length as u64
                } else {
                    neg_errno(EFAULT)
                }
            }
        },
        80 => {
            let mut path = [0u8; 160];
            match read_c_string(frame.rdi, &mut path) {
                Ok(length) => {
                    let source = &path[..length];
                    if is_directory_path(source) || source == b"/" {
                        unsafe {
                            state_mut().cwd = [0; 160];
                            state_mut().cwd[..source.len()].copy_from_slice(source);
                            state_mut().cwd_len = source.len();
                        }
                        0
                    } else {
                        neg_errno(ENOENT)
                    }
                }
                Err(code) => neg_errno(code),
            }
        }
        81 => unsafe {
            let fd = frame.rdi as i32;
            if fd < 0 || fd as usize >= FILE_FD_MAX || !state_mut().fds[fd as usize].used {
                neg_errno(EBADF)
            } else if fd_is_directory(&state_mut().fds[fd as usize]) {
                let descriptor = state_mut().fds[fd as usize];
                let path = normalize_fd_path(&descriptor);
                state_mut().cwd = [0; 160];
                state_mut().cwd[..path.len()].copy_from_slice(path);
                state_mut().cwd_len = path.len();
                0
            } else {
                neg_errno(ENOTDIR)
            }
        },
        82 => {
            let mut old = [0u8;160];
            let mut new = [0u8;160];
            match (read_c_string(frame.rdi,&mut old), read_c_string(frame.rsi,&mut new)) { (Ok(a),Ok(b)) => ram_file_rename(&old[..a],&new[..b]), _ => neg_errno(EFAULT) }
        }
        83 => neg_errno(EROFS),
        84 => neg_errno(EROFS),
        85 => { let mut path=[0u8;160]; match read_c_string(frame.rdi,&mut path) { Ok(n)=>open_path(&path[..n],O_CREAT|O_WRONLY|O_TRUNC|(frame.rsi&0o777)), Err(e)=>neg_errno(e) } }
        86 => neg_errno(EROFS),
        87 => { let mut path=[0u8;160]; match read_c_string(frame.rdi,&mut path) { Ok(n)=>ram_file_remove(&path[..n]), Err(e)=>neg_errno(e) } }
        88 => neg_errno(EROFS),
        89 | 267 => {
            let path_address = if number == 89 { frame.rdi } else { frame.rsi };
            let mut path = [0u8; 160];
            match read_c_string(path_address, &mut path) {
                Ok(length) => {
                    let source = &path[..length];
                    let target = if source == b"/proc/self/exe" || source == b"/proc/1/exe" {
                        b"/bin/bash".as_slice()
                    } else if source == b"/bin/sh" {
                        b"/bin/sh".as_slice()
                    } else if source == b"/bin/ls" {
                        b"/bin/ls".as_slice()
                    } else if source == b"/bin/cat" {
                        b"/bin/cat".as_slice()
                    } else {
                        return_frame_result!(frame, neg_errno(ENOENT))
                    };
                    let amount = core::cmp::min(
                        if number == 89 { frame.rdx } else { frame.r10 } as usize,
                        target.len(),
                    );
                    let destination = if number == 89 { frame.rsi } else { frame.rdx };
                    match copy_user_out(destination, &target[..amount]) {
                        Ok(()) => amount as u64,
                        Err(code) => neg_errno(code),
                    }
                }
                Err(code) => neg_errno(code),
            }
        }
        90 => {
            let mut path = [0u8; 160];
            match read_c_string(frame.rdi, &mut path) {
                Ok(length) => chmod_like(-1, frame.rsi, true, &path[..length]),
                Err(code) => neg_errno(code),
            }
        }
        91 => chmod_like(frame.rdi as i32, frame.rsi, false, b""),
        92 | 93 | 94 => 0,
        95 => unsafe {
            let old = state_mut().umask;
            state_mut().umask = (frame.rdi as u32) & 0o777;
            old as u64
        },
        96 => {
            let result = fill_timeval(frame.rdi);
            if result != 0 {
                result
            } else if frame.rsi != 0 {
                let timezone = [0u8; 8];
                if copy_user_out(frame.rsi, &timezone).is_err() {
                    neg_errno(EFAULT)
                } else {
                    0
                }
            } else {
                0
            }
        }
        97 => fill_rlimit(frame.rsi, frame.rdi),
        98 => fill_rusage(frame.rsi),
        99 => fill_sysinfo(frame.rdi),
        100 => current_ticks(),
        101 | 103 => neg_errno(EPERM),
        102 => unsafe { state_mut().uid as u64 },
        104 => unsafe { state_mut().gid as u64 },
        105 => {
            let value = frame.rdi as u32;
            let current = unsafe { state_mut().uid };
            if value == current || current == 0 {
                unsafe {
                    state_mut().uid = value;
                    state_mut().euid = value;
                    state_mut().fsuid = value;
                }
                0
            } else {
                neg_errno(EPERM)
            }
        }
        106 => {
            let value = frame.rdi as u32;
            let current = unsafe { state_mut().gid };
            if value == current || current == 0 {
                unsafe {
                    state_mut().gid = value;
                    state_mut().egid = value;
                    state_mut().fsgid = value;
                }
                0
            } else {
                neg_errno(EPERM)
            }
        }
        107 => unsafe { state_mut().euid as u64 },
        108 => unsafe { state_mut().egid as u64 },
        109 => {
            let pid_arg = frame.rdi as i32;
            let pgid = frame.rsi as u64;
            let self_pid = unsafe { state_mut().pid };
            if pid_arg != 0 && pid_arg as u64 != self_pid {
                neg_errno(ESRCH)
            } else {
                unsafe {
                    state_mut().pgid = if pgid == 0 { self_pid } else { pgid };
                }
                0
            }
        }
        110 => unsafe { state_mut().ppid },
        111 => unsafe { state_mut().pgid },
        112 => unsafe {
            let pid = state_mut().pid;
            state_mut().sid = pid;
            state_mut().pgid = pid;
            pid
        },
        113 => 0,
        114 => 0,
        115 => getgroups_impl(frame.rdi as usize, frame.rsi),
        116 => setgroups_impl(frame.rdi as usize, frame.rsi),
        117 => {
            let uid = unsafe { state_mut().euid };
            if copy_user_out(frame.rsi, &uid.to_le_bytes()).is_err()
                || copy_user_out(frame.rdx, &uid.to_le_bytes()).is_err()
                || copy_user_out(frame.r10, &uid.to_le_bytes()).is_err()
            {
                neg_errno(EFAULT)
            } else {
                0
            }
        }
        118 => {
            let value = unsafe { state_mut().euid };
            if frame.rdi == 0 && frame.rsi == 0 && frame.rdx == 0 {
                0
            } else if copy_user_out(frame.rdi, &value.to_le_bytes()).is_err()
                || copy_user_out(frame.rsi, &value.to_le_bytes()).is_err()
                || copy_user_out(frame.rdx, &value.to_le_bytes()).is_err()
            {
                neg_errno(EFAULT)
            } else {
                0
            }
        }
        119 => {
            let value = unsafe { state_mut().egid };
            if copy_user_out(frame.rsi, &value.to_le_bytes()).is_err()
                || copy_user_out(frame.rdx, &value.to_le_bytes()).is_err()
                || copy_user_out(frame.r10, &value.to_le_bytes()).is_err()
            {
                neg_errno(EFAULT)
            } else {
                0
            }
        }
        120 => {
            let value = unsafe { state_mut().egid };
            if copy_user_out(frame.rdi, &value.to_le_bytes()).is_err()
                || copy_user_out(frame.rsi, &value.to_le_bytes()).is_err()
                || copy_user_out(frame.rdx, &value.to_le_bytes()).is_err()
            {
                neg_errno(EFAULT)
            } else {
                0
            }
        }
        121 => unsafe { state_mut().pgid },
        122 => unsafe { state_mut().uid as u64 },
        123 => unsafe { state_mut().gid as u64 },
        124 => unsafe { state_mut().sid },
        125 => {
            let header = frame.rdi;
            if header == 0 {
                neg_errno(EFAULT)
            } else {
                let mut raw = [0u8; 8];
                if copy_user_in(header, &mut raw).is_err() {
                    neg_errno(EFAULT)
                } else {
                    0
                }
            }
        }
        126 => neg_errno(EPERM),
        127 => {
            if frame.rdi == 0 {
                neg_errno(EFAULT)
            } else if copy_user_out(frame.rdi, &0u64.to_le_bytes()).is_err() {
                neg_errno(EFAULT)
            } else {
                0
            }
        }
        128 => neg_errno(EAGAIN),
        129 => neg_errno(ENOSYS),
        130 => neg_errno(EINTR),
        131 => 0,
        132 => neg_errno(EROFS),
        133 | 134 => neg_errno(EPERM),
        135 => unsafe { state_mut().personality },
        136 => neg_errno(ENOSYS),
        137 | 138 => fill_statfs(frame.rdi),
        139 => neg_errno(ENOSYS),
        140 => unsafe { state_mut().priority as u64 },
        141 => unsafe { state_mut().priority as u64 },
        142 => {
            let which = frame.rdi;
            let who = frame.rsi;
            let prio = frame.rdx as i32;
            if which == 0 && (who == 0 || who as u64 == unsafe { state_mut().pid }) {
                let clamped = prio.clamp(-20, 19);
                unsafe {
                    state_mut().nice = clamped;
                }
                clamped as u64
            } else if which == 1 || which == 2 {
                prio.clamp(0, 139) as u64
            } else {
                neg_errno(EINVAL)
            }
        }
        143 => unsafe { state_mut().priority as u64 },
        144 => 0,
        145 => 0,
        146 => 99,
        147 => 1,
        148 => {
            let mut data = [0u8; 16];
            data[0..8].copy_from_slice(&0i64.to_le_bytes());
            data[8..16].copy_from_slice(&4_000_000i64.to_le_bytes());
            if copy_user_out(frame.rdx, &data).is_ok() {
                0
            } else {
                neg_errno(EFAULT)
            }
        }
        149 | 150 | 151 | 152 => 0,
        153 => neg_errno(EPERM),
        154 | 155 | 156 => neg_errno(EPERM),
        157 => {
            let option = frame.rdi;
            unsafe {
                let state = state_mut();
                state.init();
                match option {
                    PR_SET_PDEATHSIG => {
                        if frame.rsi > 64 {
                            neg_errno(EINVAL)
                        } else {
                            state.pdeath_signal = frame.rsi as u32;
                            0
                        }
                    }
                    PR_GET_PDEATHSIG => {
                        if copy_user_out(frame.rsi, &state.pdeath_signal.to_le_bytes()).is_ok() {
                            0
                        } else {
                            neg_errno(EFAULT)
                        }
                    }
                    PR_GET_DUMPABLE => state.dumpable as u64,
                    PR_SET_DUMPABLE => {
                        if frame.rsi <= 1 {
                            state.dumpable = frame.rsi as u32;
                            0
                        } else {
                            neg_errno(EINVAL)
                        }
                    }
                    PR_GET_KEEPCAPS => 0,
                    PR_SET_KEEPCAPS => 0,
                    PR_SET_NAME => {
                        let mut name = [0u8; 16];
                        if copy_user_in(frame.rsi, &mut name).is_err() {
                            neg_errno(EFAULT)
                        } else {
                            state.process_name = name;
                            0
                        }
                    }
                    PR_GET_NAME => {
                        if copy_user_out(frame.rsi, &state.process_name).is_ok() {
                            0
                        } else {
                            neg_errno(EFAULT)
                        }
                    }
                    PR_SET_NO_NEW_PRIVS => {
                        if frame.rsi == 1 {
                            state.no_new_privs = true;
                            0
                        } else {
                            neg_errno(EINVAL)
                        }
                    }
                    PR_GET_NO_NEW_PRIVS => state.no_new_privs as u64,
                    PR_GET_TID_ADDRESS => {
                        if copy_user_out(frame.rsi, &state.tid_address.to_le_bytes()).is_ok() {
                            0
                        } else {
                            neg_errno(EFAULT)
                        }
                    }
                    PR_GET_TIMERSLACK => state.timerslack_ns,
                    PR_SET_TIMERSLACK => {
                        if frame.rsi == 0 {
                            neg_errno(EINVAL)
                        } else {
                            state.timerslack_ns = frame.rsi;
                            0
                        }
                    }
                    PR_GET_TIMING => 0,
                    PR_SET_TIMING => {
                        if frame.rsi == 0 {
                            0
                        } else {
                            neg_errno(EINVAL)
                        }
                    }
                    PR_GET_SECCOMP => 0,
                    PR_SET_SECCOMP => {
                        if frame.rsi == 0 {
                            0
                        } else {
                            neg_errno(EINVAL)
                        }
                    }
                    PR_GET_CHILD_SUBREAPER => {
                        if copy_user_out(frame.rsi, &0i32.to_le_bytes()).is_ok() {
                            0
                        } else {
                            neg_errno(EFAULT)
                        }
                    }
                    PR_SET_CHILD_SUBREAPER => 0,
                    PR_GET_THP_DISABLE => {
                        if copy_user_out(frame.rsi, &0u64.to_le_bytes()).is_ok() {
                            0
                        } else {
                            neg_errno(EFAULT)
                        }
                    }
                    PR_SET_THP_DISABLE => 0,
                    PR_GET_TAGGED_ADDR_CTRL => {
                        if copy_user_out(frame.rsi, &0u64.to_le_bytes()).is_ok() {
                            0
                        } else {
                            neg_errno(EFAULT)
                        }
                    }
                    PR_SET_TAGGED_ADDR_CTRL => 0,
                    PR_CAP_AMBIENT => {
                        if frame.rdx == 4 {
                            0
                        } else if frame.rdx == 1 {
                            0
                        } else {
                            neg_errno(EINVAL)
                        }
                    }
                    PR_GET_SPECULATION_CTRL => {
                        if copy_user_out(frame.rsi, &0u64.to_le_bytes()).is_ok() {
                            0
                        } else {
                            neg_errno(EFAULT)
                        }
                    }
                    PR_SET_SPECULATION_CTRL => 0,
                    _ => neg_errno(EINVAL),
                }
            }
        }
        158 => match frame.rdi {
            ARCH_SET_GS => 0,
            ARCH_SET_FS => {
                set_fs_base(frame.rsi);
                unsafe {
                    state_mut().fs_base = frame.rsi;
                }
                0
            }
            ARCH_GET_FS => {
                let value = fs_base();
                if copy_user_out(frame.rsi, &value.to_le_bytes()).is_ok() {
                    0
                } else {
                    neg_errno(EFAULT)
                }
            }
            ARCH_GET_GS => {
                if copy_user_out(frame.rsi, &0u64.to_le_bytes()).is_ok() {
                    0
                } else {
                    neg_errno(EFAULT)
                }
            }
            _ => neg_errno(EINVAL),
        },
        159 => neg_errno(EPERM),
        160 => {
            let resource = frame.rdi;
            let pointer = frame.rsi;
            if pointer == 0 {
                neg_errno(EFAULT)
            } else if resource > 15 {
                neg_errno(EINVAL)
            } else {
                let mut raw = [0u8; 16];
                if copy_user_in(pointer, &mut raw).is_err() {
                    neg_errno(EFAULT)
                } else {
                    0
                }
            }
        }
        161 => neg_errno(EPERM),
        162 => 0,
        163 => neg_errno(EPERM),
        164 | 165 | 166 | 167 | 168 | 169 | 170 | 171 | 172 | 173 | 174 | 175 | 176 | 177 | 178
        | 179 | 180 | 181 | 182 | 183 | 184 | 185 => {
            if number == 182 || number == 183 {
                0
            } else {
                neg_errno(EPERM)
            }
        }
        186 => unsafe { state_mut().pid },
        187 => 0,
        188 | 189 | 190 => {
            if number == 188 {
                let value = frame.rdi as u32;
                let cur = unsafe { state_mut().uid };
                if value == cur || cur == 0 {
                    unsafe {
                        state_mut().fsuid = value;
                    }
                    0
                } else {
                    neg_errno(EPERM)
                }
            } else if number == 189 {
                let value = frame.rdi as u32;
                let cur = unsafe { state_mut().gid };
                if value == cur || cur == 0 {
                    unsafe {
                        state_mut().fsgid = value;
                    }
                    0
                } else {
                    neg_errno(EPERM)
                }
            } else {
                0
            }
        }
        191 | 192 | 193 => neg_errno(ENODATA),
        194 | 195 | 196 => {
            if frame.rdx == 0 {
                0
            } else {
                neg_errno(ENODATA)
            }
        }
        197 | 198 | 199 => neg_errno(ENOSYS),
        201 => {
            let seconds = current_ticks() / 250;
            if frame.rdi != 0 {
                if copy_user_out(frame.rdi, &(seconds as i64).to_le_bytes()).is_err() {
                    return_frame_result!(frame, neg_errno(EFAULT))
                }
            }
            seconds
        }
        202 => futex_impl(frame.rdi, frame.rsi, frame.rdx, frame.r10),
        203 => 0,
        204 => {
            let len = frame.rsi as usize;
            let bytes = core::cmp::min(len, 8);
            let mut mask = [0u8; 8];
            mask[0] = 1;
            if copy_user_out(frame.rdx, &mask[..bytes]).is_ok() {
                bytes as u64
            } else {
                neg_errno(EFAULT)
            }
        }
        205 => {
            let value = frame.rdi as u32;
            let cur = unsafe { state_mut().uid };
            if value == cur || cur == 0 {
                unsafe {
                    state_mut().uid = value;
                    state_mut().euid = value;
                    state_mut().suid = value;
                    state_mut().fsuid = value;
                }
                0
            } else {
                neg_errno(EPERM)
            }
        }
        206 => {
            if frame.rdi == 0 && frame.rsi == 0 && frame.rdx == 0 {
                0
            } else {
                let uid = unsafe { state_mut().uid };
                if copy_user_out(frame.rdi, &uid.to_le_bytes()).is_err()
                    || copy_user_out(frame.rsi, &uid.to_le_bytes()).is_err()
                    || copy_user_out(frame.rdx, &uid.to_le_bytes()).is_err()
                {
                    neg_errno(EFAULT)
                } else {
                    0
                }
            }
        }
        207 => {
            let value = frame.rdi as u32;
            let cur = unsafe { state_mut().gid };
            if value == cur || cur == 0 {
                unsafe {
                    state_mut().gid = value;
                    state_mut().egid = value;
                    state_mut().sgid = value;
                    state_mut().fsgid = value;
                }
                0
            } else {
                neg_errno(EPERM)
            }
        }
        208 => {
            let gid = unsafe { state_mut().gid };
            if frame.rdi == 0 && frame.rsi == 0 && frame.rdx == 0 {
                0
            } else if copy_user_out(frame.rdi, &gid.to_le_bytes()).is_err()
                || copy_user_out(frame.rsi, &gid.to_le_bytes()).is_err()
                || copy_user_out(frame.rdx, &gid.to_le_bytes()).is_err()
            {
                neg_errno(EFAULT)
            } else {
                0
            }
        }
        209 => chmod_like(-1, frame.rsi, true, b""),
        210 => {
            let value = frame.rdi as u32;
            let cur = unsafe { state_mut().uid };
            if value == cur || cur == 0 {
                0
            } else {
                neg_errno(EPERM)
            }
        }
        211 => {
            let value = frame.rdi as u32;
            let cur = unsafe { state_mut().gid };
            if value == cur || cur == 0 {
                0
            } else {
                neg_errno(EPERM)
            }
        }
        212 => {
            let value = frame.rdi as u32;
            let cur = unsafe { state_mut().uid };
            if value == cur || cur == 0 {
                unsafe {
                    state_mut().fsuid = value;
                }
                0
            } else {
                neg_errno(EPERM)
            }
        }
        213 => {
            let target = frame.rdi as u64;
            let pgid = frame.rsi as u64;
            let self_pid = unsafe { state_mut().pid };
            if target != 0 && target != self_pid {
                neg_errno(ESRCH)
            } else if pgid != 0 && pgid != self_pid {
                neg_errno(EPERM)
            } else {
                unsafe {
                    state_mut().pgid = self_pid;
                }
                0
            }
        }
        214 => unsafe { state_mut().ppid },
        215 => unsafe { state_mut().euid as u64 },
        216 => unsafe { state_mut().egid as u64 },
        218 => unsafe {
            state_mut().tid_address = frame.rdi;
            state_mut().pid
        },
        219 => neg_errno(EINTR),
        220 => neg_errno(ENOSYS),
        221 => {
            if frame.rdi < 0 || frame.rdi as usize >= FILE_FD_MAX {
                return_frame_result!(frame, neg_errno(EBADF))
            }
            match frame.rsi {
                POSIX_FADV_NORMAL
                | POSIX_FADV_RANDOM
                | POSIX_FADV_SEQUENTIAL
                | POSIX_FADV_WILLNEED
                | POSIX_FADV_DONTNEED
                | POSIX_FADV_NOREUSE => 0,
                _ => neg_errno(EINVAL),
            }
        }
        222 | 223 | 224 | 225 | 226 => neg_errno(ENOSYS),
        227 => neg_errno(EPERM),
        228 => clock_gettime_impl(frame.rdi, frame.rsi),
        229 => clock_getres_impl(frame.rdi, frame.rsi),
        230 => {
            let clock_id = frame.rdi;
            let flags = frame.rsi;
            let request = frame.rdx;
            let remaining = frame.r10;
            let _ = clock_id;
            if flags & !TIMER_ABSTIME != 0 {
                neg_errno(EINVAL)
            } else if request == 0 {
                0
            } else {
                let ticks = match timeout_ticks_from_timespec(request) {
                    Ok(Some(value)) => value,
                    Ok(None) => 0,
                    Err(code) => return_frame_result!(frame, code),
                };
                let target = if flags & TIMER_ABSTIME != 0 {
                    ticks
                } else {
                    current_ticks().saturating_add(ticks)
                };
                while current_ticks() < target {
                    unsafe {
                        asm!("sti; hlt", options(nomem, nostack, preserves_flags));
                    }
                }
                if remaining != 0 {
                    let _ = copy_user_out(remaining, &[0u8; 16]);
                }
                0
            }
        }
        232 => {
            if frame.rdi < 0 {
                neg_errno(EINVAL)
            } else {
                epoll_alloc(0) as u64
            }
        }
        233 => epoll_wait_impl(
            frame.rdi as i32,
            frame.rsi,
            frame.rdx as i32,
            frame.r10 as i32,
        ),
        234 => neg_errno(ESRCH),
        235 => neg_errno(EROFS),
        237 | 238 | 239 => 0,
        240 | 241 | 242 | 243 | 244 | 245 | 246 => neg_errno(ENOSYS),
        247 => crate::process::wait4(-1, frame.rsi, 0, 0),
        248 | 249 | 250 => neg_errno(ENOSYS),
        251 => 0,
        252 => 4,
        253 => neg_errno(ENOSYS),
        254 => 1,
        255 => 0,
        256 => neg_errno(ENOSYS),
        258 => {
            let mut path = [0u8; 160];
            match read_c_string(frame.rsi, &mut path) {
                Ok(_) => neg_errno(EROFS),
                Err(code) => neg_errno(code),
            }
        }
        259 => {
            let mut path = [0u8; 160];
            match read_c_string(frame.rsi, &mut path) {
                Ok(_) => neg_errno(EROFS),
                Err(code) => neg_errno(code),
            }
        }
        260 => {
            let mut path = [0u8; 160];
            match read_c_string(frame.rsi, &mut path) {
                Ok(_) => neg_errno(EROFS),
                Err(code) => neg_errno(code),
            }
        }
        261 => {
            let mut path = [0u8; 160];
            match read_c_string(frame.rsi, &mut path) {
                Ok(_) => neg_errno(EROFS),
                Err(code) => neg_errno(code),
            }
        }
        262 => {
            let mut path = [0u8; 160];
            if read_c_string(frame.rsi, &mut path).is_err() {
                neg_errno(EFAULT)
            } else {
                let mut len = 0usize;
                while len < path.len() && path[len] != 0 {
                    len += 1;
                }
                let p = &path[..len];
                if !path_exists(p) {
                    neg_errno(ENOENT)
                } else {
                    fill_stat(
                        frame.rdx,
                        path_mode(p),
                        crate::velf::store::find(p)
                            .map(|x| x.bytes.len() as u64)
                            .or_else(|| text_file(p).map(|x| x.len() as u64))
                            .unwrap_or(0),
                        p,
                    )
                }
            }
        }
        263 => {
            let mut path = [0u8; 160];
            match read_c_string(frame.rsi, &mut path) {
                Ok(_) => neg_errno(EROFS),
                Err(code) => neg_errno(code),
            }
        }
        264 => {
            let mut old = [0u8; 160];
            let mut new = [0u8; 160];
            if read_c_string(frame.rsi, &mut old).is_err()
                || read_c_string(frame.r10, &mut new).is_err()
            {
                neg_errno(EFAULT)
            } else {
                neg_errno(EROFS)
            }
        }
        265 => {
            let mut old = [0u8; 160];
            let mut new = [0u8; 160];
            if read_c_string(frame.rsi, &mut old).is_err()
                || read_c_string(frame.r10, &mut new).is_err()
            {
                neg_errno(EFAULT)
            } else {
                neg_errno(EROFS)
            }
        }
        266 => {
            let mut target = [0u8; 160];
            let mut path = [0u8; 160];
            if read_c_string(frame.rdi, &mut target).is_err()
                || read_c_string(frame.rdx, &mut path).is_err()
            {
                neg_errno(EFAULT)
            } else {
                neg_errno(EROFS)
            }
        }
        268 => neg_errno(EPERM),
        270 => pselect_impl(frame.rdi, frame.rsi, frame.rdx, frame.r10, frame.r8),
        271 => {
            let user_fds = frame.rdi;
            let count = frame.rsi as usize;
            let timeout = frame.rdx;
            let _sigmask = frame.r10;
            let start = current_ticks();
            let deadline = if timeout == u64::MAX {
                None
            } else {
                Some(
                    start.saturating_add(
                        timeout_ticks_from_timespec(timeout)
                            .unwrap_or(Some(0))
                            .unwrap_or(0),
                    ),
                )
            };
            loop {
                match poll_scan(user_fds, count) {
                    Ok(value) if value != 0 => break value as u64,
                    Ok(_) => {}
                    Err(code) => break code,
                }
                if let Some(deadline) = deadline {
                    if current_ticks() >= deadline {
                        break 0;
                    }
                } else if timeout != 0 {
                    break 0;
                } else {
                    break 0;
                }
                unsafe {
                    asm!("sti; hlt", options(nomem, nostack, preserves_flags));
                }
            }
        }
        272 => 0,
        273 => unsafe {
            if frame.rsi < 24 && frame.rdi != 0 {
                neg_errno(EINVAL)
            } else {
                state_mut().robust_list = frame.rdi;
                state_mut().robust_len = frame.rsi;
                0
            }
        },
        274 => unsafe {
            let pid_arg = frame.rdi as i32;
            if pid_arg != 0 && pid_arg as u64 != state_mut().pid {
                neg_errno(ESRCH)
            } else {
                if copy_user_out(frame.rsi, &state_mut().robust_list.to_le_bytes()).is_err()
                    || copy_user_out(frame.rdx, &state_mut().robust_len.to_le_bytes()).is_err()
                {
                    neg_errno(EFAULT)
                } else {
                    0
                }
            }
        },
        275 => splice_impl(
            frame.rdi as i32,
            frame.rsi,
            frame.rdx as i32,
            frame.r10,
            frame.r8 as usize,
            frame.r9,
        ),
        276 => tee_impl(
            frame.rdi as i32,
            frame.rsi as i32,
            frame.rdx as usize,
            frame.r10,
        ),
        277 => 0,
        278 => vmsplice_impl(frame.rdi as i32, frame.rsi, frame.rdx as usize, frame.r10),
        279 | 280 => neg_errno(ENOSYS),
        281 => epoll_wait_impl(
            frame.rdi as i32,
            frame.rsi,
            frame.rdx as i32,
            frame.r10 as i32,
        ),
        282 => signalfd_alloc(frame.rdi, 0) as u64,
        283 => timerfd_alloc(frame.rdi, frame.rsi) as u64,
        284 => eventfd_alloc(frame.rdi, 0, false) as u64,
        285 => {
            let fd = frame.rdi as i32;
            let Ok(descriptor) = fd_snapshot(fd) else {
                return_frame_result!(frame, neg_errno(EBADF))
            };
            if descriptor.kind == FILE_KIND_BINARY || descriptor.kind == FILE_KIND_TEXT {
                neg_errno(EROFS)
            } else {
                0
            }
        }
        286 => timerfd_settime_impl(frame.rdi as i32, frame.rsi, frame.rdx, frame.r10),
        287 => timerfd_gettime_impl(frame.rdi as i32, frame.rsi),
        288 => neg_errno(EAGAIN),
        289 => signalfd_alloc(frame.rdi, frame.rsi) as u64,
        290 => eventfd_alloc(frame.rdi, frame.rsi, false) as u64,
        291 => epoll_alloc(frame.rdi) as u64,
        292 => fd_duplicate_exact(
            frame.rdi as i32,
            frame.rsi as i32,
            frame.rdx & O_CLOEXEC != 0,
        ),
        294 => neg_errno(ENOSYS),
        297 => neg_errno(ESRCH),
        298 => neg_errno(EPERM),
        299 => neg_errno(EAGAIN),
        300 => neg_errno(EPERM),
        301 => neg_errno(EPERM),
        302 => {
            let resource = frame.rsi;
            fill_rlimit(frame.r9, resource)
        }
        303 => neg_errno(ENOSYS),
        304 => neg_errno(ENOSYS),
        305 => 0,
        306 => 0,
        307 => neg_errno(EAGAIN),
        308 | 309 => neg_errno(EPERM),
        310 => process_vm_impl(
            frame.rdi as i32,
            frame.rsi,
            frame.rdx as usize,
            frame.r10 as i32 as u64,
            frame.r8,
            frame.r9 as usize,
            true,
        ),
        311 => process_vm_impl(
            frame.rdi as i32,
            frame.rsi,
            frame.rdx as usize,
            frame.r10 as i32 as u64,
            frame.r8,
            frame.r9 as usize,
            false,
        ),
        312 => 0,
        313 => neg_errno(EPERM),
        314 | 315 => 0,
        316 => {
            let mut old = [0u8; 160];
            let mut new = [0u8; 160];
            if read_c_string(frame.rsi, &mut old).is_err()
                || read_c_string(frame.r10, &mut new).is_err()
            {
                neg_errno(EFAULT)
            } else {
                neg_errno(EROFS)
            }
        }
        317 => neg_errno(EPERM),
        318 => getrandom_impl(frame.rdi, frame.rsi as usize, frame.rdx),
        319 => memfd_create_impl(frame.rdi, frame.rsi),
        320 => neg_errno(EPERM),
        321 => neg_errno(EPERM),
        322 => execveat_impl(frame.rdi as i32, frame.rsi, frame.rdx, frame.r10, frame.r8),
        323 => neg_errno(ENOSYS),
        324 => 0,
        325 => 0,
        326 => copy_file_range_impl(
            frame.rdi as i32,
            frame.rsi,
            frame.rdx as i32,
            frame.r10,
            frame.r8 as usize,
            frame.r9,
        ),
        329 => 0,
        330 => 1,
        331 => 0,
        332 => {
            let mut path = [0u8; 160];
            let path_addr = frame.rsi;
            match read_c_string(path_addr, &mut path) {
                Ok(length) => {
                    let p = &path[..length];
                    if !path_exists(p) {
                        neg_errno(ENOENT)
                    } else {
                        fill_statx(
                            frame.r10,
                            p,
                            path_mode(p),
                            crate::velf::store::find(p)
                                .map(|x| x.bytes.len() as u64)
                                .or_else(|| text_file(p).map(|x| x.len() as u64))
                                .unwrap_or(0),
                        )
                    }
                }
                Err(code) => neg_errno(code),
            }
        }
        333 => neg_errno(ENOSYS),
        334 => {
            unsafe {
                let state = state_mut();
                state.rseq = frame.rdi;
                state.rseq_len = frame.rsi as u32;
            }
            0
        }
        424 => {
            let target = frame.rdi as u64;
            let self_pid = unsafe { state_mut().pid };
            if target != self_pid {
                return_frame_result!(frame, neg_errno(ESRCH))
            }
            if frame.rdx > 64 {
                return_frame_result!(frame, neg_errno(EINVAL))
            }
            if frame.rdx == 0 {
                return_frame_result!(frame, 0)
            }
            if raise_signal(frame.rdx) {
                0
            } else {
                neg_errno(EINVAL)
            }
        }
        425 | 426 | 427 => neg_errno(ENOSYS),
        428 | 429 | 430 | 431 | 432 | 433 => neg_errno(EPERM),
        434 => {
            if frame.rdi as u64 == unsafe { state_mut().pid } {
                pidfd_impl(unsafe { state_mut().pid })
            } else {
                neg_errno(ESRCH)
            }
        }
        435 => clone3_impl(frame.rdi),
        436 => close_range_impl(frame.rdi as u32, frame.rsi as u32, frame.rdx),
        437 => openat2_impl(frame.rdi as i32, frame.rsi, frame.rdx),
        438 => neg_errno(EPERM),
        439 => {
            let mut path = [0u8; 160];
            match read_c_string(frame.rsi, &mut path) {
                Ok(length) => access_path(&path[..length], frame.rdx),
                Err(code) => neg_errno(code),
            }
        }
        440 => neg_errno(ENOSYS),
        441 => epoll_wait_impl(frame.rdi as i32, frame.rsi, frame.rdx as i32, -1),
        442 | 443 | 444 | 445 | 446 | 447 | 448 => neg_errno(EPERM),
        449 => futex_waitv_impl(frame.rdi, frame.rsi, frame.rdx, frame.r10, frame.r8),
        450 => 0,
        _ => compat_syscall(number, frame)
    };
    frame.rax = result;
    deliver_pending_signal(frame);
}

core::arch::global_asm!(
    r#"
.text
.global vibux_syscall_entry
.type vibux_syscall_entry,@function
vibux_syscall_entry:
    cmpb $1, vibux_vfork_child_active(%rip)
    je vibux_child_syscall_entry
    cmpq $60, %rax
    je vibux_syscall_exit_fast
    cmpq $231, %rax
    je vibux_syscall_exit_fast
    movq %rsp, vibux_syscall_saved_rsp(%rip)
    movq %rsp, SYSCALL_SAVED_RSP(%rip)
    leaq vibux_syscall_stack_top(%rip), %rsp
    pushq %rcx
    pushq %r11
    pushq %rax
    pushq %rdx
    pushq %rbx
    pushq %rbp
    pushq %rsi
    pushq %rdi
    pushq %r8
    pushq %r9
    pushq %r10
    pushq %r12
    pushq %r13
    pushq %r14
    pushq %r15
    subq $8, %rsp
    leaq 8(%rsp), %rdi
    call syscall_dispatch
    addq $8, %rsp
    cmpb $0, vibux_exit_requested(%rip)
    jne vibux_syscall_exit
    popq %r15
    popq %r14
    popq %r13
    popq %r12
    popq %r10
    popq %r9
    popq %r8
    popq %rdi
    popq %rsi
    popq %rbp
    popq %rbx
    popq %rdx
    popq %rax
    popq %r11
    popq %rcx
    movq vibux_syscall_saved_rsp(%rip), %rsp
    sysretq
vibux_child_syscall_entry:
    cmpq $60, %rax
    je vibux_vfork_exit_fast
    cmpq $231, %rax
    je vibux_vfork_exit_fast
    movq %rsp, vibux_child_syscall_saved_rsp(%rip)
    leaq vibux_child_syscall_stack_top(%rip), %rsp
    pushq %rcx
    pushq %r11
    pushq %rax
    pushq %rdx
    pushq %rbx
    pushq %rbp
    pushq %rsi
    pushq %rdi
    pushq %r8
    pushq %r9
    pushq %r10
    pushq %r12
    pushq %r13
    pushq %r14
    pushq %r15
    subq $8, %rsp
    leaq 8(%rsp), %rdi
    call syscall_dispatch
    addq $8, %rsp
    cmpb $0, vibux_exit_requested(%rip)
    je 1f
    movl vibux_exit_code(%rip), %edi
    jmp vibux_vfork_exit_fast
1:
    popq %r15
    popq %r14
    popq %r13
    popq %r12
    popq %r10
    popq %r9
    popq %r8
    popq %rdi
    popq %rsi
    popq %rbp
    popq %rbx
    popq %rdx
    popq %rax
    popq %r11
    popq %rcx
    movq vibux_child_syscall_saved_rsp(%rip), %rsp
    sysretq
vibux_syscall_exit:
    leaq vibux_user_return_stack_top-56(%rip), %rsp
    jmp vibux_user_return
.global vibux_enter_user
.type vibux_enter_user,@function
vibux_enter_user:
    movq %rsp, vibux_user_host_rsp(%rip)
    leaq vibux_user_return_stack_top(%rip), %rsp
    leaq vibux_user_return(%rip), %r11
    pushq %r11
    pushq %r15
    pushq %r14
    pushq %r13
    pushq %r12
    pushq %rbx
    pushq %rbp
    movq $0x1B, %r11
    pushq %r11
    pushq %rsi
    movq $0x202, %r11
    pushq %r11
    movq $0x23, %r11
    pushq %r11
    pushq %rdi
    xorq %rax, %rax
    xorq %rbx, %rbx
    xorq %rcx, %rcx
    xorq %rdx, %rdx
    xorq %rsi, %rsi
    xorq %rdi, %rdi
    xorq %rbp, %rbp
    xorq %r8, %r8
    xorq %r9, %r9
    xorq %r10, %r10
    xorq %r12, %r12
    xorq %r13, %r13
    xorq %r14, %r14
    xorq %r15, %r15
    movw $0x3F8, %dx
    movb $0x49, %al
    outb %al, %dx
    iretq
vibux_user_return:
    popq %rbp
    popq %rbx
    popq %r12
    popq %r13
    popq %r14
    popq %r15
    addq $8, %rsp
    movq vibux_user_host_rsp(%rip), %rsp
    movl vibux_exit_code(%rip), %eax
    ret
.global vibux_enter_vfork_child
.type vibux_enter_vfork_child,@function
vibux_enter_vfork_child:
    movq %rsp,vibux_vfork_host_rsp(%rip)
    movq vibux_syscall_saved_rsp(%rip), %r11
    movq %r11,vibux_vfork_parent_user_rsp(%rip)
    movq %rsi,%rax
    movq 0(%rdi),%r15
    movq 8(%rdi),%r14
    movq 16(%rdi),%r13
    movq 24(%rdi),%r12
    movq 40(%rdi),%r9
    movq 48(%rdi),%r8
    movq 64(%rdi),%rsi
    movq 72(%rdi),%rbp
    movq 80(%rdi),%rbx
    movq 88(%rdi),%rdx
    movq 104(%rdi),%r11
    movq 112(%rdi),%rcx
    movq 32(%rdi),%r10
    movq 56(%rdi),%rdi
    movq %rax,%rsp
    xorq %rax,%rax
    sysretq
.size vibux_enter_vfork_child,.-vibux_enter_vfork_child
.global vibux_enter_vfork_exec
.type vibux_enter_vfork_exec,@function
vibux_enter_vfork_exec:
    leaq vibux_vfork_exec_stack_top(%rip), %rsp
    pushq $0x1B
    pushq %rsi
    movq $0x202, %r11
    pushq %r11
    pushq $0x23
    pushq %rdi
    xorq %rax, %rax
    xorq %rbx, %rbx
    xorq %rcx, %rcx
    xorq %rdx, %rdx
    xorq %rsi, %rsi
    xorq %rdi, %rdi
    xorq %rbp, %rbp
    xorq %r8, %r8
    xorq %r9, %r9
    xorq %r10, %r10
    xorq %r11, %r11
    xorq %r12, %r12
    xorq %r13, %r13
    xorq %r14, %r14
    xorq %r15, %r15
    iretq
.section .rodata
.align 8
USER_CS:
.quad 0x23
USER_SS:
.quad 0x1B
USER_RFLAGS:
.quad 0x202
.section .text
.global vibux_syscall_exit_fast
.type vibux_syscall_exit_fast,@function
vibux_syscall_exit_fast:
    cmpb $1, vibux_vfork_child_active(%rip)
    je vibux_vfork_exit_fast
    movw $0x3F8, %dx
    movb $0x58, %al
    outb %al, %dx
    movq %rcx, %r8
    movq %rax, %rcx
    movq %rsp, %rsi
    movq vibux_user_host_rsp(%rip), %rdx
    movq (%rdx), %rdx
    leaq vibux_syscall_stack_top(%rip), %rsp
    andq $-16, %rsp
    call vibux_debug_exit_fast
    movq vibux_user_return_stack_top-56(%rip), %rbp
    movq vibux_user_return_stack_top-48(%rip), %rbx
    movq vibux_user_return_stack_top-40(%rip), %r12
    movq vibux_user_return_stack_top-32(%rip), %r13
    movq vibux_user_return_stack_top-24(%rip), %r14
    movq vibux_user_return_stack_top-16(%rip), %r15
    movq vibux_user_host_rsp(%rip), %rsp
    ret
.global vibux_exec_failure_fast
.type vibux_exec_failure_fast,@function
vibux_exec_failure_fast:
    jmp vibux_vfork_exit_fast
vibux_vfork_exit_fast:
    movl %edi, %r12d
    leaq vibux_vfork_return_stack_top(%rip), %rsp
    andq $-16, %rsp
    subq $8, %rsp
    call vibux_vfork_child_exit
    addq $8, %rsp
    movq vibux_vfork_parent_user_rsp(%rip), %r11
    movq %r11,vibux_syscall_saved_rsp(%rip)
    movq vibux_vfork_host_rsp(%rip), %rsp
    movl %r12d, %eax
    ret
.section .bss
.align 16
vibux_syscall_saved_rsp:
.quad 0
vibux_syscall_stack:
.skip 65536
vibux_syscall_stack_top:
.align 16
vibux_child_syscall_stack:
.skip 65536
vibux_child_syscall_stack_top:
.align 16
vibux_vfork_return_stack:
.skip 4096
vibux_vfork_return_stack_top:
.align 16
vibux_vfork_exec_stack:
.skip 4096
vibux_vfork_exec_stack_top:
.align 16
vibux_vfork_host_rsp:
.quad 0
vibux_vfork_parent_user_rsp:
.quad 0
vibux_child_syscall_saved_rsp:
.quad 0
vibux_user_return_stack:
.skip 4096
vibux_user_return_stack_top:
vibux_user_host_rsp:
.quad 0
"#,
    options(att_syntax)
);

#[unsafe(no_mangle)]
pub static mut vibux_exit_requested: u8 = 0;
#[unsafe(no_mangle)]
pub static mut vibux_exit_code: i32 = 0;