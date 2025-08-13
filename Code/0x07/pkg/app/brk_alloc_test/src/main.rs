#![no_std]
#![no_main]

use lib::*;
use alloc::vec::Vec;
use alloc::string::String;
use alloc::boxed::Box;

extern crate lib;

fn main() -> isize {
    println!("🧪 开始测试 brk_alloc 内存分配器...");

    // 测试 1: Vec 分配
    println!("📍 测试 1: Vec 动态数组分配");
    let mut vec = Vec::new();
    for i in 0..100 {
        vec.push(i);
    }
    
    let sum: i32 = vec.iter().sum();
    let expected_sum = (0..100).sum::<i32>();
    
    if sum == expected_sum {
        println!("✅ Vec 测试通过! 总和: {}", sum);
    } else {
        println!("❌ Vec 测试失败! 期望: {}, 实际: {}", expected_sum, sum);
        return -1;
    }

    // 测试 2: String 分配
    println!("📍 测试 2: String 字符串分配");
    let mut s = String::new();
    s.push_str("Hello, ");
    s.push_str("brk_alloc ");
    s.push_str("allocator!");
    
    let expected = "Hello, brk_alloc allocator!";
    if s == expected {
        println!("✅ String 测试通过! 内容: '{}'", s);
    } else {
        println!("❌ String 测试失败! 期望: '{}', 实际: '{}'", expected, s);
        return -2;
    }

    // 测试 3: Box 分配
    println!("📍 测试 3: Box 堆分配");
    let boxed_array = Box::new([42u32; 256]);
    let mut all_correct = true;
    
    for (i, &value) in boxed_array.iter().enumerate() {
        if value != 42 {
            println!("❌ Box 测试失败! 位置 {} 期望: 42, 实际: {}", i, value);
            all_correct = false;
            break;
        }
    }
    
    if all_correct {
        println!("✅ Box 测试通过! 256个元素都是42");
    } else {
        return -3;
    }

    // 测试 4: 大量小分配
    println!("📍 测试 4: 大量小分配测试");
    let mut boxes = Vec::new();
    for i in 0..50 {
        let boxed_val = Box::new(i * 2);
        boxes.push(boxed_val);
    }
    
    let mut verification_passed = true;
    for (i, boxed_val) in boxes.iter().enumerate() {
        if **boxed_val != i * 2 {
            println!("❌ 大量分配测试失败! 位置 {} 期望: {}, 实际: {}", i, i * 2, **boxed_val);
            verification_passed = false;
            break;
        }
    }
    
    if verification_passed {
        println!("✅ 大量小分配测试通过! 50个Box分配成功");
    } else {
        return -4;
    }

    // 测试 5: 混合分配测试
    println!("📍 测试 5: 混合分配测试");
    let mut mixed_data = Vec::new();
    
    // 添加不同类型的数据
    for i in 0..20 {
        let s = format!("Item_{}", i);
        mixed_data.push(s);
    }
    
    // 验证数据
    let mut mixed_test_passed = true;
    for (i, item) in mixed_data.iter().enumerate() {
        let expected = format!("Item_{}", i);
        if *item != expected {
            println!("❌ 混合分配测试失败! 位置 {} 期望: '{}', 实际: '{}'", i, expected, item);
            mixed_test_passed = false;
            break;
        }
    }
    
    if mixed_test_passed {
        println!("✅ 混合分配测试通过! 20个字符串分配成功");
    } else {
        return -5;
    }

    // 测试 6: 内存释放测试
    println!("📍 测试 6: 内存释放测试");
    {
        let _temp_vec: Vec<u64> = (0..1000).collect();
        let _temp_string = "这是一个临时字符串，用于测试内存释放".repeat(10);
        let _temp_box = Box::new([0u8; 1024]);
        // 这些变量在作用域结束时会被自动释放
    }
    
    // 分配新内存验证释放是否正常
    let final_vec: Vec<i32> = (0..100).map(|x| x * x).collect();
    let final_sum: i32 = final_vec.iter().sum();
    let expected_final_sum: i32 = (0..100).map(|x| x * x).sum();
    
    if final_sum == expected_final_sum {
        println!("✅ 内存释放测试通过! 平方和: {}", final_sum);
    } else {
        println!("❌ 内存释放测试失败! 期望: {}, 实际: {}", expected_final_sum, final_sum);
        return -6;
    }

    println!("🎉 所有 brk_alloc 分配器测试通过!");
    println!("📊 测试统计:");
    println!("   - Vec 动态数组: ✅");
    println!("   - String 字符串: ✅");
    println!("   - Box 堆分配: ✅");
    println!("   - 大量小分配: ✅");
    println!("   - 混合分配: ✅");
    println!("   - 内存释放: ✅");
    
    0
}

entry!(main);