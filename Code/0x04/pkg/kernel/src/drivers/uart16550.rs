use core::fmt;
use x86_64::instructions::port::{Port, PortReadOnly, PortWriteOnly};

/// A port-mapped UART 16550 serial interface.
pub struct SerialPort<const BASE_ADDR: u16>;

impl<const BASE_ADDR: u16> SerialPort<BASE_ADDR> {
    pub const fn new() -> Self {
        Self
    }

    /// Initializes the serial port.
    pub fn init(&self) {
        // 禁用所有中断
        let mut interrupt_enable = PortWriteOnly::new(BASE_ADDR + 1);
        unsafe {
            interrupt_enable.write(0x00_u8);
        }
        
        // 启用 DLAB (设置波特率除数)
        let mut line_control = PortWriteOnly::new(BASE_ADDR + 3);
        unsafe {
            line_control.write(0x80_u8);
        }
        
        // 设置除数为 3 (低字节) 38400 波特率
        let mut divisor_lo = PortWriteOnly::new(BASE_ADDR + 0);
        unsafe {
            divisor_lo.write(0x03_u8);
        }
        
        // 高字节设置为 0
        let mut divisor_hi = PortWriteOnly::new(BASE_ADDR + 1);
        unsafe {
            divisor_hi.write(0x00_u8);
        }
        
        // 8 位数据, 无奇偶校验, 一个停止位
        unsafe {
            line_control.write(0x03_u8);
        }
        
        // 启用 FIFO, 清空, 使用 14 字节阈值
        let mut fifo_control = PortWriteOnly::new(BASE_ADDR + 2);
        unsafe {
            fifo_control.write(0xC7_u8);
        }
        
        // 启用 IRQ, 设置 RTS/DSR
        let mut modem_control = PortWriteOnly::new(BASE_ADDR + 4);
        unsafe {
            modem_control.write(0x0B_u8);
        }
        
        // 设置回环模式, 测试串行芯片
        unsafe {
            modem_control.write(0x1E_u8);
        }
        
        // 测试串行芯片 (发送 0xAE 字节并检查返回值)
        let mut data = Port::new(BASE_ADDR);
        unsafe {
            data.write(0xAE_u8);
        }
        
        // 检查串口是否有故障 (返回字节是否与发送字节相同)
        unsafe {
            if data.read() != 0xAE_u8 {
                panic!("Serial port initialization failed");
            }
        }
        
        // 设置为正常操作模式 (非回环模式, IRQ 启用, OUT#1 和 OUT#2 位启用)
        unsafe {
            modem_control.write(0x0F_u8);
        }
        
        // 启用接收数据中断
        unsafe {
            interrupt_enable.write(0x01_u8);
        }
    }

    /// Sends a byte on the serial port.
    pub fn send(&mut self, data: u8) {
        // 等待发送缓冲区为空
        let mut line_status = PortReadOnly::<u8>::new(BASE_ADDR + 5);
        // 检查 Line Status Register 的 Transmitter Holding Register Empty 位(第5位)
        while unsafe { (line_status.read() & 0x20) == 0 } {
            // 忙等待直到发送缓冲区为空
        }
        
        // 发送字节
        let mut data_port = Port::new(BASE_ADDR);
        unsafe {
            data_port.write(data);
        }
    }

    /// Receives a byte on the serial port no wait.
    pub fn receive(&mut self) -> Option<u8> {
        let mut line_status = PortReadOnly::<u8>::new(BASE_ADDR + 5);
        // 检查 Line Status Register 的 Data Ready 位(第0位)
        if unsafe { (line_status.read() & 0x01) == 0 } {
            // 无数据可读
            None
        } else {
            // 有数据可读, 读取数据
            let mut data_port = Port::new(BASE_ADDR);
            unsafe {
                Some(data_port.read())
            }
        }
    }
    
    /// 发送退格控制符序列 (用于删除一个字符)
    pub fn backspace(&mut self) {
        self.send(0x08); // 后退
        self.send(0x20); // 空格覆盖
        self.send(0x08); // 再次后退
    }
}

impl<const BASE_ADDR: u16> fmt::Write for SerialPort<BASE_ADDR> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            self.send(byte);
        }
        Ok(())
    }
}
