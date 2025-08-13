use super::*;
use crate::proc::vm::ProcessVm;
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use spin::*;
use x86_64::structures::paging::mapper::MapToError;
use x86_64::structures::paging::*;
use xmas_elf::ElfFile;

#[derive(Clone)]
pub struct Process {
    pid: ProcessId,
    inner: Arc<RwLock<ProcessInner>>,
}

pub struct ProcessInner {
    name: String,
    parent: Option<Weak<Process>>,
    children: Vec<Arc<Process>>,
    ticks_passed: usize,
    pub status: ProgramStatus,
    pub context: ProcessContext,
    exit_code: Option<isize>,
    proc_data: Option<ProcessData>,
    proc_vm: Option<ProcessVm>,
}

impl Process {
    #[inline]
    pub fn pid(&self) -> ProcessId {
        self.pid
    }

    #[inline]
    pub fn write(&self) -> RwLockWriteGuard<ProcessInner> {
        self.inner.write()
    }

    #[inline]
    pub fn read(&self) -> RwLockReadGuard<ProcessInner> {
        self.inner.read()
    }

    pub fn new(
        name: String,
        parent: Option<Weak<Process>>,
        proc_vm: Option<ProcessVm>,
        proc_data: Option<ProcessData>,
    ) -> Arc<Self> {
        let name = name.to_ascii_lowercase();

        // create context
        let pid = ProcessId::new();
        let proc_vm = proc_vm.unwrap_or_else(|| ProcessVm::new(PageTableContext::new()));

        let inner = ProcessInner {
            name,
            parent,
            status: ProgramStatus::Ready,
            context: ProcessContext::default(),
            ticks_passed: 0,
            exit_code: None,
            children: Vec::new(),
            proc_vm: Some(proc_vm),
            proc_data: Some(proc_data.unwrap_or_default()),
        };

        trace!("New process {}#{} created.", &inner.name, pid);

        // create process struct
        Arc::new(Self {
            pid,
            inner: Arc::new(RwLock::new(inner)),
        })
    }

    // 新增一个可以指定PID的创建方法
    pub fn new_with_pid(
        pid: ProcessId,
        name: String,
        parent: Option<Weak<Process>>,
        proc_vm: Option<ProcessVm>,
        proc_data: Option<ProcessData>,
    ) -> Arc<Self> {
        let name = name.to_ascii_lowercase();
        let proc_vm = proc_vm.unwrap_or_else(|| ProcessVm::new(PageTableContext::new()));

        let inner = ProcessInner {
            name,
            parent,
            status: ProgramStatus::Ready,
            context: ProcessContext::default(),
            ticks_passed: 0,
            exit_code: None,
            children: Vec::new(),
            proc_vm: Some(proc_vm),
            proc_data: Some(proc_data.unwrap_or_default()),
        };

        trace!("New process {}#{} created with specific PID.", &inner.name, pid);

        // 创建进程结构体，使用指定的PID
        Arc::new(Self {
            pid,
            inner: Arc::new(RwLock::new(inner)),
        })
    }

    pub fn kill(&self, ret: isize) {
        let mut inner = self.inner.write();

        debug!(
            "Killing process {}#{} with ret code: {}",
            inner.name(),
            self.pid,
            ret
        );

        inner.kill(ret);
    }

    pub fn alloc_init_stack(&self) -> VirtAddr {
        self.write().vm_mut().init_proc_stack(self.pid)
    }

    pub fn fork(self: &Arc<Self>) -> Arc<Self> {
        // FIXME: lock inner as write
        let mut parent_inner = self.write();

        // FIXME: inner fork with parent weak ref
        // The number of existing children is used as the stack_offset_count
        let child_inner_template = parent_inner.fork(Arc::downgrade(self), self.pid);

        // FOR DBG: maybe print the child process info
        //          e.g. parent, name, pid, etc.
        let child_pid = ProcessId::new(); // Assign a new PID for the child
        trace!(
            "Parent {}#{} forking. Child will be {}#{}",
            parent_inner.name(),
            self.pid(),
            child_inner_template.name, // Name is cloned in ProcessInner::fork
            child_pid
        );

        // FIXME: make the arc of child
        // Construct the child Process struct with a new PID
        let child_proc = Arc::new(Process {
            pid: child_pid,
            inner: Arc::new(RwLock::new(child_inner_template)),
        });

        // FIXME: add child to current process's children list
        parent_inner.children.push(child_proc.clone());

        // FIXME: set fork ret value for parent with `context.set_rax`
        // Parent's fork returns the child's PID
        parent_inner.context.set_rax(child_pid.0 as usize);

        // FIXME: mark the child as ready & return it
        // Child is already marked as Ready in ProcessInner::fork
        // The child_proc Arc is returned
        child_proc
    }
}

impl ProcessInner {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn tick(&mut self) {
        self.ticks_passed += 1;
    }

