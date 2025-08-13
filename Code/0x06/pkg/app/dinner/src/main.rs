#![no_std]
#![no_main]

extern crate alloc;
extern crate lib;

use lib::*;
use rand::prelude::*;
use rand_chacha::ChaCha20Rng;

// 定义常量
const PHILOSOPHER_COUNT: usize = 5;
const LONG_MEAL_TIME: u64 = 800000;  // 长时间就餐
const SHORT_MEAL_TIME: u64 = 100000; // 短时间就餐
static CHOPSTICKS: [Semaphore; PHILOSOPHER_COUNT] = semaphore_array![0, 1, 2, 3, 4];
// 用于同步开始的信号量
static START_SEM: Semaphore = Semaphore::new(5);


fn philosopher(id: usize) -> ! {
    let mut rng = ChaCha20Rng::seed_from_u64(sys_get_pid() as u64);
    let mut meal_count = 0;  // 记录就餐次数
    let left = id;
    let right = (id + 1) % PHILOSOPHER_COUNT;
    
    // 等待所有哲学家就绪
    START_SEM.wait();
    
    loop {
        crate::println!("🔄 哲学家 {} 准备就餐 (已就餐{}次)...", id, meal_count);
        
        if id % 2 == 0 {
            // 偶数号哲学家先拿右边的筷子
            crate::println!("⌛ 哲学家 {} 尝试拿起右边的筷子 {}...", id, right);
            CHOPSTICKS[right].wait();
            crate::println!("✅ 哲学家 {} 拿到了右边的筷子 {}", id, right);
            
            // 随机短暂延迟，增加竞争
            for _ in 0..rng.gen_range(10000..50000) {
                // 空循环
            }
            
            crate::println!("⌛ 哲学家 {} 尝试拿起左边的筷子 {}...", id, left);
            CHOPSTICKS[left].wait();
            crate::println!("✅ 哲学家 {} 拿到了左边的筷子 {}", id, left);
        } else {
            // 奇数号哲学家先拿左边的筷子
            crate::println!("⌛ 哲学家 {} 尝试拿起左边的筷子 {}...", id, left);
            CHOPSTICKS[left].wait();
            crate::println!("✅ 哲学家 {} 拿到了左边的筷子 {}", id, left);
            
            // 随机短暂延迟，增加竞争
            for _ in 0..rng.gen_range(10000..50000) {
                // 空循环
            }
            
            crate::println!("⌛ 哲学家 {} 尝试拿起右边的筷子 {}...", id, right);
            CHOPSTICKS[right].wait();
            crate::println!("✅ 哲学家 {} 拿到了右边的筷子 {}", id, right);
        }
        
        // 偶数号哲学家吃得更久，导致邻座饥饿
        let meal_time = if id % 2 == 0 {
            LONG_MEAL_TIME
        } else {
            SHORT_MEAL_TIME
        };
        
        crate::println!("🍜 哲学家 {} 正在就餐{}...", id, if id % 2 == 0 { "(贪吃)" } else { "" });
        
        for _ in 0..meal_time {
            // 空循环
        }
        
        meal_count += 1;
        
        // 放下筷子
        CHOPSTICKS[left].signal();
        CHOPSTICKS[right].signal();
        crate::println!("⬇️ 哲学家 {} 放下了筷子 (已就餐{}次)", id, meal_count);
    }
}

fn main() -> isize {
    // 初始化信号量
    START_SEM.init(0);  // 初始化为0，等所有哲学家创建完毕后再释放
    for i in 0..PHILOSOPHER_COUNT {
        if !CHOPSTICKS[i].init(1) {
            crate::println!("初始化筷子{}失败 ❌", i);
            return -1;
        }
    }
    
    crate::println!("正在创建哲学家...");
    
    // 创建哲学家进程
    for i in 0..PHILOSOPHER_COUNT-1 {
        if sys_fork() == 0 {
            philosopher(i);
        }
    }
    
    // 等待一会儿确保所有子进程都已创建
    for _ in 0..1000000 {
        // 空循环
    }
    
    crate::println!("\n🎬 所有哲学家就绪，开始就餐实验...\n");
    
    // 释放信号量，让所有哲学家同时开始
    for _ in 0..PHILOSOPHER_COUNT {
        START_SEM.signal();
    }
    
    // 主进程作为最后一个哲学家
    philosopher(PHILOSOPHER_COUNT-1);
}

entry!(main);