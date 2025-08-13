use super::*;
use crate::memory::{
    self,
    get_frame_alloc_for_sure, PAGE_SIZE,
};
use alloc::{collections::*, format, sync::Arc};
use alloc::sync::Weak;
use spin::{Mutex, RwLock};
use xmas_elf::ElfFile;

pub static PROCESS_MANAGER: spin::Once<ProcessManager> = spin::Once::new();

pub fn init(init: Arc<Process>, app_list: Option<&'static boot::AppList>) {
    // 设置初始进程为运行状态
    init.write().status = ProgramStatus::Running;
    
    // 设置处理器的当前pid为初始进程的pid
    processor::set_pid(init.pid());
    
    PROCESS_MANAGER.call_once(|| {
        let mut manager = ProcessManager::new(init);
        manager.app_list = app_list;
        manager
    });
}

pub fn get_process_manager() -> &'static ProcessManager {
    PROCESS_MANAGER
        .get()
        .expect("Process Manager has not been initialized")
}

pub struct ProcessManager {
    processes: RwLock<BTreeMap<ProcessId, Arc<Process>>>,
    ready_queue: Mutex<VecDeque<ProcessId>>,
    app_list: Option<&'static boot::AppList>,
    wait_queue: Mutex<BTreeMap<ProcessId, BTreeSet<ProcessId>>>,
}

impl ProcessManager {
    pub fn new(init: Arc<Process>) -> Self {
        let mut processes = BTreeMap::new();
        let ready_queue = VecDeque::new();
        let pid = init.pid();

        trace!("Init {:#?}", init);

        processes.insert(pid, init);
        Self {
            processes: RwLock::new(processes),
            ready_queue: Mutex::new(ready_queue),
            app_list: None, // 默认为None
            wait_queue: Mutex::new(BTreeMap::new()),
        }
    }