    pub fn status(&self) -> ProgramStatus {
        self.status
    }

    pub fn pause(&mut self) {
        self.status = ProgramStatus::Ready;
    }

    pub fn resume(&mut self) {
        self.status = ProgramStatus::Running;
    }

    pub fn block(&mut self) {
        self.status = ProgramStatus::Blocked;
    }

    pub fn exit_code(&self) -> Option<isize> {
        self.exit_code
    }

    pub fn clone_page_table(&self) -> PageTableContext {
        self.proc_vm.as_ref().unwrap().page_table.clone()
    }

    pub fn is_ready(&self) -> bool {
        self.status == ProgramStatus::Ready
    }

    pub fn vm(&self) -> &ProcessVm {
        self.proc_vm.as_ref().unwrap()
    }

    pub fn vm_mut(&mut self) -> &mut ProcessVm {
        self.proc_vm.as_mut().unwrap()
    }

    pub fn handle_page_fault(&mut self, addr: VirtAddr) -> bool {
        self.vm_mut().handle_page_fault(addr)
    }
    
    pub fn load_elf(&mut self, elf: &ElfFile) -> Result<(), MapToError<Size4KiB>> {
        // 确保进程虚拟内存和进程数据存在
        if self.proc_vm.is_none() || self.proc_data.is_none() {
            // Consider a more specific error type if possible
            return Err(MapToError::ParentEntryHugePage);
        }

        // 调用 ProcessVm 的 load_elf 并获取内存使用信息
        let (code_bytes, _code_pages) = self.vm_mut().load_elf(elf)?; // We only need code_bytes for now

        // 获取栈使用的页面数
        let stack_pages = self.vm().stack_usage_pages();

        // 更新 ProcessData 中的内存使用统计
        self.proc_data
            .as_mut()
            .unwrap() // Already checked is_some above
            .update_memory_usage(code_bytes, stack_pages);

        Ok(()) // Return Ok if everything succeeded
    }

    /// Save the process's context
    /// 只保存上下文，不改变进程状态
    pub(super) fn save(&mut self, context: &ProcessContext) {
        // 只保存进程上下文
        self.context = context.clone();
        
        // 注意：不再在这里修改状态和增加调度计数
        // 这些操作已移至switch函数中处理
    }

    /// Restore the process's context
    /// mark the process as running
    pub(super) fn restore(&mut self, context: &mut ProcessContext) {
        // 恢复进程上下文
        *context = self.context.clone();
        
        // 恢复进程的页表
        self.vm().page_table.load();
        
        // 将进程状态设置为Running
        self.resume();
    }

    pub fn parent(&self) -> Option<Arc<Process>> {
        self.parent.as_ref().and_then(|p| p.upgrade())
    }

    pub fn proc_data(&self) -> Option<&ProcessData> {
        self.proc_data.as_ref()
    }

    pub fn kill(&mut self, ret: isize) {
        // 设置退出码
        self.exit_code = Some(ret);
        
        // 设置状态为死亡
        self.status = ProgramStatus::Dead;
        
        // 释放不再需要的资源（进程数据和虚拟内存）
        self.proc_data = None;
        self.proc_vm = None;
    }

    pub fn fork(&mut self, parent_weak_ref: Weak<Process>, parent_pid: ProcessId) -> ProcessInner {
        // FIXME: fork the process virtual memory struct
        // Use parent's current children count to determine stack offset for the new child.
        // This assumes children are added to parent's list *after* this call.
        let stack_offset_count = self.children.len() as u64;
        let child_vm = self
            .proc_vm
            .as_ref()
            .expect("Parent ProcessVm is None during fork")
            .fork(stack_offset_count);

        // Clone the parent's context for the child.
        let mut child_context = self.context.clone();

        // Update the child's stack pointer (rsp) to point to its new stack.
        // The content of the stack is copied, so the offset of rsp from the
        // start of the stack allocation should remain the same.
        let parent_rsp_virtaddr = self.context.stack_frame.stack_pointer;
        let parent_stack_start_virtaddr = self
            .proc_vm
            .as_ref()
            .unwrap()
            .stack_start_address();
        
        let rsp_offset_from_stack_start = parent_rsp_virtaddr - parent_stack_start_virtaddr;
        
        let child_stack_start_virtaddr = child_vm.stack_start_address();
        let new_rsp_val = child_stack_start_virtaddr + rsp_offset_from_stack_start;
        child_context.set_rsp(new_rsp_val);

        // Set the return value 0 for child.
        child_context.set_rax(0); // Child's fork returns 0

        // FIXME: clone the process data struct
        // ProcessData already derives Clone. If it contains Arcs, they will be shared.
        let child_proc_data = self
            .proc_data
            .as_ref()
            .expect("Parent ProcessData is None during fork")
            .clone();
        
        // FIXME: construct the child process inner
        // Child gets a new name (e.g., "parent_name-child") or inherits parent's name.
        // For simplicity, let's append "-child" to parent's name.
        // A more robust solution might involve a counter or a different naming scheme.
        let child_name = alloc::format!("{}-child-{}", self.name, parent_pid.0); // Use parent_pid to make it somewhat unique initially

        debug!(
            "ProcessInner::fork: Parent name: {}, Parent PID: {}, Child name: {}",
            self.name, parent_pid, child_name
        );
        
        ProcessInner {
            name: child_name,
            parent: Some(parent_weak_ref),
            children: Vec::new(), // Child starts with no children
            ticks_passed: 0,      // Child starts with 0 ticks
            status: ProgramStatus::Ready, // Child is ready to run
            context: child_context,
            exit_code: None,
            proc_data: Some(child_proc_data),
            proc_vm: Some(child_vm),
        }
        // NOTE: return inner because there's no pid record in inner
    }

