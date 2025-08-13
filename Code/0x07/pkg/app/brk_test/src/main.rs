#![no_std]
#![no_main]

use lib::*;

extern crate lib;

const HEAP_SIZE: usize = 4096; // 4KB 测试堆大小

fn main() -> isize {
    println!("🧪 开始测试 brk 系统调用...");

    // 1. 获取当前堆的起始地址
    println!("📍 步骤 1: 获取当前堆起始地址");
    let heap_start = match sys_brk(None) {
        Some(addr) => {
            println!("✅ 堆起始地址: 0x{:x}", addr);
            addr
        }
        None => {
            println!("❌ 获取堆起始地址失败!");
            return -1;
        }
    };

    // 2. 计算目标堆结束地址
    let heap_end = heap_start + HEAP_SIZE;
    println!("📍 步骤 2: 计算目标堆结束地址: 0x{:x} (大小: {} 字节)", heap_end, HEAP_SIZE);

    // 3. 调用 brk 扩展堆
    println!("📍 步骤 3: 扩展堆到目标地址");
    let ret = match sys_brk(Some(heap_end)) {
        Some(addr) => {
            println!("✅ brk 返回地址: 0x{:x}", addr);
            addr
        }
        None => {
            println!("❌ 扩展堆失败!");
            return -2;
        }
    };

    // 4. 验证返回地址是否正确
    if ret != heap_end {
        println!("❌ 堆扩展失败! 期望: 0x{:x}, 实际: 0x{:x}", heap_end, ret);
        return -3;
    }
    println!("✅ 堆扩展成功!");

    // 5. 测试写入和读取操作
    println!("📍 步骤 4: 测试堆内存的写入和读取");
    
    // 将堆起始地址转换为可写的指针
    let heap_ptr = heap_start as *mut u8;
    
    unsafe {
        // 写入测试数据
        println!("📝 写入测试数据...");
        for i in 0..256 {
            *heap_ptr.add(i) = (i % 256) as u8;
        }
        
        // 读取并验证数据
        println!("📖 读取并验证数据...");
        let mut success = true;
        for i in 0..256 {
            let expected = (i % 256) as u8;
            let actual = *heap_ptr.add(i);
            if actual != expected {
                println!("❌ 数据验证失败! 位置: {}, 期望: {}, 实际: {}", i, expected, actual);
                success = false;
                break;
            }
        }
        
        if success {
            println!("✅ 数据写入和读取测试通过!");
        } else {
            return -4;
        }
    }

    // 6. 测试堆缩小
    println!("📍 步骤 5: 测试堆缩小");
    let smaller_heap_end = heap_start + HEAP_SIZE / 2;
    match sys_brk(Some(smaller_heap_end)) {
        Some(addr) => {
            if addr == smaller_heap_end {
                println!("✅ 堆缩小成功! 新结束地址: 0x{:x}", addr);
            } else {
                println!("❌ 堆缩小失败! 期望: 0x{:x}, 实际: 0x{:x}", smaller_heap_end, addr);
                return -5;
            }
        }
        None => {
            println!("❌ 堆缩小调用失败!");
            return -6;
        }
    }

    // 7. 最终验证当前堆状态
    println!("📍 步骤 6: 验证最终堆状态");
    match sys_brk(None) {
        Some(addr) => {
            println!("✅ 当前堆结束地址: 0x{:x}", addr);
            if addr == smaller_heap_end {
                println!("🎉 所有测试通过! brk 系统调用工作正常!");
                return 0;
            } else {
                println!("❌ 最终状态验证失败! 期望: 0x{:x}, 实际: 0x{:x}", smaller_heap_end, addr);
                return -7;
            }
        }
        None => {
            println!("❌ 获取最终堆状态失败!");
            return -8;
        }
    }
}

entry!(main);