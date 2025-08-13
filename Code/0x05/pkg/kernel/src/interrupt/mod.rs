mod apic;
mod consts;
pub mod clock;
mod serial;  // 添加 serial 模块
mod exceptions;
pub mod syscall;

use apic::*;
use x86_64::structures::idt::InterruptDescriptorTable;
use crate::memory::physical_to_virtual;

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();
        unsafe {
            exceptions::register_idt(&mut idt);
            clock::register_idt(&mut idt);
            serial::register_idt(&mut idt);  // 注册串口中断
            syscall::register_idt(&mut idt); // 注册系统调用中断
        }
        idt
    };
}

/// init interrupts system
pub fn init() {
    IDT.load();

    // 初始化APIC
    if XApic::support() {
        let mut lapic = unsafe { XApic::new(physical_to_virtual(LAPIC_ADDR)) };
        lapic.cpu_init();
        
        // 启用计时器中断
        enable_irq(consts::Irq::Timer as u8, lapic.id() as u8);
        
        // 启用串口中断
        enable_irq(consts::Irq::Serial0 as u8, lapic.id() as u8);
        
        info!("APIC initialized.");
    } else {
        warn!("APIC not supported!");
    }

    info!("Interrupts Initialized.");
}

#[inline(always)]
pub fn enable_irq(irq: u8, cpuid: u8) {
    let mut ioapic = unsafe { IoApic::new(physical_to_virtual(IOAPIC_ADDR)) };
    ioapic.enable(irq, cpuid);
}

#[inline(always)]
pub fn ack() {
    let mut lapic = unsafe { XApic::new(physical_to_virtual(LAPIC_ADDR)) };
    lapic.eoi();
}
