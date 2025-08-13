use super::uart16550::SerialPort;

const SERIAL_IO_PORT: u16 = 0x3F8; // COM1

// 使用常量泛型定义串口
once_mutex!(pub SERIAL: SerialPort<SERIAL_IO_PORT>);

pub fn init() {
    // 不再需要传入端口地址
    init_SERIAL(SerialPort::<SERIAL_IO_PORT>::new());
    get_serial_for_sure().init();
    
    // 添加清屏转义序列
    // \x1b[2J 是清屏命令
    // \x1b[H 是将光标移到屏幕左上角
    print!("\x1b[2J\x1b[H");
    
    println!("{}", crate::get_ascii_header());
    println!("[+] Serial Initialized.");
}

guard_access_fn!(pub get_serial(SERIAL: SerialPort<SERIAL_IO_PORT>));
