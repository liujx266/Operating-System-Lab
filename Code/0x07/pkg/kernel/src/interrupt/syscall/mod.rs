use crate::{memory::gdt, proc::*};
// use crate::proc::processor; // No longer needed here as sys_fork was simplified
use alloc::format;
use x86_64::{
    structures::idt::{InterruptDescriptorTable, InterruptStackFrame},
    VirtAddr, // 导入 VirtAddr
};

// NOTE: import `ysos_syscall` package as `syscall_def` in Cargo.toml
use ysos_syscall::Syscall; // 修正导入的包名
use crate::proc::sync::sys_sem; // 导入新的 sys_sem 处理函数

mod service;
use super::consts;

// FIXME: write syscall service handler in `service.rs`
use service::*;

pub unsafe fn register_idt(idt: &mut InterruptDescriptorTable) {
    unsafe { idt[consts::Interrupts::Syscall as u8] // 使用 u8 作为索引类型
        .set_handler_addr(VirtAddr::new(syscall_handler as *const () as u64)) // 安全地转换函数项为 VirtAddr
        .set_stack_index(gdt::SYSCALL_IST_INDEX) // 设置独立的系统调用栈 (假设 GDT 中定义了 SYSCALL_IST_INDEX)
        .set_privilege_level(x86_64::PrivilegeLevel::Ring3) }; // 设置 DPL 为 3，允许用户态调用
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

pub fn sys_fork(context: &mut ProcessContext) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let manager = get_process_manager();
        
        // Capture the parent's PID before its context is saved.
        let parent_pid = crate::proc::processor::get_pid(); // Use full path for clarity
        
        // Save current process (parent) context.
        // After this, parent_proc.inner.context holds the saved state.
        // Parent's status is still Running.
        manager.save_current(context);
        
        // Create child process.
        // - Child's initial context will have rax = 0 and status = Ready.
        // - Parent's *saved* context (in parent_proc.inner.context.regs.rax) is set to child_pid.
        let child_pid = manager.fork();
        
        // Set the return value for the parent process in the *current live* context.
        // This is what the fork syscall returns immediately in the parent.
        context.set_rax(child_pid.0 as usize);
        
        // Set parent process to Ready and add it to the ready queue.
        let parent_process_obj = manager.get_proc(&parent_pid)
            .expect("Parent process not found after fork");
        parent_process_obj.write().status = ProgramStatus::Ready; // ProgramStatus comes from proc::*
        manager.push_ready(parent_pid);
        
        // Add child process to the ready queue.
        manager.push_ready(child_pid);
        
        // Switch to the next process. `context` will be updated to the next process's context.
        let _next_pid = manager.switch_next(context);
        // The return value (rax) for child (0) or parent (child_pid) is already set
        // in their respective saved contexts and will be restored by switch_next.
    });
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

        Syscall::Fork => {
            sys_fork(context);
        },

        // path: &str (ptr: arg0 as *const u8, len: arg1)
        Syscall::ListDir => {
            list_dir(&args);
            context.set_rax(0);
        },
        // path: &str (ptr: arg0 as *const u8, len: arg1) -> fd: u8
        Syscall::Open => {
            context.set_rax(sys_open(&args));
        },
        // fd: arg0 as u8 -> status: isize
        Syscall::Close => {
            context.set_rax(sys_close(&args));
        },

        // ----------------------------------------------------
        // NOTE: following syscall examples are implemented
        // ----------------------------------------------------

        // layout: arg0 as *const Layout -> ptr: *mut u8
        Syscall::Allocate => context.set_rax(sys_allocate(&args)),
        // ptr: arg0 as *mut u8
        Syscall::Deallocate => sys_deallocate(&args),
        Syscall::Sem => {
            // 调用 proc::sync 模块中的 sys_sem 函数
            sys_sem(&args, context);
        },
        // addr: arg0 as Option<VirtAddr> -> new_addr: VirtAddr
        Syscall::Brk => {
            context.set_rax(sys_brk(&args));
        },
        // Unknown
        Syscall::Unknown => {
            warn!(
                "Unhandled syscall: ID {}, Args: ({}, {}, {})",
                context.regs.rax, args.arg0, args.arg1, args.arg2
            );
            // Optionally set an error code in rax for unknown syscalls
            // context.set_rax(ysos_syscall::SysErr::NotSupported as usize); // Example
        }
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
