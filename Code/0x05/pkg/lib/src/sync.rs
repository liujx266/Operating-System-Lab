use core::{
    hint::spin_loop,
    sync::atomic::{AtomicBool, Ordering},
};

use crate::*;

pub struct SpinLock {
    bolt: AtomicBool,
}

impl SpinLock {
    pub const fn new() -> Self {
        Self {
            bolt: AtomicBool::new(false),
        }
    }

    pub fn acquire(&self) {
        while self
            .bolt
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            // 如果锁被占用，则自旋
            while self.bolt.load(Ordering::Relaxed) {
                spin_loop();
            }
        }
    }

    pub fn release(&self) {
        self.bolt.store(false, Ordering::Release);
    }
}

unsafe impl Sync for SpinLock {} // Why? Check reflection question 5

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Semaphore {
    key: u32,
}

impl Semaphore {
    pub const fn new(key: u32) -> Self {
        Self { key }
    }

    #[inline(always)]
    pub fn init(&self, value: usize) -> bool {
        sys_new_sem(self.key, value)
    }

    #[inline(always)]
    pub fn remove(&self) -> bool {
        sys_remove_sem(self.key)
    }

    #[inline(always)]
    pub fn signal(&self) -> bool {
        sys_signal_sem(self.key)
    }

    #[inline(always)]
    pub fn wait(&self) -> bool {
        sys_wait_sem(self.key)
    }
}

#[inline(always)]
fn sys_new_sem(key: u32, value: usize) -> bool {
    syscall!(Syscall::Sem, 0, key as usize, value) == 0
}

#[inline(always)]
fn sys_remove_sem(key: u32) -> bool {
    syscall!(Syscall::Sem, 1, key as usize, 0) == 0 // value is not used for remove
}

#[inline(always)]
fn sys_signal_sem(key: u32) -> bool {
    syscall!(Syscall::Sem, 2, key as usize, 0) == 0 // value is not used for signal
}

#[inline(always)]
fn sys_wait_sem(key: u32) -> bool {
    syscall!(Syscall::Sem, 3, key as usize, 0) == 0 // value is not used for wait
}

unsafe impl Sync for Semaphore {}

#[macro_export]
macro_rules! semaphore_array {
    [$($x:expr),+ $(,)?] => {
        [ $($crate::Semaphore::new($x),)* ]
    }
}
