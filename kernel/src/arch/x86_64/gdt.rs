use core::arch::asm;
use core::mem::size_of;
use core::ptr::{addr_of, addr_of_mut, write};

pub const KERNEL_CODE_SELECTOR: u16 = 0x08;
pub const KERNEL_DATA_SELECTOR: u16 = 0x10;
pub const USER_DATA_SELECTOR: u16 = 0x18;
pub const USER_CODE_SELECTOR: u16 = 0x20;
pub const TSS_SELECTOR: u16 = 0x28;

#[repr(C, packed(1))]
struct TaskStateSegment {
    reserved_1: u32,
    rsp0: u64,
    rsp1: u64,
    rsp2: u64,
    reserved_2: u64,
    ist1: u64,
    ist2: u64,
    ist3: u64,
    ist4: u64,
    ist5: u64,
    ist6: u64,
    ist7: u64,
    reserved_3: u64,
    reserved_4: u16,
    io_map_base: u16,
}

#[repr(C, packed(1))]
struct Gdtr {
    limit: u16,
    base: u64,
}

static mut GDT: [u64; 7] = [0; 7];

static mut TSS: TaskStateSegment = TaskStateSegment {
    reserved_1: 0,
    rsp0: 0,
    rsp1: 0,
    rsp2: 0,
    reserved_2: 0,
    ist1: 0,
    ist2: 0,
    ist3: 0,
    ist4: 0,
    ist5: 0,
    ist6: 0,
    ist7: 0,
    reserved_3: 0,
    reserved_4: 0,
    io_map_base: 0,
};

#[repr(align(16))]
struct Stack([u8; 16 * 1024]);

static mut DOUBLE_FAULT_STACK: Stack = Stack([0; 16 * 1024]);
static mut SYSCALL_STACK: Stack = Stack([0; 16 * 1024]);

const _: () = assert!(size_of::<TaskStateSegment>() == 104);

fn code_descriptor(access: u64) -> u64 {
    (access << 40) | (0b1010u64 << 52)
}

fn data_descriptor(access: u64) -> u64 {
    access << 40
}

fn make_tss_descriptor(base: u64, limit: u32) -> (u64, u64) {
    let low = (limit as u64 & 0xffff)
        | ((base & 0x00ff_ffff) << 16)
        | (0x89u64 << 40)
        | (((limit as u64 >> 16) & 0x0f) << 48)
        | (((base >> 24) & 0xff) << 56);

    let high = base >> 32;

    (low, high)
}

pub fn init() {
    unsafe {
        let df_stack_top = addr_of_mut!(DOUBLE_FAULT_STACK.0) as usize + 16 * 1024;

        let syscall_stack_top = addr_of_mut!(SYSCALL_STACK.0) as usize + 16 * 1024;

        let tss_ptr = addr_of_mut!(TSS);

        write(addr_of_mut!((*tss_ptr).rsp0), syscall_stack_top as u64);

        write(addr_of_mut!((*tss_ptr).ist1), df_stack_top as u64);

        write(addr_of_mut!((*tss_ptr).ist2), syscall_stack_top as u64);

        write(
            addr_of_mut!((*tss_ptr).io_map_base),
            size_of::<TaskStateSegment>() as u16,
        );

        let gdt = addr_of_mut!(GDT).cast::<u64>();

        write(gdt.add(0), 0);
        write(gdt.add(1), code_descriptor(0x9a));
        write(gdt.add(2), data_descriptor(0x92));
        write(gdt.add(3), data_descriptor(0xf2));
        write(gdt.add(4), code_descriptor(0xfa));

        let (tss_low, tss_high) = make_tss_descriptor(
            tss_ptr as usize as u64,
            (size_of::<TaskStateSegment>() - 1) as u32,
        );

        write(gdt.add(5), tss_low);
        write(gdt.add(6), tss_high);

        let gdtr = Gdtr {
            limit: (size_of::<[u64; 7]>() - 1) as u16,
            base: gdt as usize as u64,
        };

        asm!(
            "lgdt [{gdtr}]",
            "push {code}",
            "lea rax, [rip + 2f]",
            "push rax",
            "retfq",
            "2:",
            "mov ax, {data}",
            "mov ds, ax",
            "mov es, ax",
            "mov ss, ax",
            "mov ax, {tss}",
            "ltr ax",
            gdtr = in(reg) addr_of!(gdtr),
            code = const KERNEL_CODE_SELECTOR,
            data = const KERNEL_DATA_SELECTOR,
            tss = const TSS_SELECTOR,
            lateout("rax") _,
            options(preserves_flags),
        );
    }
}

pub fn syscall_stack_top() -> u64 {
    unsafe { addr_of_mut!(SYSCALL_STACK.0) as usize as u64 + 16 * 1024 }
}
