#![no_std]
#![no_main]

use lib::*;

extern crate lib;

// 消息队列容量，可以设置为1, 4, 8, 16进行测试
const QUEUE_CAPACITY: usize = 16;
// 进程总数
const PROCESS_COUNT: usize = 16;
// 每个进程处理的消息数量
const MESSAGE_COUNT: usize = 10;
// 生产者数量
const PRODUCER_COUNT: usize = PROCESS_COUNT / 2;
// 消费者数量
const CONSUMER_COUNT: usize = PROCESS_COUNT / 2;

// 信号量键值
const MUTEX_SEM_KEY: u32 = 0x1000;       // 互斥锁信号量
const EMPTY_SEM_KEY: u32 = 0x1001;       // 空槽位信号量
const FILLED_SEM_KEY: u32 = 0x1002;      // 满槽位信号量
const PRINT_MUTEX_KEY: u32 = 0x1003;     // 打印互斥锁

// 信号量
static MUTEX_SEM: Semaphore = Semaphore::new(MUTEX_SEM_KEY);
static EMPTY_SEM: Semaphore = Semaphore::new(EMPTY_SEM_KEY);
static FILLED_SEM: Semaphore = Semaphore::new(FILLED_SEM_KEY);
static PRINT_MUTEX: Semaphore = Semaphore::new(PRINT_MUTEX_KEY);

// 消息队列
static mut QUEUE: [u32; QUEUE_CAPACITY] = [0; QUEUE_CAPACITY];
static mut QUEUE_HEAD: usize = 0;
static mut QUEUE_TAIL: usize = 0;
static mut QUEUE_SIZE: usize = 0;

fn main() -> isize {
    println!("消息队列测试开始，队列容量: {}", QUEUE_CAPACITY);
    
    // 初始化信号量
    MUTEX_SEM.init(1);                  // 互斥锁初始值为1
    EMPTY_SEM.init(QUEUE_CAPACITY);     // 空槽位初始值为队列容量
    FILLED_SEM.init(0);                 // 满槽位初始值为0
    PRINT_MUTEX.init(1);                // 打印互斥锁初始值为1
    
    println!("信号量初始化完成");
    
    let mut pids = [0u16; PROCESS_COUNT];
    
    // 创建生产者进程
    for i in 0..PRODUCER_COUNT {
        let pid = sys_fork();
        if pid == 0 {
            // 子进程
            producer(i);
            sys_exit(0);
        } else {
            // 父进程
            pids[i] = pid;
        }
    }
    
    // 创建消费者进程
    for i in 0..CONSUMER_COUNT {
        let pid = sys_fork();
        if pid == 0 {
            // 子进程
            consumer(i);
            sys_exit(0);
        } else {
            // 父进程
            pids[PRODUCER_COUNT + i] = pid;
        }
    }
    
    // 输出所有进程信息
    println!("所有进程创建完成，进程ID: {:?}", pids);
    sys_stat();
    
    // 等待所有子进程退出
    for pid in pids.iter() {
        if *pid != 0 {
            sys_wait_pid(*pid);
        }
    }
    
    // 输出最终消息队列的消息数量
    unsafe {
        let queue_size = QUEUE_SIZE;
        println!("所有进程已退出，最终消息队列的消息数量: {}", queue_size);
        if queue_size == 0 {
            println!("测试通过！队列为空，所有消息都被消费了。");
        } else {
            println!("测试失败！队列不为空，还有{}条消息未被消费。", queue_size);
        }
    }
    
    // 清理信号量
    MUTEX_SEM.remove();
    EMPTY_SEM.remove();
    FILLED_SEM.remove();
    PRINT_MUTEX.remove();
    
    0
}

// 生产者函数
fn producer(id: usize) {
    let pid = sys_get_pid();
    
    safe_print(&format!("生产者 #{} (PID: {}) 开始生产消息", id, pid));
    
    for i in 0..MESSAGE_COUNT {
        // 等待空槽位
        EMPTY_SEM.wait();
        
        // 获取互斥锁
        MUTEX_SEM.wait();
        
        // 生产消息
        let message = (id as u32) * 100 + i as u32;
        unsafe {
            QUEUE[QUEUE_TAIL] = message;
            QUEUE_TAIL = (QUEUE_TAIL + 1) % QUEUE_CAPACITY;
            QUEUE_SIZE += 1;
            
            let queue_size = QUEUE_SIZE;
            safe_print(&format!(
                "生产者 #{} (PID: {}) 生产消息: {}，队列大小: {}/{}，队列是否满: {}",
                id, pid, message, queue_size, QUEUE_CAPACITY, queue_size == QUEUE_CAPACITY
            ));
        }
        
        // 释放互斥锁
        MUTEX_SEM.signal();
        
        // 通知有新消息
        FILLED_SEM.signal();
        
        // 短暂延时，模拟生产过程
        delay();
    }
    
    safe_print(&format!("生产者 #{} (PID: {}) 完成生产，退出", id, pid));
}

// 消费者函数
fn consumer(id: usize) {
    let pid = sys_get_pid();
    
    safe_print(&format!("消费者 #{} (PID: {}) 开始消费消息", id, pid));
    
    for _ in 0..MESSAGE_COUNT {
        // 等待有消息
        FILLED_SEM.wait();
        
        // 获取互斥锁
        MUTEX_SEM.wait();
        
        // 消费消息
        let message;
        unsafe {
            message = QUEUE[QUEUE_HEAD];
            QUEUE_HEAD = (QUEUE_HEAD + 1) % QUEUE_CAPACITY;
            QUEUE_SIZE -= 1;
            
            let queue_size = QUEUE_SIZE;
            safe_print(&format!(
                "消费者 #{} (PID: {}) 消费消息: {}，队列大小: {}/{}，队列是否空: {}",
                id, pid, message, queue_size, QUEUE_CAPACITY, queue_size == 0
            ));
        }
        
        // 释放互斥锁
        MUTEX_SEM.signal();
        
        // 通知有空槽位
        EMPTY_SEM.signal();
        
        // 短暂延时，模拟消费过程
        delay();
    }
    
    safe_print(&format!("消费者 #{} (PID: {}) 完成消费，退出", id, pid));
}

// 安全打印函数，使用互斥锁保证打印不会被打断
fn safe_print(s: &str) {
    PRINT_MUTEX.wait();
    println!("{}", s);
    PRINT_MUTEX.signal();
}

// 延时函数
#[inline(never)]
fn delay() {
    for _ in 0..0x10000 {
        core::hint::spin_loop();
    }
}

entry!(main);