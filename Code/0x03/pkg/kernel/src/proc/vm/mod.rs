use alloc::format;
use x86_64::{
    structures::paging::{page::*, *},
    VirtAddr,
};

use crate::{humanized_size, memory::*};
// 从正确的路径导入elf，elf是一个单独的crate
extern crate ysos_elf as elf;

pub mod stack;

use self::stack::*;

use super::{PageTableContext, ProcessId};

type MapperRef<'a> = &'a mut OffsetPageTable<'static>;
type FrameAllocatorRef<'a> = &'a mut BootInfoFrameAllocator;

pub struct ProcessVm {
    // page table is shared by parent and child
    pub(super) page_table: PageTableContext,

    // stack is pre-process allocated
    pub(super) stack: Stack,
}

impl ProcessVm {
    pub fn new(page_table: PageTableContext) -> Self {
        Self {
            page_table,
            stack: Stack::empty(),
        }
    }

    pub fn init_kernel_vm(mut self) -> Self {
        // TODO: record kernel code usage
        self.stack = Stack::kstack();
        self
    }

    pub fn init_proc_stack(&mut self, pid: ProcessId) -> VirtAddr {
        // 计算进程的栈空间地址（根据PID偏移）
        let pid_offset = pid.0 as u64 * STACK_MAX_SIZE;
        let stack_max = STACK_MAX - pid_offset;
        let stack_init_bot = stack_max - STACK_DEF_SIZE;
        let stack_init_top = stack_max - 8; // 8字节对齐
        
        // 初始化栈空间
        let mapper = &mut self.page_table.mapper();
        let alloc = &mut *get_frame_alloc_for_sure();
        
        // 映射栈空间并更新栈信息
        let top_page = Page::containing_address(VirtAddr::new(stack_init_top));
        self.stack = Stack::new(top_page, STACK_DEF_PAGE);
        let range = elf::map_range(stack_init_bot, STACK_DEF_PAGE, mapper, alloc).unwrap();
        self.stack.range = range;
        self.stack.usage = STACK_DEF_PAGE;
        
        // 返回栈顶地址
        VirtAddr::new(stack_init_top)
    }

    pub fn handle_page_fault(&mut self, addr: VirtAddr) -> bool {
        let mapper = &mut self.page_table.mapper();
        let alloc = &mut *get_frame_alloc_for_sure();

        self.stack.handle_page_fault(addr, mapper, alloc)
    }

    pub(super) fn memory_usage(&self) -> u64 {
        self.stack.memory_usage()
    }
}

impl core::fmt::Debug for ProcessVm {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        let (size, unit) = humanized_size(self.memory_usage());

        f.debug_struct("ProcessVm")
            .field("stack", &self.stack)
            .field("memory_usage", &format!("{} {}", size, unit))
            .field("page_table", &self.page_table)
            .finish()
    }
}