    #[inline]
    pub fn app_list(&self) -> Option<&'static boot::AppList> {
        self.app_list
    }

    #[inline]
    pub fn push_ready(&self, pid: ProcessId) {
        self.ready_queue.lock().push_back(pid);
    }

    #[inline]
    fn add_proc(&self, pid: ProcessId, proc: Arc<Process>) {
        self.processes.write().insert(pid, proc);
    }

    #[inline]
    pub fn get_proc(&self, pid: &ProcessId) -> Option<Arc<Process>> {
        self.processes.read().get(pid).cloned()
    }

    pub fn current(&self) -> Arc<Process> {
        self.get_proc(&processor::get_pid())
            .expect("No current process")
    }

    pub fn save_current(&self, context: &ProcessContext) {
        // 获取当前进程
        let current_pid = processor::get_pid();
        let current = self.get_proc(&current_pid).expect("No current process");
        
        // 保存当前进程上下文
        let mut proc_inner = current.write();
        proc_inner.save(context);
        
        // 如果进程状态不是Dead，将其加入就绪队列
        if proc_inner.status() == ProgramStatus::Ready {
            drop(proc_inner); // 提前释放锁，避免死锁
            self.push_ready(current_pid);
        }
    }

    pub fn switch_next(&self, context: &mut ProcessContext) -> ProcessId {
        // 获取就绪队列的互斥锁
        let mut ready_queue = self.ready_queue.lock();
        
        // 从就绪队列中取出下一个进程
        while let Some(next_pid) = ready_queue.pop_front() {
            // 释放就绪队列的锁，以避免死锁
            drop(ready_queue);
            
            // 获取下一个进程
            if let Some(next_proc) = self.get_proc(&next_pid) {
                // 检查进程状态
                let mut next_inner = next_proc.write();
                
                // 如果进程已经就绪，则恢复其上下文
                if next_inner.status() == ProgramStatus::Ready {
                    // 恢复进程上下文和页表
                    next_inner.restore(context);
                    
                    // 更新当前处理器的PID
                    processor::set_pid(next_pid);
                    
                    // 释放锁并返回下一个进程的PID
                    drop(next_inner);
                    return next_pid;
                }
                
                // 如果进程不是就绪状态（可能是死亡或阻塞），则继续寻找下一个进程
                drop(next_inner);
            }
            
            // 重新获取就绪队列的锁
            ready_queue = self.ready_queue.lock();
        }
        
        // 如果就绪队列为空，获取当前PID
        let current_pid = processor::get_pid();
        
        // 检查当前是否已经是内核进程
        if current_pid == KERNEL_PID {
            // 如果当前已经是内核进程，使用hlt指令让CPU空闲一会儿
            // 这样可以减少内核进程的执行频率，让其他进程有更多机会被调度
            x86_64::instructions::hlt();
        }
        
        // 获取内核进程
        let kernel = self.get_proc(&KERNEL_PID).expect("Kernel process not found");
        let mut kernel_inner = kernel.write();
        
        // 恢复内核进程上下文
        kernel_inner.restore(context);
        
        // 更新当前处理器的PID
        processor::set_pid(KERNEL_PID);
        
        // 返回内核进程PID
        KERNEL_PID
    }

    pub fn spawn_kernel_thread(
        &self,
        entry: VirtAddr,
        name: String,
        proc_data: Option<ProcessData>,
    ) -> ProcessId {
        // 获取内核进程，如果找不到就创建一个新的内核进程
        let kproc = match self.get_proc(&KERNEL_PID) {
            Some(proc) => proc,
            None => {
                warn!("Kernel process not found, creating a new one");
                let vm = ProcessVm::new(PageTableContext::new()).init_kernel_vm();
                let kernel_proc = Process::new_with_pid(
                    KERNEL_PID,
                    String::from("kernel"),
                    None,
                    Some(vm),
                    Some(ProcessData::new()),
                );
                self.add_proc(KERNEL_PID, kernel_proc.clone());
                kernel_proc
            }
        };
        
        let page_table = kproc.read().clone_page_table();
        let proc_vm = Some(ProcessVm::new(page_table));
        let proc = Process::new(name, Some(Arc::downgrade(&kproc)), proc_vm, proc_data);

        // 获取进程ID
        let pid = proc.pid();
        
        // 分配栈空间
        let stack_top = proc.alloc_init_stack();
        
        // 设置栈帧
        let mut inner = proc.write();
        inner.context.init_stack_frame(entry, stack_top);
        drop(inner); // 释放锁
        
        // 将进程添加到进程映射中
        self.add_proc(pid, proc.clone());
        
        // 将进程加入就绪队列
        self.push_ready(pid);
        
        // 返回新进程的PID
        pid
    }

    pub fn spawn(
        &self,
        elf: &ElfFile,
        name: String,
        parent: Option<Weak<Process>>,
        proc_data: Option<ProcessData>,
    ) -> ProcessId {
        let kproc = self.get_proc(&KERNEL_PID).unwrap();
        let page_table = kproc.read().clone_page_table();
        let proc_vm = Some(ProcessVm::new(page_table));
        let proc = Process::new(name, parent, proc_vm, proc_data);

        // 获取进程ID
        let pid = proc.pid();

        let mut inner = proc.write();
        // 加载ELF文件到进程页表，设置用户访问权限标志
        if let Err(err) = inner.load_elf(elf) {
            error!("Failed to load ELF: {:?}", err);
            return pid;
        }

        // 为进程分配新栈
        drop(inner);
        let stack_top = proc.alloc_init_stack();
        
        // 设置栈帧
        let mut inner = proc.write();
        inner.context.init_stack_frame(VirtAddr::new(elf.header.pt2.entry_point()), stack_top);
        
        // 标记进程为就绪状态
        inner.status = ProgramStatus::Ready;
        drop(inner);

        trace!("New {:#?}", &proc);

        // 将进程添加到进程映射中
        self.add_proc(pid, proc.clone());
        
        // 将进程加入就绪队列
        self.push_ready(pid);

        pid
    }

    pub fn kill_current(&self, ret: isize) {
        self.kill(processor::get_pid(), ret);
    }

    pub fn handle_page_fault(&self, addr: VirtAddr, err_code: PageFaultErrorCode) -> bool {
        // 获取当前进程
        let current = self.current();
        
        // 检查是否是越权访问（保护违规）
        if err_code.contains(PageFaultErrorCode::PROTECTION_VIOLATION) {
            warn!(
                "Protection violation page fault at {:#x}, error code: {:?}",
                addr,
                err_code
            );
            return false;
        }
        
        // 获取进程的写锁
        let mut proc_inner = current.write();
        
        // 尝试处理页面错误（委托给进程的虚拟内存管理器）
        proc_inner.handle_page_fault(addr)
    }

    pub fn kill(&self, pid: ProcessId, ret: isize) {
        let proc = self.get_proc(&pid);

        if proc.is_none() {
            warn!("Process #{} not found.", pid);
            return;
        }

        let proc = proc.unwrap();

        if proc.read().status() == ProgramStatus::Dead {
            warn!("Process #{} is already dead.", pid);
            return;
        }

        trace!("Kill {:#?}", &proc);

        proc.kill(ret);

        // Wake up processes waiting for this one, using variable names from the document
        if let Some(pids) = self.wait_queue.lock().remove(&pid) { // 'pid' is the one being killed
            for pid_to_wake_up in pids { // 'pid_to_wake_up' (from the set) is called 'pid' in the document snippet
                self.wake_up(pid_to_wake_up, Some(ret)); // 'ret' is the exit code of the killed process
            }
        }
    }

    pub fn get_exit_code(&self, pid: ProcessId) -> Option<isize> {
        // 获取进程对象
        if let Some(proc) = self.get_proc(&pid) {
            // 读取进程内部数据，检查其exit_code
            proc.read().exit_code()
        } else {
            // 进程不存在
            None
        }
    }

    /// Block the process with the given pid
    pub fn block(&self, pid: ProcessId) {
        if let Some(proc) = self.get_proc(&pid) {
            proc.write().block();
        }
    }

    pub fn wait_pid(&self, pid: ProcessId) {
        let mut wait_queue = self.wait_queue.lock();
        let current_pid = processor::get_pid();
        wait_queue.entry(pid).or_default().insert(current_pid);
    }

    /// Wake up the process with the given pid
    ///
    /// If `ret` is `Some`, set the return value of the process
    pub fn wake_up(&self, pid: ProcessId, ret: Option<isize>) {
        if let Some(proc) = self.get_proc(&pid) {
            let mut inner = proc.write();
            // Only attempt to wake up if the process is actually Blocked.
            if inner.status() == ProgramStatus::Blocked {
                if let Some(ret_val) = ret {
                    // Set the return value (e.g., for wait_pid)
                    inner.context.set_rax(ret_val as usize);
                }
                // Set the process status to Ready
                inner.pause(); // .pause() sets status to ProgramStatus::Ready
                
                // Drop the lock before pushing to ready_queue
                drop(inner);
                
                // Push the process to the ready queue
                self.push_ready(pid);
            }
        }
    }

    pub fn fork(&self) -> ProcessId {
        // FIXME: get current process
        let parent_process = self.current();
        trace!(
            "ProcessManager: Forking current process {}#{}",
            parent_process.read().name(),
            parent_process.pid()
        );

        // FIXME: fork to get child
        // This will internally set the parent's rax to child_pid
        // and child's rax to 0.
        let child_process = parent_process.fork(); // Process::fork()
        trace!(
            "ProcessManager: Forked. Child PID: {}, Parent PID: {}",
            child_process.pid(),
            parent_process.pid()
        );
        
        // FIXME: add child to process list
        self.add_proc(child_process.pid(), child_process.clone());
        
        // Child process is already marked as Ready by Process::fork -> ProcessInner::fork.
        // It will be added to the ready queue later by the top-level syscall handler.

        // FOR DBG: maybe print the process ready queue?
        debug!("ProcessManager: Fork complete. Current ready queue: {:?}", self.ready_queue.lock());
        
        child_process.pid()
    }

    pub fn print_process_list(&self) {
        let mut output = String::from("  PID | PPID | Process Name |  Ticks  | Status\n");

        self.processes
            .read()
            .values()
            .filter(|p| p.read().status() != ProgramStatus::Dead)
            .for_each(|p| output += format!("{}\n", p).as_str());

        // TODO: print memory usage of kernel heap

        output += format!("Queue  : {:?}\n", self.ready_queue.lock()).as_str();

        output += &processor::print_processors();

        print!("{}", output);
    }
}