    // Semaphore related methods
    // These methods assume that `self.proc_data` is Some and contains the `semaphores` field.
    // The `semaphores` field itself is an Arc<RwLock<SemaphoreSet>>.
    // Access to `self.semaphores` is via Deref/DerefMut for ProcessInner -> ProcessData.

    pub fn sem_new(&mut self, key: u32, value: usize) -> bool {
        // self.proc_data must be Some. DerefMut gives &mut ProcessData.
        // Then .semaphores gives Arc<RwLock<SemaphoreSet>>.
        // .write() on Arc<RwLock<T>> gets a write lock on T.
        if let Some(pd) = self.proc_data.as_mut() {
             pd.semaphores.write().insert(key, value)
        } else {
            // This case should ideally not happen if proc_data is always present for live processes
            // or if sem_new is only called on live processes.
            // Consider logging an error or panicking if proc_data is None.
            error!("sem_new called on ProcessInner with no proc_data");
            false
        }
    }

    pub fn sem_remove(&mut self, key: u32) -> bool {
        if let Some(pd) = self.proc_data.as_mut() {
            pd.semaphores.write().remove(key)
        } else {
            error!("sem_remove called on ProcessInner with no proc_data");
            false
        }
    }

    pub fn sem_signal(&self, key: u32) -> sync::SemaphoreResult {
        // Deref gives &ProcessData.
        if let Some(pd) = self.proc_data.as_ref() {
            pd.semaphores.read().signal(key)
        } else {
            error!("sem_signal called on ProcessInner with no proc_data");
            sync::SemaphoreResult::NotExist // Or a more generic error
        }
    }

    pub fn sem_wait(&self, key: u32, pid_to_block: ProcessId) -> sync::SemaphoreResult {
        if let Some(pd) = self.proc_data.as_ref() {
            pd.semaphores.read().wait(key, pid_to_block)
        } else {
            error!("sem_wait called on ProcessInner with no proc_data");
            sync::SemaphoreResult::NotExist // Or a more generic error
        }
    }
}

impl core::ops::Deref for Process {
    type Target = Arc<RwLock<ProcessInner>>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl core::ops::Deref for ProcessInner {
    type Target = ProcessData;

    fn deref(&self) -> &Self::Target {
        self.proc_data
            .as_ref()
            .expect("Process data empty. The process may be killed.")
    }
}

impl core::ops::DerefMut for ProcessInner {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.proc_data
            .as_mut()
            .expect("Process data empty. The process may be killed.")
    }
}


impl core::fmt::Debug for Process {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        let inner = self.inner.read();
        f.debug_struct("Process")
            .field("pid", &self.pid)
            .field("name", &inner.name)
            .field("parent", &inner.parent().map(|p| p.pid))
            .field("status", &inner.status)
            .field("ticks_passed", &inner.ticks_passed)
            .field("children", &inner.children.iter().map(|c| c.pid.0))
            .field("status", &inner.status)
            .field("context", &inner.context)
            .field("vm", &inner.proc_vm)
            .finish()
    }
}

impl core::fmt::Display for Process {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        let inner = self.inner.read();
        // 获取内存占用页数，如果proc_data不存在（例如进程已结束），则显示 0
        let mem_pages = inner.proc_data.as_ref().map_or(0, |data| data.memory_usage_pages());
        // 格式化内存大小
        let (mem_size, mem_unit) = crate::humanized_size(mem_pages * crate::memory::PAGE_SIZE);

        write!(
            f,
            // PID | PPID | Name        | Ticks   | Mem Pages | Mem Size | Status
            " #{:-3} | #{:-3} | {:<12} | {:<7} | {:<9} | {:>6} {} | {:?}",
            self.pid.0,                                         // PID
            inner.parent().map(|p| p.pid.0).unwrap_or(0),       // Parent PID
            inner.name,                                         // Process Name
            inner.ticks_passed,                                 // Ticks Passed
            mem_pages,                                          // Memory Pages
            mem_size, mem_unit,                                 // Humanized Memory Size
            inner.status                                        // Status
        )?;
        Ok(())
    }
}
