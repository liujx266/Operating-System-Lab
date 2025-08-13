use super::ProcessId;
use alloc::collections::*;
use spin::Mutex;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct SemaphoreId(u32);

impl SemaphoreId {
    pub fn new(key: u32) -> Self {
        Self(key)
    }
}

/// Mutex is required for Semaphore
#[derive(Debug, Clone)]
pub struct Semaphore {
    count: usize,
    wait_queue: VecDeque<ProcessId>,
}

/// Semaphore result
#[derive(Debug)]
pub enum SemaphoreResult {
    Ok,
    NotExist,
    Block(ProcessId),
    WakeUp(ProcessId),
}

impl Semaphore {
    /// Create a new semaphore
    pub fn new(value: usize) -> Self {
        Self {
            count: value,
            wait_queue: VecDeque::new(),
        }
    }

    /// Wait the semaphore (acquire/down/proberen)
    ///
    /// if the count is 0, then push the process into the wait queue
    /// else decrease the count and return Ok
    pub fn wait(&mut self, pid: ProcessId) -> SemaphoreResult {
        if self.count == 0 {
            self.wait_queue.push_back(pid);
            SemaphoreResult::Block(pid)
        } else {
            self.count -= 1;
            SemaphoreResult::Ok
        }
    }

    /// Signal the semaphore (release/up/verhogen)
    ///
    /// if the wait queue is not empty, then pop a process from the wait queue
    /// else increase the count
    pub fn signal(&mut self) -> SemaphoreResult {
        if let Some(pid) = self.wait_queue.pop_front() {
            SemaphoreResult::WakeUp(pid)
        } else {
            self.count += 1;
            SemaphoreResult::Ok
        }
    }
}

#[derive(Debug, Default)]
pub struct SemaphoreSet {
    sems: BTreeMap<SemaphoreId, Mutex<Semaphore>>,
}

impl SemaphoreSet {
    pub fn insert(&mut self, key: u32, value: usize) -> bool {
        trace!("Sem Insert: <{:#x}>{}", key, value);
        self.sems
            .insert(SemaphoreId::new(key), Mutex::new(Semaphore::new(value)))
            .is_none()
    }

    pub fn remove(&mut self, key: u32) -> bool {
        trace!("Sem Remove: <{:#x}>", key);
        self.sems.remove(&SemaphoreId::new(key)).is_some()
    }

    /// Wait the semaphore (acquire/down/proberen)
    pub fn wait(&self, key: u32, pid: ProcessId) -> SemaphoreResult {
        let sid = SemaphoreId::new(key);
        if let Some(sem_mutex) = self.sems.get(&sid) {
            sem_mutex.lock().wait(pid)
        } else {
            SemaphoreResult::NotExist
        }
    }

    /// Signal the semaphore (release/up/verhogen)
    pub fn signal(&self, key: u32) -> SemaphoreResult {
        let sid = SemaphoreId::new(key);
        if let Some(sem_mutex) = self.sems.get(&sid) {
            sem_mutex.lock().signal()
        } else {
            SemaphoreResult::NotExist
        }
    }
}

impl core::fmt::Display for Semaphore {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Semaphore({}) {:?}", self.count, self.wait_queue)
    }
}

use crate::proc::{get_process_manager, ProcessContext};
use crate::proc::processor;
use crate::interrupt::syscall::SyscallArgs;
use x86_64;

pub fn new_sem(key: u32, value: usize) -> usize {
    let manager = get_process_manager();
    if manager.current().write().sem_new(key, value) {
        0
    } else {
        1 // Indicates failure (e.g., semaphore already exists with this key, though current BTreeMap replaces)
    }
}

pub fn remove_sem(key: u32) -> usize {
    let manager = get_process_manager();
    if manager.current().write().sem_remove(key) {
        0
    } else {
        1 // Indicates failure (e.g., semaphore does not exist)
    }
}

pub fn sem_signal(key: u32, context: &mut ProcessContext) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let manager = get_process_manager();
        // let pid = processor::get_pid(); // Not directly used in signal logic itself, but good for consistency if needed later
        let ret = manager.current().write().sem_signal(key);
        match ret {
            SemaphoreResult::Ok => context.set_rax(0),
            SemaphoreResult::NotExist => context.set_rax(1), // Using 1 for NotExist as per convention
            SemaphoreResult::WakeUp(pid_wake) => {
                manager.wake_up(pid_wake, None);
                context.set_rax(0);
            }
            _ => unreachable!("sem_signal should not block"),
        }
    })
}

pub fn sem_wait(key: u32, context: &mut ProcessContext) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let manager = get_process_manager();
        let pid = processor::get_pid();
        let ret = manager.current().write().sem_wait(key, pid);
        match ret {
            SemaphoreResult::Ok => context.set_rax(0),
            SemaphoreResult::NotExist => context.set_rax(1), // Using 1 for NotExist
            SemaphoreResult::Block(pid_block) => {
                // Ensure pid_block is the current pid, as wait should block the caller
                assert_eq!(pid_block, pid, "SemaphoreResult::Block should carry the current PID");
                manager.save_current(context); // Save current process's context
                manager.current().write().block(); // Block the current process by calling ProcessInner's block method
                                             // The `block_current` might take an optional reason or state,
                                             // but for semaphores, just blocking is fine.
                                             // The `pid_block` from SemaphoreResult confirms which process to block,
                                             // which should be the current one.
                manager.switch_next(context); // Switch to the next available process
            }
            _ => unreachable!("sem_wait should not wake up another process directly"),
        }
    })
}

pub fn sys_sem(args: &SyscallArgs, context: &mut ProcessContext) {
    match args.arg0 {
        0 => context.set_rax(new_sem(args.arg1 as u32, args.arg2)), // op 0: new_sem
        1 => context.set_rax(remove_sem(args.arg1 as u32)),      // op 1: remove_sem
        2 => sem_signal(args.arg1 as u32, context),              // op 2: sem_signal
        3 => sem_wait(args.arg1 as u32, context),                // op 3: sem_wait
        _ => context.set_rax(usize::MAX), // Invalid operation, return a distinct error code
    }
}
