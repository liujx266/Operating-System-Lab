use crate::*;
use alloc::string::{String, ToString};
use alloc::vec;

pub struct Stdin;
pub struct Stdout;
pub struct Stderr;

impl Stdin {
    fn new() -> Self {
        Self
    }

    pub fn read_line(&self) -> String {
        // 分配一个String用于存储输入
        let mut input = String::new();
        let mut buffer = [0u8; 1];
        
        loop {
            // 读取一个字符
            if let Some(n) = sys_read(0, &mut buffer) {
                if n == 0 {
                    continue;
                }
                
                match buffer[0] {
                    // 回车键，结束输入
                    b'\n' | b'\r' => {
                        stdout().write("\n");
                        break;
                    },
                    // 退格键，删除一个字符
                    8 | 127 => {
                        if !input.is_empty() {
                            input.pop();
                            // 在终端上显示退格效果（移动光标并清除字符）
                            stdout().write("\u{8} \u{8}");
                        }
                    },
                    // 普通可打印字符
                    32..=126 => {
                        let c = buffer[0] as char;
                        input.push(c);
                        // 回显字符
                        let echo = [c as u8];
                        stdout().write(unsafe { core::str::from_utf8_unchecked(&echo) });
                    },
                    // 其他控制字符忽略
                    _ => {}
                }
            }
        }
        
        input
    }
}

impl Stdout {
    fn new() -> Self {
        Self
    }

    pub fn write(&self, s: &str) {
        sys_write(1, s.as_bytes());
    }
}

impl Stderr {
    fn new() -> Self {
        Self
    }

    pub fn write(&self, s: &str) {
        sys_write(2, s.as_bytes());
    }
}

pub fn stdin() -> Stdin {
    Stdin::new()
}

pub fn stdout() -> Stdout {
    Stdout::new()
}

pub fn stderr() -> Stderr {
    Stderr::new()
}
