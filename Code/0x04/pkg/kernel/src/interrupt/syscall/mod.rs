use crate::{memory::gdt, proc::*};
use alloc::format;
use x86_64::{
    structures::idt::{InterruptDescriptorTable, InterruptStackFrame},
    VirtAddr, // 导入 VirtAddr
};

// NOTE: import `ysos_syscall` package as `syscall_def` in Cargo.toml
use ysos_syscall::Syscall; // 修正导入的包名

mod service;
use super::consts;

// FIXME: write syscall service handler in `service.rs`
use service::*;

pub unsafe fn register_idt(idt: &mut InterruptDescriptorTable) {
    idt[consts::Interrupts::Syscall as u8] // 使用 u8 作为索引类型
        .set_handler_addr(unsafe { VirtAddr::new(syscall_handler as *const () as u64) }) // 安全地转换函数项为 VirtAddr
        .set_stack_index(gdt::SYSCALL_IST_INDEX) // 设置独立的系统调用栈 (假设 GDT 中定义了 SYSCALL_IST_INDEX)
        .set_privilege_level(x86_64::PrivilegeLevel::Ring3); // 设置 DPL 为 3，允许用户态调用
}

pub extern "C" fn syscall(mut context: ProcessContext) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        super::syscall::dispatcher(&mut context);
    });
}

as_handler!(syscall);

#[derive(Clone, Debug)]
pub struct SyscallArgs {
    pub syscall: Syscall,
    pub arg0: usize,
    pub arg1: usize,
    pub arg2: usize,
}

pub fn dispatcher(context: &mut ProcessContext) {
    let args = super::syscall::SyscallArgs::new(
        Syscall::from(context.regs.rax),
        context.regs.rdi,
        context.regs.rsi,
        context.regs.rdx,
    );

    // NOTE: you may want to trace syscall arguments
    // trace!("{}", args);

    match args.syscall {
        // fd: arg0 as u8, buf: &mut [u8] (ptr: arg1 as *mut u8, len: arg2)
        Syscall::Read => {
            context.set_rax(sys_read(&args));
        },
        // fd: arg0 as u8, buf: &[u8] (ptr: arg1 as *const u8, len: arg2)
        Syscall::Write => {
            context.set_rax(sys_write(&args));
        },
        // None -> pid: u16
        Syscall::GetPid => {
            context.set_rax(sys_getpid(&args));
        },

        // path: &str (ptr: arg0 as *const u8, len: arg1) -> pid: u16
        Syscall::Spawn => {
            context.set_rax(spawn_process(&args));
        },
        // ret: arg0 as isize
        Syscall::Exit => {
            exit_process(&args, context);
            // 注意：此处不需要设置返回值，因为进程会退出
        },
        // pid: arg0 as u16 -> status: isize
        Syscall::WaitPid => {
            context.set_rax(sys_waitpid(&args));
        },

        // None
        Syscall::Stat => {
            list_process();
            context.set_rax(0); // 返回0表示成功
        },
        // None
        Syscall::ListApp => {
            crate::proc::list_app();
            context.set_rax(0); // 返回0表示成功
        },

        // ----------------------------------------------------
        // NOTE: following syscall examples are implemented
        // ----------------------------------------------------

        // layout: arg0 as *const Layout -> ptr: *mut u8
        Syscall::Allocate => context.set_rax(sys_allocate(&args)),
        // ptr: arg0 as *mut u8
        Syscall::Deallocate => sys_deallocate(&args),
        // Unknown
        Syscall::Unknown => warn!("Unhandled syscall: {:x?}", context.regs.rax),
    }
}

impl SyscallArgs {
    pub fn new(syscall: Syscall, arg0: usize, arg1: usize, arg2: usize) -> Self {
        Self {
            syscall,
            arg0,
            arg1,
            arg2,
        }
    }
}

impl core::fmt::Display for SyscallArgs {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(
            f,
            "SYSCALL: {:<10} (0x{:016x}, 0x{:016x}, 0x{:016x})",
            format!("{:?}", self.syscall),
            self.arg0,
            self.arg1,
            self.arg2
        )
    }
}
