mod context;
mod data;
mod manager;
mod paging;
mod pid;
mod process;
pub mod processor; // Make processor module public
mod vm;
pub mod sync;

pub use manager::get_process_manager; // Publicly re-export get_process_manager
use process::*;
use vm::ProcessVm;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::sync::Arc;
pub use context::ProcessContext;
pub use paging::PageTableContext;
pub use data::ProcessData;
pub use pid::ProcessId;
use xmas_elf::ElfFile;
use storage::FileSystem;

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
pub fn init(boot_info: &'static boot::BootInfo) {
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
    
    // 获取应用列表
    let app_list = boot_info.loaded_apps.as_ref();
    manager::init(kproc, app_list);

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
pub fn wait_pid(pid: ProcessId, context: &mut ProcessContext) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let manager = get_process_manager();
        if let Some(ret) = manager.get_exit_code(pid) {
            context.set_rax(ret as usize);
        } else {
            manager.wait_pid(pid);
            manager.save_current(context);
            manager.current().write().block();
            manager.switch_next(context);
        }
    })
}

pub fn list_app() {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let app_list = get_process_manager().app_list();
        if app_list.is_none() {
            println!("[!] No app found in list!");
            return;
        }

        let apps = app_list
            .unwrap()
            .iter()
            .map(|app| app.name.as_str())
            .collect::<Vec<&str>>()
            .join(", ");

        // TODO: print more information like size, entry point, etc.

        println!("[+] App list: {}", apps);
    });
}

pub fn spawn(path: &str) -> Option<ProcessId> {
    use alloc::boxed::Box;

    // 首先尝试从文件路径加载
    if let Ok(mut file_handle) = crate::drivers::filesystem::get_rootfs().open_file(path) {
        // 获取文件大小并分配缓冲区
        let file_size = file_handle.meta.len;
        let mut buffer = alloc::vec![0u8; file_size];

        // 读取整个文件
        if let Ok(bytes_read) = file_handle.read(&mut buffer) {
            if bytes_read == file_size {
                // 将缓冲区移动到Box中以确保生命周期
                let buffer = Box::leak(buffer.into_boxed_slice());

                // 解析ELF文件
                if let Ok(elf) = xmas_elf::ElfFile::new(buffer) {
                    // 从路径中提取文件名作为进程名
                    let process_name = path.split('/').last().unwrap_or(path).to_string();

                    let entry_point = elf.header.pt2.entry_point();
                    info!("Loading ELF from path '{}' as process '{}', entry point: {:#x}", path, process_name, entry_point);

                    if buffer.len() >= 16 {
                        info!("ELF header bytes: {:02x?}", &buffer[0..16]);
                    }

                    if entry_point != 0 {
                        return elf_spawn(process_name, &elf);
                    }
                }
            }
        }
    }

    // 如果文件路径加载失败，尝试从bootloader应用列表加载（向后兼容）
    let app = x86_64::instructions::interrupts::without_interrupts(|| {
        let app_list = get_process_manager().app_list()?;
        app_list.iter().find(|&app| app.name.eq(path))
    })?;

    // 添加调试信息
    let entry_point = app.elf.header.pt2.entry_point();
    info!("Loading ELF from bootloader app '{}', entry point: {:#x}", path, entry_point);

    // 打印ELF文件的前16个字节用于调试
    if app.elf.input.len() >= 16 {
        info!("ELF header bytes: {:02x?}", &app.elf.input[0..16]);
    }

    elf_spawn(path.to_string(), &app.elf)
}



pub fn elf_spawn(name: String, elf: &ElfFile) -> Option<ProcessId> {
    let pid = x86_64::instructions::interrupts::without_interrupts(|| {
        let manager = get_process_manager();
        let process_name = name.to_lowercase();
        let parent = Arc::downgrade(&manager.current());
        let pid = manager.spawn(elf, name, Some(parent), None);

        debug!("Spawned process: {}#{}", process_name, pid);
        pid
    });

    Some(pid)
}

pub fn read(fd: u8, buf: &mut [u8]) -> isize {
    x86_64::instructions::interrupts::without_interrupts(|| get_process_manager().current().read().read(fd, buf))
}

pub fn write(fd: u8, buf: &[u8]) -> isize {
    x86_64::instructions::interrupts::without_interrupts(|| get_process_manager().current().read().write(fd, buf))
}

pub fn open_file(path: &str) -> Result<u8, ()> {
    x86_64::instructions::interrupts::without_interrupts(|| {
        // 尝试打开文件
        match crate::drivers::filesystem::get_rootfs().open_file(path) {
            Ok(file_handle) => {
                // 获取当前进程并添加文件到资源集合
                let current_proc = get_process_manager().current();
                let proc_data = current_proc.read().proc_data().unwrap().clone();
                let fd = proc_data.open_resource(crate::utils::Resource::File(file_handle));
                Ok(fd)
            }
            Err(_) => Err(()),
        }
    })
}

pub fn close_file(fd: u8) -> bool {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let current_proc = get_process_manager().current();
        let proc_data = current_proc.read().proc_data().unwrap().clone();
        proc_data.close_resource(fd)
    })
}

pub fn exit(ret: isize, context: &mut ProcessContext) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let manager = get_process_manager();
        manager.kill_current(ret);
        manager.switch_next(context);
    })
}

pub fn still_alive(pid: ProcessId) -> bool {
    x86_64::instructions::interrupts::without_interrupts(|| {
        // 检查进程是否仍然存活
        match get_process_manager().get_proc(&pid) {
            Some(proc) => {
                let status = proc.read().status();
                status != ProgramStatus::Dead
            }
            None => false,
        }
    })
}