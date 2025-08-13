mod context;
mod data;
mod manager;
mod paging;
mod pid;
mod process;
mod processor;
mod vm;

use manager::*;
pub use manager::get_process_manager; // Publicly re-export get_process_manager
use process::*;
use vm::ProcessVm;

use alloc::string::String;
pub use context::ProcessContext;
pub use paging::PageTableContext;
pub use data::ProcessData;
pub use pid::ProcessId;

use x86_64::structures::idt::PageFaultErrorCode;
use x86_64::VirtAddr;
pub const KERNEL_PID: ProcessId = ProcessId(1);

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ProgramStatus {
    Running,
    Ready,
    Blocked,
    Dead,
}

/// init process manager
pub fn init() {
    let proc_vm = ProcessVm::new(PageTableContext::new()).init_kernel_vm();

    trace!("Init kernel vm: {:#?}", proc_vm);

    // kernel process
    let kproc = {
        // 创建内核进程数据
        let proc_data = ProcessData::new();
        
        // 创建内核进程，强制使用KERNEL_PID
        let process = Process::new_with_pid(
            KERNEL_PID,
            String::from("kernel"),
            None, // 内核进程没有父进程
            Some(proc_vm),
            Some(proc_data)
        );
        
        // 确认PID设置正确
        assert_eq!(process.pid(), KERNEL_PID, "Kernel PID mismatch");
        
        process
    };
    manager::init(kproc);

    info!("Process Manager Initialized.");
}

pub fn switch(context: &mut ProcessContext) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let process_manager = get_process_manager();
        // 保存当前进程上下文
        process_manager.save_current(context);
        
        // 获取当前进程，检查状态并添加到就绪队列
        let pro = process_manager.current();
        let current_pid = pro.pid();
        
        // 使用读取锁检查进程状态
        let status = pro.read().status();
        
        // 如果进程不是Dead或Blocked状态，将其添加到就绪队列
        if status != ProgramStatus::Dead && status != ProgramStatus::Blocked {
            // 将进程加入就绪队列
            process_manager.push_ready(current_pid);
            
            // 获取写锁并修改状态
            let mut proc_guard = pro.write();
            proc_guard.pause(); // 设置为Ready状态
            proc_guard.tick();  // 更新进程的时间片计数
        }
        
        // 切换到下一个进程
        let next_pid = process_manager.switch_next(context);
        
        // 完全禁用进程切换日志，即使进程ID发生变化也不输出
        // 只在需要调试时开启
        /*
        static mut LAST_SWITCHED_PID: Option<ProcessId> = None;
        unsafe {
            if LAST_SWITCHED_PID != Some(next_pid) && next_pid != KERNEL_PID {
                trace!("Switch to process: {}", next_pid);
                LAST_SWITCHED_PID = Some(next_pid);
            }
        }
        */
    });
}

pub fn spawn_kernel_thread(entry: fn() -> !, name: String, data: Option<ProcessData>) -> ProcessId {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let entry = VirtAddr::new(entry as usize as u64);
        get_process_manager().spawn_kernel_thread(entry, name, data)
    })
}

pub fn print_process_list() {
    x86_64::instructions::interrupts::without_interrupts(|| {
        get_process_manager().print_process_list();
    })
}

pub fn env(key: &str) -> Option<String> {
    x86_64::instructions::interrupts::without_interrupts(|| {
        // 获取当前进程的读锁，并查询ProcessData的env函数
        get_process_manager().current().read().env(key)
    })
}

pub fn process_exit(ret: isize) -> ! {
    x86_64::instructions::interrupts::without_interrupts(|| {
        get_process_manager().kill_current(ret);
    });

    loop {
        x86_64::instructions::hlt();
    }
}

pub fn handle_page_fault(addr: VirtAddr, err_code: PageFaultErrorCode) -> bool {
    x86_64::instructions::interrupts::without_interrupts(|| {
        get_process_manager().handle_page_fault(addr, err_code)
    })
}

pub fn get_exit_code(pid: ProcessId) -> Option<isize> {
    x86_64::instructions::interrupts::without_interrupts(|| {
        get_process_manager().get_exit_code(pid)
    })
}
