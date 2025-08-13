#![no_std]
#![no_main]

use lib::{entry, print, println, stdin, sys_list_app, sys_stat, sys_spawn, sys_wait_pid, sys_list_dir, sys_open, sys_close, sys_read};

use lib::alloc::vec::Vec;

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
            println!("  help           显示此帮助信息");
            println!("  ls [路径]      列出目录内容（默认为根目录）");
            println!("  cat <文件>     显示文件内容");
            println!("  apps           列出所有可用的应用程序");
            println!("  ps             列出当前运行的所有进程");
            println!("  run <程序>     运行指定的程序（支持文件路径，如 /factorial）");
            println!("  clear          清空屏幕");
            println!("  exit           退出Shell");
            println!("学号: {}", STUDENT_ID);
        },
        "ls" => {
            let path = if args.is_empty() { "/" } else { args[0] };
            println!("目录内容 '{}':", path);
            sys_list_dir(path);
        },
        "cat" => {
            if args.is_empty() {
                println!("错误: 请指定要显示的文件名");
            } else {
                let filename = args[0];
                cat_file(filename);
            }
        },
        "apps" => {
            println!("可用的应用程序列表：");
            sys_list_app();
        },
        "ps" => {
            println!("当前运行的进程列表：");
            sys_stat();
        },
        "run" => {
            if args.is_empty() {
                println!("错误: 请指定要运行的程序路径或名称");
            } else {
                let program = args[0];
                println!("正在运行程序: {}", program);

                // 生成新进程（现在支持文件路径）
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

fn cat_file(filename: &str) {
    // 尝试打开文件
    let fd = sys_open(filename);
    if fd == 0 {
        println!("错误: 无法打开文件 '{}'", filename);
        return;
    }

    // 读取文件内容
    let mut buffer = [0u8; 1024];
    let mut total_read = 0;

    loop {
        match sys_read(fd, &mut buffer) {
            Some(bytes_read) => {
                if bytes_read == 0 {
                    break; // 文件结束
                }

                // 将读取的字节转换为字符串并打印
                if let Ok(content) = core::str::from_utf8(&buffer[..bytes_read]) {
                    print!("{}", content);
                } else {
                    println!("错误: 文件 '{}' 包含非UTF-8数据", filename);
                    break;
                }

                total_read += bytes_read;

                // 如果读取的字节数少于缓冲区大小，说明已到文件末尾
                if bytes_read < buffer.len() {
                    break;
                }
            }
            None => {
                println!("错误: 读取文件 '{}' 失败", filename);
                break;
            }
        }
    }

    // 关闭文件
    sys_close(fd);

    if total_read == 0 {
        println!("文件 '{}' 为空", filename);
    }
}

entry!(main);