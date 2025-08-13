#![no_std]
#![no_main]

use lib::{entry, print, println, stdin, sys_list_app, sys_stat, sys_spawn, sys_wait_pid};
use lib::alloc::string::String;
use lib::alloc::vec::Vec;
use lib::alloc::borrow::ToOwned;

// 学号，请将它替换为您的实际学号
const STUDENT_ID: &str = "23336152";
const AUTO_COMMANDS: &[&str] = &["help", "ls", "ps"];

fn main() -> isize {
    println!("欢迎使用YSOS Shell!");
    println!("输入 help 获取帮助信息");
    
    // 自动执行一些命令来展示功能
    // println!("\n[自动执行模式开始]");
    // for &cmd in AUTO_COMMANDS {
    //     process_command(cmd, &[]);
    // }
    // println!("[自动执行模式结束]\n");
    
    loop {
        print!("ysos> ");
        
        // 使用已修复的read_line方法读取输入
        let input = stdin().read_line();
        
        // 去掉输入两端的空白字符
        let input = input.trim();
        
        if input.is_empty() {
            continue;
        }
        
        // 解析命令和参数
        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }
        
        let command = parts[0];
        let args = &parts[1..];
        
        if command == "exit" {
            println!("退出Shell...");
            return 0;
        }
        
        process_command(command, args);
    }
}

fn process_command(command: &str, args: &[&str]) {
    match command {
        "help" => {
            println!("YSOS Shell - 可用命令：");
            println!("  help        显示此帮助信息");
            println!("  ls          列出所有可用的应用程序");
            println!("  ps          列出当前运行的所有进程");
            println!("  run <程序>  运行指定的程序");
            println!("  clear       清空屏幕");
            println!("  exit        退出Shell");
            println!("学号: {}", STUDENT_ID);
        },
        "ls" => {
            println!("可用的应用程序列表：");
            sys_list_app();
        },
        "ps" => {
            println!("当前运行的进程列表：");
            sys_stat();
        },
        "run" => {
            if args.is_empty() {
                println!("错误: 请指定要运行的程序名");
            } else {
                let program = args[0];
                println!("正在运行程序: {}", program);
                
                // 生成新进程
                let pid = sys_spawn(program);
                
                if pid == 0 {
                    println!("错误: 无法运行程序 '{}'", program);
                } else {
                    println!("进程ID: {}", pid);
                    
                    // 等待进程结束
                    let exit_code = sys_wait_pid(pid);
                    println!("程序 '{}' 已退出，返回值: {}", program, exit_code);
                }
            }
        },
        "clear" => {
            // 通过打印ANSI转义序列清空屏幕
            print!("\x1B[2J\x1B[1;1H");
        },
        "factorial" => {
            // 直接运行阶乘测试程序
            execute_factorial_test();
        },
        _ => {
            println!("未知命令: '{}'", command);
            println!("输入 help 获取可用命令列表");
        },
    }
}

// 阶乘测试程序
fn execute_factorial_test() {
    println!("正在运行阶乘测试程序");
    
    // 生成进程
    let pid = sys_spawn("factorial");
    
    if pid == 0 {
        println!("错误: 无法运行阶乘测试程序");
        return;
    }
    
    println!("阶乘测试进程ID: {}", pid);
    
    // 等待进程结束
    let exit_code = sys_wait_pid(pid);
    println!("阶乘测试程序已退出，返回值: {}", exit_code);
}

entry!(main);