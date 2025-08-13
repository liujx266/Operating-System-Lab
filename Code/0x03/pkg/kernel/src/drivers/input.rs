use crossbeam_queue::ArrayQueue;
use crate::drivers::uart16550::SerialPort;
use alloc::string::String;
use log::warn;


/// Represents different types of input events.
#[derive(Debug, Clone, Copy)]
pub enum InputKey {
    Char(char),
    Backspace,
    Newline,
}

lazy_static! {
    static ref INPUT_BUF: ArrayQueue<InputKey> = ArrayQueue::new(128);
}

/// Pushes a character key into the input buffer.
#[inline]
pub fn push_char(key: char) {
    if INPUT_BUF.push(InputKey::Char(key)).is_err() {
        warn!("Input buffer is full. Dropping key '{:?}'", key);
    }
}

/// Pushes a backspace key event into the input buffer.
#[inline]
pub fn push_backspace() {
    if INPUT_BUF.push(InputKey::Backspace).is_err() {
        warn!("Input buffer is full. Dropping backspace");
    }
}

/// Pushes a newline key event into the input buffer.
#[inline]
pub fn push_newline() {
    if INPUT_BUF.push(InputKey::Newline).is_err() {
        warn!("Input buffer is full. Dropping newline");
    }
}

/// 尝试从缓冲区获取一个按键，如果没有则返回 None
#[inline]
pub fn try_pop_key() -> Option<InputKey> {
    INPUT_BUF.pop()
}

/// 阻塞直到有按键可用
pub fn pop_key() -> InputKey {
    loop {
        if let Some(key) = try_pop_key() {
            return key;
        }
        // 使用 hint::spin_loop 优化等待循环
        core::hint::spin_loop();
    }
}

/// 读取一行输入，直到遇到换行符
pub fn get_line() -> String {
    // 创建一个预分配容量的字符串
    let mut line = String::with_capacity(64);
    // 使用 COM1 端口创建串口实例
    let mut serial = SerialPort::<0x3F8>::new();
    
    loop {
        let input_key = pop_key();
        
        // 移除此日志，避免输入时的日志干扰
        // trace!("Popped key: {:?}", input_key);
        
        match input_key {
            InputKey::Newline => {
                serial.send(b'\r');
                serial.send(b'\n');
                break;
            }
            
            InputKey::Backspace => {
                if !line.is_empty() {
                    line.pop();
                    serial.backspace();
                }
            }
            
            InputKey::Char(c) => {
                line.push(c);
                let mut buf = [0u8; 4];
                for byte in c.encode_utf8(&mut buf).as_bytes() {
                    serial.send(*byte);
                }
            }
        }
    }
    
    line
}
