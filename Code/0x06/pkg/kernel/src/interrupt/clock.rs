use super::consts::*;
use crate::proc::ProcessContext;
use crate::as_handler;
use crate::memory::gdt::TIMER_IST_INDEX;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};

pub unsafe fn register_idt(idt: &mut InterruptDescriptorTable) {
    // 设置时钟中断处理函数并指定使用独立的栈空间
    unsafe {
        idt[Interrupts::IrqBase as u8 + Irq::Timer as u8]
            .set_handler_fn(clock_handler)
            .set_stack_index(TIMER_IST_INDEX);
    }
}

// 实际的时钟中断处理逻辑
pub extern "C" fn clock(mut context: ProcessContext) {
    // 在这里调用进程切换函数
    crate::proc::switch(&mut context);
    
    // 发送中断结束信号
    super::ack();
    
    // 调试用：每隔一段时间打印进程列表
    // static mut COUNTER: u64 = 0;
    // unsafe {
    //     COUNTER += 1;
    //     if COUNTER % 100 == 0 {
    //         crate::proc::print_process_list();
    //     }
    // }
}

// 使用as_handler宏生成真正的中断处理函数
as_handler!(clock);