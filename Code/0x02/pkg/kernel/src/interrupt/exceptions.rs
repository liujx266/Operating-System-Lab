// 这个文件负责处理 x86_64 CPU 架构中的所有异常
// 异常是 CPU 内部产生的事件，用于通知操作系统需要处理的特殊情况
use crate::memory::*;
use x86_64::registers::control::Cr2;  // 用于访问 CR2 寄存器，其中存储了页错误的地址
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};
use x86_64::VirtAddr;

/// 将所有 CPU 异常处理程序注册到中断描述符表(IDT)
///
/// x86_64 架构定义了 20 多种不同的异常：
/// - 0-31 号向量被保留用于 CPU 异常
/// - 其余的可由操作系统用于硬件中断
pub unsafe fn register_idt(idt: &mut InterruptDescriptorTable) {
    // 基本错误：数学和基本操作相关的异常
    idt.divide_error.set_handler_fn(divide_error_handler);  // 除零错误 (#DE)
    idt.debug.set_handler_fn(debug_handler);  // 调试异常 (#DB)
    idt.non_maskable_interrupt.set_handler_fn(nmi_handler);  // 不可屏蔽中断，硬件级的严重错误
    idt.breakpoint.set_handler_fn(breakpoint_handler);  // 断点异常 (#BP)，用于调试
    idt.overflow.set_handler_fn(overflow_handler);  // 溢出异常 (#OF)
    idt.bound_range_exceeded.set_handler_fn(bound_range_exceeded_handler);  // 数组访问越界 (#BR)
    idt.invalid_opcode.set_handler_fn(invalid_opcode_handler);  // 无效操作码 (#UD)
    idt.device_not_available.set_handler_fn(device_not_available_handler);  // 设备不可用 (#NM)
    
    // 双重错误需要特殊处理，发生在处理一个异常时又触发了另一个异常
    // 使用单独的栈以防止栈溢出导致三重错误（系统重置）
    unsafe {
        idt.double_fault
            .set_handler_fn(double_fault_handler)
            .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);  // 使用专用栈处理双重错误
    }
    
    // 内存和段错误相关的异常
    idt.invalid_tss.set_handler_fn(invalid_tss_handler);  // 无效的 TSS (#TS)
    idt.segment_not_present.set_handler_fn(segment_not_present_handler);  // 段不存在 (#NP)
    idt.stack_segment_fault.set_handler_fn(stack_segment_fault_handler);  // 栈段错误 (#SS)
    idt.general_protection_fault.set_handler_fn(general_protection_fault_handler);  // 通用保护错误 (#GP)
    
    // 页错误也需要特殊处理，使用专用栈防止在处理页错误时发生栈溢出
    unsafe {
        idt.page_fault
            .set_handler_fn(page_fault_handler)
            .set_stack_index(gdt::PAGE_FAULT_IST_INDEX);  // 使用专用栈处理页错误
    }
    
    // 浮点和 SIMD 相关的异常
    idt.x87_floating_point.set_handler_fn(x87_floating_point_handler);  // x87 FPU 错误 (#MF)
    idt.alignment_check.set_handler_fn(alignment_check_handler);  // 对齐检查错误 (#AC)
    idt.machine_check.set_handler_fn(machine_check_handler);  // 机器检查异常 (#MC)，可能是硬件故障
    idt.simd_floating_point.set_handler_fn(simd_floating_point_handler);  // SIMD 浮点异常 (#XF)
    
    // 虚拟化和安全相关的异常
    idt.virtualization.set_handler_fn(virtualization_handler);  // 虚拟化异常 (#VE)
    idt.security_exception.set_handler_fn(security_exception_handler);  // 安全异常 (#SX)
}

/// 处理除零错误 (向量 0)
/// 当程序尝试除以零时触发
pub extern "x86-interrupt" fn divide_error_handler(stack_frame: InterruptStackFrame) {
    panic!("EXCEPTION: DIVIDE ERROR\n\n{:#?}", stack_frame);
}

/// 处理调试异常 (向量 1)
/// 当启用调试功能并触发断点条件时触发
pub extern "x86-interrupt" fn debug_handler(stack_frame: InterruptStackFrame) {
    panic!("EXCEPTION: DEBUG\n\n{:#?}", stack_frame);
}

/// 处理不可屏蔽中断 (向量 2)
/// 通常由硬件故障引起，无法被 CLI 指令屏蔽
pub extern "x86-interrupt" fn nmi_handler(stack_frame: InterruptStackFrame) {
    panic!("EXCEPTION: NON-MASKABLE INTERRUPT\n\n{:#?}", stack_frame);
}

/// 处理断点异常 (向量 3)
/// 由 INT3 指令触发，常用于调试器实现
pub extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    panic!("EXCEPTION: BREAKPOINT\n\n{:#?}", stack_frame);
}

/// 处理溢出异常 (向量 4)
/// 当 INTO 指令检测到溢出时触发
pub extern "x86-interrupt" fn overflow_handler(stack_frame: InterruptStackFrame) {
    panic!("EXCEPTION: OVERFLOW\n\n{:#?}", stack_frame);
}

/// 处理数组边界超出异常 (向量 5)
/// 当 BOUND 指令检测到数组索引超出范围时触发
pub extern "x86-interrupt" fn bound_range_exceeded_handler(stack_frame: InterruptStackFrame) {
    panic!("EXCEPTION: BOUND RANGE EXCEEDED\n\n{:#?}", stack_frame);
}

