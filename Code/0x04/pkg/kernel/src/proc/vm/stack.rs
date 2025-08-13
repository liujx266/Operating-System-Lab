use x86_64::{
    structures::paging::{
        mapper::MapToError,
        page::*,
        Page,
        PageTableFlags,
        FrameAllocator,  // 添加缺失的 trait 导入
        Mapper,          // 添加缺失的 trait 导入
    },
    VirtAddr,
};

extern crate ysos_elf as elf;

use super::{FrameAllocatorRef, MapperRef};

// 0xffff_ff00_0000_0000 is the kernel's address space
pub const STACK_MAX: u64 = 0x4000_0000_0000;
pub const STACK_MAX_PAGES: u64 = 0x100000;
pub const STACK_MAX_SIZE: u64 = STACK_MAX_PAGES * crate::memory::PAGE_SIZE;
pub const STACK_START_MASK: u64 = !(STACK_MAX_SIZE - 1);
// [bot..0x2000_0000_0000..top..0x3fff_ffff_ffff]
// init stack
pub const STACK_DEF_BOT: u64 = STACK_MAX - STACK_MAX_SIZE;
pub const STACK_DEF_PAGE: u64 = 1;
pub const STACK_DEF_SIZE: u64 = STACK_DEF_PAGE * crate::memory::PAGE_SIZE;

pub const STACK_INIT_BOT: u64 = STACK_MAX - STACK_DEF_SIZE;
pub const STACK_INIT_TOP: u64 = STACK_MAX - 8;

const STACK_INIT_TOP_PAGE: Page<Size4KiB> = Page::containing_address(VirtAddr::new(STACK_INIT_TOP));

// [bot..0xffffff0100000000..top..0xffffff01ffffffff]
// kernel stack
pub const KSTACK_MAX: u64 = 0xffff_ff02_0000_0000;
pub const KSTACK_DEF_BOT: u64 = KSTACK_MAX - STACK_MAX_SIZE;
pub const KSTACK_DEF_PAGE: u64 = 512;  // 设置为512页（2MB）
pub const KSTACK_DEF_SIZE: u64 = KSTACK_DEF_PAGE * crate::memory::PAGE_SIZE;

pub const KSTACK_INIT_BOT: u64 = KSTACK_MAX - KSTACK_DEF_SIZE;
pub const KSTACK_INIT_TOP: u64 = KSTACK_MAX - 8;

const KSTACK_INIT_PAGE: Page<Size4KiB> = Page::containing_address(VirtAddr::new(KSTACK_INIT_BOT));
const KSTACK_INIT_TOP_PAGE: Page<Size4KiB> =
    Page::containing_address(VirtAddr::new(KSTACK_INIT_TOP));

pub struct Stack {
    pub(super) range: PageRange<Size4KiB>,
    pub(super) usage: u64,
}

impl Stack {
    pub fn new(top: Page, size: u64) -> Self {
        Self {
            range: Page::range(top - size + 1, top + 1),
            usage: size,
        }
    }

    pub const fn empty() -> Self {
        Self {
            range: Page::range(STACK_INIT_TOP_PAGE, STACK_INIT_TOP_PAGE),
            usage: 0,
        }
    }

    pub const fn kstack() -> Self {
        Self {
            range: Page::range(KSTACK_INIT_PAGE, KSTACK_INIT_TOP_PAGE),
            usage: KSTACK_DEF_PAGE,
        }
    }

    // 提取共用的映射逻辑为私有辅助函数
    fn map_pages(
        range_start: Page<Size4KiB>,
        page_count: u64,
        mapper: MapperRef,
        alloc: FrameAllocatorRef,
    ) -> Result<PageRange<Size4KiB>, MapToError<Size4KiB>> {
        // 设置页面标志：存在、可写、用户可访问
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;
        
        // 计算结束页面
        let range_end = range_start + page_count;
        
        // 映射每个页面
        for page in Page::range(range_start, range_end) {
            let frame = alloc.allocate_frame()
                .ok_or(MapToError::FrameAllocationFailed)?;
            unsafe {
                mapper.map_to(page, frame, flags, alloc)?
                    .flush();
            }
        }
        
        // 返回映射的页面范围
        Ok(Page::range(range_start, range_end))
    }

    pub fn init(&mut self, mapper: MapperRef, alloc: FrameAllocatorRef) {
        debug_assert!(self.usage == 0, "Stack is not empty.");
        
        // 计算需要映射的页面范围
        let range_start = Page::containing_address(VirtAddr::new(STACK_INIT_BOT));
        
        // 使用辅助函数映射页面
        self.range = Self::map_pages(range_start, STACK_DEF_PAGE, mapper, alloc).unwrap();
        self.usage = STACK_DEF_PAGE;
    }

    pub fn handle_page_fault(
        &mut self,
        addr: VirtAddr,
        mapper: MapperRef,
        alloc: FrameAllocatorRef,
    ) -> bool {
        if !self.is_on_stack(addr) {
            return false;
        }

        if let Err(m) = self.grow_stack(addr, mapper, alloc) {
            error!("Grow stack failed: {:?}", m);
            return false;
        }

        true
    }

    fn is_on_stack(&self, addr: VirtAddr) -> bool {
        let addr = addr.as_u64();
        let cur_stack_bot = self.range.start.start_address().as_u64();
        trace!("Current stack bot: {:#x}", cur_stack_bot);
        trace!("Address to access: {:#x}", addr);
        addr & STACK_START_MASK == cur_stack_bot & STACK_START_MASK
    }

    fn grow_stack(
        &mut self,
        addr: VirtAddr,
        mapper: MapperRef,
        alloc: FrameAllocatorRef,
    ) -> Result<(), MapToError<Size4KiB>> {
        debug_assert!(self.is_on_stack(addr), "Address is not on stack.");

        // 获取需要访问的页面
        let page = Page::containing_address(addr);
        
        // 计算需要新增的页面数量（每次增加32页，约128KB）
        let growth_pages = 32u64;
        
        // 确保不超过栈的最大页面数
        if self.usage + growth_pages > STACK_MAX_PAGES {
            return Err(MapToError::FrameAllocationFailed);
        }
        
        // 计算新的栈底页面
        let new_start_page = if page < self.range.start {
            // 如果缺页的地址在当前栈底以下，则以该页为新栈底
            page
        } else {
            // 否则保持当前栈底不变
            self.range.start
        };
        
        // 计算需要映射的页面范围
        let pages_to_map_count = self.range.start - new_start_page;
        if pages_to_map_count == 0 {
            // 如果不需要映射新页面，则返回成功
            return Ok(());
        }
        
        // 计算映射的起始地址
        let map_addr = new_start_page.start_address().as_u64();
        
        // 映射新的页面
        trace!(
            "Grow stack: map {:#x} with {} pages", 
            map_addr, 
            pages_to_map_count
        );

        // 使用辅助函数映射页面
        let new_range = Self::map_pages(new_start_page, pages_to_map_count, mapper, alloc)?;
        
        // 更新栈的范围和使用量
        self.range = PageRange {
            start: new_range.start,
            end: self.range.end,
        };
        self.usage += pages_to_map_count;
        
        Ok(())
    }

    pub fn memory_usage(&self) -> u64 {
        self.usage * crate::memory::PAGE_SIZE
    }
}

impl core::fmt::Debug for Stack {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        f.debug_struct("Stack")
            .field(
                "top",
                &format_args!("{:#x}", self.range.end.start_address().as_u64()),
            )
            .field(
                "bot",
                &format_args!("{:#x}", self.range.start.start_address().as_u64()),
            )
            .finish()
    }
}
