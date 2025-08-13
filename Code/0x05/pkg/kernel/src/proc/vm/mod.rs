use alloc::format;
use x86_64::{
    structures::paging::{page::*, mapper::*, *},
    VirtAddr,
};

use crate::{humanized_size, memory::*};
// 从正确的路径导入elf，elf是一个单独的crate
extern crate ysos_elf as elf;
use xmas_elf::ElfFile;

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
    // Note: Memory usage tracking moved to ProcessData
}

impl ProcessVm {
    pub fn new(page_table: PageTableContext) -> Self {
        Self {
            page_table,
            stack: Stack::empty(),
            // code_memory_usage: 0, // Removed
        }
    }

    pub fn init_kernel_vm(mut self) -> Self {
        // TODO: record kernel code usage in ProcessData for kernel process
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
        
        // 不再使用elf::map_range，改为使用Stack::map_pages方法
        // 这样可以确保设置了USER_ACCESSIBLE标志
        let range_start = Page::containing_address(VirtAddr::new(stack_init_bot));
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;
        
        // 映射页面
        for page in Page::range(range_start, range_start + STACK_DEF_PAGE) {
            let frame = alloc.allocate_frame()
                .expect("Failed to allocate frame for stack");
            unsafe {
                mapper.map_to(page, frame, flags, alloc)
                    .expect("Failed to map stack page")
                    .flush();
            }
        }
        
        self.stack.range = Page::range(range_start, range_start + STACK_DEF_PAGE);
        self.stack.usage = STACK_DEF_PAGE;
        
        // 返回栈顶地址
        VirtAddr::new(stack_init_top)
    }
    
    // Loads ELF, returns (code_bytes, code_pages) on success
    pub fn load_elf(&mut self, elf: &ElfFile) -> Result<(u64, u64), MapToError<Size4KiB>> {
        let mapper = &mut self.page_table.mapper();
        let alloc = &mut *get_frame_alloc_for_sure();

        // 初始化栈
        self.stack.init(mapper, alloc);

        // 计数加载的段数量和页数
        let mut loaded_segments = 0;
        let mut code_page_count = 0; // Renamed for clarity
        let mut total_code_bytes = 0; // Track total bytes for segments

        // 遍历ELF文件的程序段
        for segment in elf.program_iter() {
            // 只处理可加载的段
            if segment.get_type().unwrap() != xmas_elf::program::Type::Load {
                continue;
            }

            // 获取段信息
            let virt_addr = segment.virtual_addr() as u64;
            let mem_size = segment.mem_size() as u64;
            let file_size = segment.file_size() as u64;
            
            // 计算需要映射的页数
            let page_start = Page::<Size4KiB>::containing_address(VirtAddr::new(virt_addr));
            let page_end = Page::<Size4KiB>::containing_address(VirtAddr::new(virt_addr + mem_size - 1));
            let page_range = Page::range_inclusive(page_start, page_end);
            
            // 创建适当的页表标志
            let mut flags = PageTableFlags::PRESENT;
            
            // 根据段的权限设置页表标志
            if segment.flags().is_write() {
                flags |= PageTableFlags::WRITABLE;
            }
            
            // 用户程序需要USER_ACCESSIBLE标志
            flags |= PageTableFlags::USER_ACCESSIBLE;
            
            // 遍历页范围，分配帧并映射到页表
            for page in page_range {
                let frame = alloc.allocate_frame()
                    .ok_or(MapToError::FrameAllocationFailed)?;
                
                unsafe {
                    // 映射页面
                    mapper.map_to(page, frame, flags, alloc)?
                        .flush();
                    
                    // 如果这个页在文件段内，复制数据
                    let dest = (frame.start_address().as_u64() + *crate::memory::PHYSICAL_OFFSET.get().expect("PHYSICAL_OFFSET should be initialized")) as *mut u8;
                    let src_offset = if page == page_start {
                        0
                    } else {
                        page.start_address().as_u64() - virt_addr
                    };
                    
                    // 计算在这个页内需要复制的字节数
                    let copy_size = if (page.start_address().as_u64() + PAGE_SIZE - virt_addr) < file_size {
                        PAGE_SIZE as usize
                    } else {
                        (file_size - src_offset) as usize
                    };
                    
                    // 如果有数据要复制
                    if src_offset < file_size {
                        let src = (elf.input.as_ptr() as usize + segment.offset() as usize + src_offset as usize) as *const u8;
                        core::ptr::copy_nonoverlapping(src, dest, copy_size);
                        
                        // 如果页内还有bss部分，清零
                        if copy_size < PAGE_SIZE as usize {
                            core::ptr::write_bytes(dest.add(copy_size), 0, PAGE_SIZE as usize - copy_size);
                        }
                    } else {
                        // 这个页完全是bss部分，清零整个页
                        core::ptr::write_bytes(dest, 0, PAGE_SIZE as usize);
                    }
                }
                
                code_page_count += 1;
            }
            total_code_bytes += mem_size; // Add segment size to total bytes
            loaded_segments += 1;
        }

        // Don't store usage here, return it
        // self.code_memory_usage = code_page_count * PAGE_SIZE; // Removed
        trace!(
            "ELF Loader: {} segments, {} code pages, {} code bytes",
            loaded_segments, code_page_count, total_code_bytes
        );

        Ok((total_code_bytes, code_page_count)) // Return calculated values
    }

    pub fn handle_page_fault(&mut self, addr: VirtAddr) -> bool {
        let mapper = &mut self.page_table.mapper();
        let alloc = &mut *get_frame_alloc_for_sure();

        self.stack.handle_page_fault(addr, mapper, alloc)
    }

    // Removed memory_usage method, now handled by ProcessData
    // pub(super) fn memory_usage(&self) -> u64 { ... }

    // Returns the number of pages currently used by the stack
    pub(super) fn stack_usage_pages(&self) -> u64 {
        self.stack.usage // Assuming 'usage' field in Stack holds page count
    }

    pub fn fork(&self, stack_offset_count: u64) -> Self {
        // Clone the page table context (shares the page table via Arc)
        let owned_page_table = self.page_table.fork();

        let mapper = &mut owned_page_table.mapper();
        let alloc = &mut *get_frame_alloc_for_sure();

        // Fork the stack (allocates new stack and copies content)
        let child_stack = self.stack.fork(mapper, alloc, stack_offset_count);

        Self {
            page_table: owned_page_table,
            stack: child_stack,
        }
    }

    pub fn stack_start_address(&self) -> VirtAddr {
        self.stack.start_address()
    }
}

impl core::fmt::Debug for ProcessVm {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        // Removed memory_usage display from here
        f.debug_struct("ProcessVm")
            .field("stack", &self.stack)
            // .field("memory_usage", &format!("{} {}", size, unit)) // Removed
            .field("page_table", &self.page_table)
            .finish()
    }
}
