use super::LocalApic;
use bit_field::BitField;
use core::fmt::{Debug, Error, Formatter};
use core::ptr::{read_volatile, write_volatile};
use x86::cpuid::CpuId;

/// Default physical address of xAPIC
pub const LAPIC_ADDR: u64 = 0xFEE00000;

pub struct XApic {
    addr: u64,
}

impl XApic {
    pub unsafe fn new(addr: u64) -> Self {
        XApic { addr }
    }

    unsafe fn read(&self, reg: u32) -> u32 {
        unsafe {
            read_volatile((self.addr + reg as u64) as *const u32)
        }
    }

    unsafe fn write(&mut self, reg: u32, value: u32) {
        unsafe {
            write_volatile((self.addr + reg as u64) as *mut u32, value);
            self.read(0x20);
        }
    }
}

impl LocalApic for XApic {
    /// If this type APIC is supported
    fn support() -> bool {
        CpuId::new().get_feature_info().map(|f| f.has_apic()).unwrap_or(false)
    }

    /// Initialize the xAPIC for the current CPU.
    fn cpu_init(&mut self) {
        // 定义 APIC 寄存器地址常量
        const REG_ID: u32 = 0x0020;
        const REG_VERSION: u32 = 0x0030;
        const REG_EOI: u32 = 0x00B0;
        const REG_ESR: u32 = 0x0280;
        const REG_ICR_LOW: u32 = 0x0300;
        const REG_ICR_HIGH: u32 = 0x0310;
        const REG_LVT_TIMER: u32 = 0x0320;
        const REG_LVT_PERF: u32 = 0x0340;
        const REG_LVT_LINT0: u32 = 0x0350;
        const REG_LVT_LINT1: u32 = 0x0360;
        const REG_LVT_ERROR: u32 = 0x0370;
        const REG_TIMER_INIT_CNT: u32 = 0x0380;
        const REG_TIMER_DIV: u32 = 0x03E0;
        const REG_SVR: u32 = 0x00F0;

        // 定义配置位常量
        const APIC_ENABLE: u32 = 1 << 8;
        const MASKED: u32 = 1 << 16;
        const TIMER_PERIODIC: u32 = 1 << 17;
        const BCAST: u32 = 1 << 19; // 广播到所有处理器
        const INIT: u32 = 5 << 8;   // INIT De-assert 模式
        const TMLV: u32 = 1 << 15;  // TM=1, LV=0
        const DS: u32 = 1 << 12;    // 传递状态位

        // 假设的中断向量常量 - 实际项目中应使用真实定义
        const IRQ_BASE: u32 = 32;
        const IRQ_SPURIOUS: u32 = 31;
        const IRQ_TIMER: u32 = 0;
        const IRQ_ERROR: u32 = 19;

        unsafe {
            // 1. 启用本地 APIC 并设置虚假中断向量
            let mut svr = self.read(REG_SVR);
            svr |= APIC_ENABLE;  // 设置 EN 位
            svr &= !0xFF;        // 清除向量字段
            svr |= IRQ_BASE + IRQ_SPURIOUS;
            self.write(REG_SVR, svr);

            // 2. 配置定时器 - 周期模式
            let mut lvt_timer = self.read(REG_LVT_TIMER);
            lvt_timer &= !0xFF;  // 清除向量字段
            lvt_timer |= IRQ_BASE + IRQ_TIMER;
            lvt_timer &= !MASKED;  // 清除屏蔽位
            lvt_timer |= TIMER_PERIODIC;  // 设置周期模式
            self.write(REG_LVT_TIMER, lvt_timer);
            
            // 设置分频系数为 1
            self.write(REG_TIMER_DIV, 0b1011);
            // 设置初始计数值
            self.write(REG_TIMER_INIT_CNT, 0x40000);

            // 3. 禁用逻辑中断线 LINT0, LINT1
            self.write(REG_LVT_LINT0, MASKED);
            self.write(REG_LVT_LINT1, MASKED);
            
            // 4. 禁用性能计数器溢出中断
            self.write(REG_LVT_PERF, MASKED);
            
            // 5. 映射错误中断
            let mut lvt_error = self.read(REG_LVT_ERROR);
            lvt_error &= !0xFF;  // 清除向量字段
            lvt_error |= IRQ_BASE + IRQ_ERROR;
            self.write(REG_LVT_ERROR, lvt_error);
            
            // 6. 清除错误状态寄存器 (需要连续两次写入)
            self.write(REG_ESR, 0);
            self.write(REG_ESR, 0);
            
            // 7. 确认未处理的中断
            self.eoi();
            
            // 8. 发送 Init Level De-Assert 以同步仲裁 ID
            self.write(REG_ICR_HIGH, 0);  // 设置高位
            self.write(REG_ICR_LOW, BCAST | INIT | TMLV);  // 设置低位
            
            // 等待传递完成
            while self.read(REG_ICR_LOW) & DS != 0 {}
        }
    }

    fn id(&self) -> u32 {
        // NOTE: Maybe you can handle regs like `0x0300` as a const.
        unsafe { self.read(0x0020) >> 24 }
    }

    fn version(&self) -> u32 {
        unsafe { self.read(0x0030) }
    }

    fn icr(&self) -> u64 {
        unsafe { (self.read(0x0310) as u64) << 32 | self.read(0x0300) as u64 }
    }

    fn set_icr(&mut self, value: u64) {
        unsafe {
            while self.read(0x0300).get_bit(12) {}
            self.write(0x0310, (value >> 32) as u32);
            self.write(0x0300, value as u32);
            while self.read(0x0300).get_bit(12) {}
        }
    }

    fn eoi(&mut self) {
        unsafe {
            self.write(0x00B0, 0);
        }
    }
}

impl Debug for XApic {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        f.debug_struct("Xapic")
            .field("id", &self.id())
            .field("version", &self.version())
            .field("icr", &self.icr())
            .finish()
    }
}
