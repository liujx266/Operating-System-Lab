use super::consts::*;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};
use crate::drivers::uart16550::SerialPort;
use core::str;
use log::trace;

// Static buffer for UTF-8 decoding. Unsafe in concurrent contexts.
static mut UTF8_BUF: [u8; 4] = [0; 4];
static mut UTF8_LEN: usize = 0;

use crate::drivers::input;

pub unsafe fn register_idt(idt: &mut InterruptDescriptorTable) {
    idt[Interrupts::IrqBase as u8 + Irq::Serial0 as u8]
        .set_handler_fn(serial_handler);
}

pub extern "x86-interrupt" fn serial_handler(_st: InterruptStackFrame) {
    receive();
    super::ack();
}

/// 从串口接收字符并放入输入缓冲区
/// 在每次中断时调用
/// Handles UTF-8 decoding.
fn receive() {
    // 创建串口实例
    let mut serial = SerialPort::<0x3F8>::new();
    
    while let Some(byte) = serial.receive() {
        trace!("Serial received byte: {:#02x}", byte);
        unsafe {
            const UTF8_BUF_SIZE: usize = 4; // Use a constant for buffer size
            if UTF8_LEN >= UTF8_BUF_SIZE {
                // Buffer full, but no valid char yet. This indicates an error or
                // a character longer than 4 bytes (which shouldn't happen with standard UTF-8).
                // Push replacement char and reset.
                input::push_char('\u{FFFD}');
                UTF8_LEN = 0;
                // Try processing the current byte as the start of a new sequence
            }

            UTF8_BUF[UTF8_LEN] = byte;
            UTF8_LEN += 1;

            // Copy the relevant part of the static buffer to a local buffer
            // to avoid creating a shared reference to `static mut`.
            let current_bytes = &UTF8_BUF[0..UTF8_LEN];
            match str::from_utf8(current_bytes) {
                Ok(s) => {
                    // Successfully decoded a character(s).
                    // Since we add byte-by-byte, `s` should contain exactly one char when Ok.
                    if let Some(c) = s.chars().next() {
                        trace!("Decoded char: {:?}", c);
                        match c {
                            '\r' => input::push_newline(), // Treat carriage return as newline
                            '\x08' | '\x7f' => input::push_backspace(), // Backspace or Delete
                            _ => input::push_char(c),
                        }
                    }
                    UTF8_LEN = 0; // Reset buffer for next character
                }
                Err(e) => {
                    if e.error_len().is_none() {
                        // Incomplete sequence, need more bytes. Continue loop.
                        trace!("Incomplete UTF-8 sequence: {:?}", current_bytes);
                    } else {
                        // Invalid sequence found. Push replacement char and reset.
                        input::push_char('\u{FFFD}');
                        UTF8_LEN = 0;
                    }
                }
            }
        }
    }
}
