#![no_std]
#![no_main]

use ysos::*;
use ysos_kernel as ysos;

extern crate alloc;

boot::entry_point!(kernel_main);

pub fn kernel_main(boot_info: &'static boot::BootInfo) -> ! {
    ysos::init(boot_info);
    ysos::wait(spawn_init()); 
    ysos::shutdown();
}

pub fn spawn_init() -> proc::ProcessId {
    // NOTE: you may want to clear the screen before starting the shell
    // print!("\x1b[1;1H\x1b[2J");

    proc::list_app();

    // 使用修改后的spawn函数，首先尝试从文件系统加载Shell
    // 如果失败，会自动回退到bootloader应用
    proc::spawn("/shell").or_else(|| proc::spawn("shell")).unwrap()
}
