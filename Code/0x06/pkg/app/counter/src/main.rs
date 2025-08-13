#![no_std]
#![no_main]

use lib::*;

extern crate lib;

const THREAD_COUNT: usize = 8;
static SPIN_LOCK: SpinLock = SpinLock::new();
const SEM_KEY: u32 = 0x1234; // 信号量键值
static SEMAPHORE: Semaphore = Semaphore::new(SEM_KEY); // 静态信号量实例
static mut COUNTER: isize = 0;

fn test_spin() {
    println!("--- Testing SpinLock ---");
    unsafe { COUNTER = 0; } // 重置计数器
    let mut pids = [0u16; THREAD_COUNT];

    for i in 0..THREAD_COUNT {
        let pid = sys_fork();
        if pid == 0 {
            do_counter_inc_spin();
            sys_exit(0);
        } else {
            pids[i] = pid;
        }
    }

    let cpid = sys_get_pid();
    println!("SpinLock test process #{} holds threads: {:?}", cpid, &pids);
    sys_stat(); // 暂时注释掉，以便观察纯粹的计数器行为

    for i in 0..THREAD_COUNT {
        println!("#{} waiting for #{}...", cpid, pids[i]); // 可以暂时注释掉，减少输出
        sys_wait_pid(pids[i]);
    }

    println!("SpinLock COUNTER result: {}", unsafe { COUNTER });
    if unsafe { COUNTER } == (THREAD_COUNT * 100) as isize {
        println!("SpinLock test PASSED! 🎉");
    } else {
        println!("SpinLock test FAILED! 😢 Expected {}, got {}", THREAD_COUNT * 100, unsafe { COUNTER });
    }
}
fn main() -> isize {
    test_spin(); // 先注释掉 SpinLock 测试
    // test_semaphore();
    0
}

fn do_counter_inc_sem() {
    for _ in 0..100 {
        SEMAPHORE.wait(); // P 操作
        inc_counter();    // 临界区
        SEMAPHORE.signal(); // V 操作
    }
}

fn test_semaphore() {
    println!("--- Testing Semaphore ---");
    unsafe { COUNTER = 0; } // 重置计数器

    // 初始化信号量，初始值为1，用作互斥锁
    if !SEMAPHORE.init(1) {
        println!("Failed to initialize semaphore! Skipping Semaphore test. 😥");
        return;
    }
    println!("Semaphore initialized with key {:#x} and value 1.", SEM_KEY);


    let mut pids = [0u16; THREAD_COUNT];
    for i in 0..THREAD_COUNT {
        let pid = sys_fork();
        if pid == 0 {
            do_counter_inc_sem();
            sys_exit(0);
        } else {
            pids[i] = pid;
        }
    }

    let cpid = sys_get_pid();
    println!("Semaphore test process #{} holds threads: {:?}", cpid, &pids);

    for i in 0..THREAD_COUNT {
        sys_wait_pid(pids[i]);
    }

    println!("Semaphore COUNTER result: {}", unsafe { COUNTER });
    if unsafe { COUNTER } == (THREAD_COUNT * 100) as isize {
        println!("Semaphore test PASSED! 🎉");
    } else {
        println!("Semaphore test FAILED! 😢 Expected {}, got {}", THREAD_COUNT * 100, unsafe { COUNTER });
    }

    // 移除信号量
    if !SEMAPHORE.remove() {
        println!("Failed to remove semaphore with key {:#x}. This might cause issues in subsequent runs.", SEM_KEY);
    } else {
        println!("Semaphore with key {:#x} removed successfully.", SEM_KEY);
    }
}

fn do_counter_inc_spin() {
    for _ in 0..100 {
        SPIN_LOCK.acquire();
        inc_counter(); // 这是临界区
        SPIN_LOCK.release();
    }
}
fn do_counter_inc() {
    for _ in 0..100 {
        // FIXME: protect the critical section
        inc_counter();
    }
}

/// Increment the counter
///
/// this function simulate a critical section by delay
/// DO NOT MODIFY THIS FUNCTION
fn inc_counter() {
    unsafe {
        delay();
        let mut val = COUNTER;
        delay();
        val += 1;
        delay();
        COUNTER = val;
    }
}

#[inline(never)]
#[unsafe(no_mangle)]
fn delay() {
    for _ in 0..0x100 {
        core::hint::spin_loop();
    }
}

entry!(main);