/// 处理无效操作码异常 (向量 6)
/// 当 CPU 遇到无法识别的指令时触发
pub extern "x86-interrupt" fn invalid_opcode_handler(stack_frame: InterruptStackFrame) {
    panic!("EXCEPTION: INVALID OPCODE\n\n{:#?}", stack_frame);
}

/// 处理设备不可用异常 (向量 7)
/// 当尝试使用 x87 FPU 而它不存在或被禁用时触发
pub extern "x86-interrupt" fn device_not_available_handler(stack_frame: InterruptStackFrame) {
    panic!("EXCEPTION: DEVICE NOT AVAILABLE\n\n{:#?}", stack_frame);
}

/// 处理双重错误 (向量 8)
/// 当异常处理过程中发生第二个异常时触发
/// 这是一个严重错误，如果不处理会导致系统重置
/// 返回 '!' 表示此函数永不返回
pub extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) -> ! {
    panic!(
        "EXCEPTION: DOUBLE FAULT, ERROR_CODE: 0x{:016x}\n\n{:#?}",
        error_code, stack_frame
    );
}

/// 处理无效 TSS 异常 (向量 10)
/// 当任务状态段(TSS)中包含无效数据时触发
pub extern "x86-interrupt" fn invalid_tss_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    panic!(
        "EXCEPTION: INVALID TSS, ERROR_CODE: 0x{:016x}\n\n{:#?}",
        error_code, stack_frame
    );
}

/// 处理段不存在异常 (向量 11)
/// 当程序尝试使用不存在的段或描述符时触发
pub extern "x86-interrupt" fn segment_not_present_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    panic!(
        "EXCEPTION: SEGMENT NOT PRESENT, ERROR_CODE: 0x{:016x}\n\n{:#?}",
        error_code, stack_frame
    );
}

/// 处理栈段错误 (向量 12)
/// 当栈操作导致段限制被超过时触发
pub extern "x86-interrupt" fn stack_segment_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    panic!(
        "EXCEPTION: STACK SEGMENT FAULT, ERROR_CODE: 0x{:016x}\n\n{:#?}",
        error_code, stack_frame
    );
}

/// 处理通用保护错误 (向量 13)
/// 这是最常见的异常之一，当程序违反保护机制时触发
/// 例如：访问非法内存地址、执行特权指令等
pub extern "x86-interrupt" fn general_protection_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    panic!(
        "EXCEPTION: GENERAL PROTECTION FAULT, ERROR_CODE: 0x{:016x}\n\n{:#?}",
        error_code, stack_frame
    );
}

/// 处理页错误 (向量 14)
/// 当内存分页操作失败时触发，如访问未映射的页面
/// PageFaultErrorCode 包含具体错误信息：是否因写操作触发、是否是特权级访问等
/// CR2 寄存器中存储了导致页错误的内存地址
pub extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    err_code: PageFaultErrorCode,
) {
    panic!(
        "EXCEPTION: PAGE FAULT, ERROR_CODE: {:?}\n\nTrying to access: {:#x}\n{:#?}",
        err_code,
        Cr2::read().unwrap_or(VirtAddr::new_truncate(0xdeadbeef)),
        stack_frame
    );
}

/// 处理 x87 浮点异常 (向量 16)
/// 当 x87 FPU 操作发生错误时触发
pub extern "x86-interrupt" fn x87_floating_point_handler(stack_frame: InterruptStackFrame) {
    panic!("EXCEPTION: X87 FLOATING POINT\n\n{:#?}", stack_frame);
}

/// 处理对齐检查异常 (向量 17)
/// 当启用对齐检查时，访问未对齐的内存地址时触发
pub extern "x86-interrupt" fn alignment_check_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    panic!(
        "EXCEPTION: ALIGNMENT CHECK, ERROR_CODE: 0x{:016x}\n\n{:#?}",
        error_code, stack_frame
    );
}

/// 处理机器检查异常 (向量 18)
/// 这是硬件级别的严重错误信号，如内存或总线错误
/// 返回 '!' 表示此函数永不返回
pub extern "x86-interrupt" fn machine_check_handler(stack_frame: InterruptStackFrame) -> ! {
    panic!("EXCEPTION: MACHINE CHECK\n\n{:#?}", stack_frame);
}

/// 处理 SIMD 浮点异常 (向量 19)
/// 当 SSE/AVX 等 SIMD 指令执行错误时触发
pub extern "x86-interrupt" fn simd_floating_point_handler(stack_frame: InterruptStackFrame) {
    panic!("EXCEPTION: SIMD FLOATING POINT\n\n{:#?}", stack_frame);
}

/// 处理虚拟化异常 (向量 20)
/// 与虚拟机操作相关的异常
pub extern "x86-interrupt" fn virtualization_handler(stack_frame: InterruptStackFrame) {
    panic!("EXCEPTION: VIRTUALIZATION\n\n{:#?}", stack_frame);
}

/// 处理安全异常 (向量 30)
/// 与 CPU 安全机制相关的异常
pub extern "x86-interrupt" fn security_exception_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    panic!(
        "EXCEPTION: SECURITY EXCEPTION, ERROR_CODE: 0x{:016x}\n\n{:#?}",
        error_code, stack_frame
    );
}
