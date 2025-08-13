use x86_64::{
    structures::paging::{
        mapper::{MapToError, UnmapError},
        page::*,
        Page,
        PageTableFlags,
        FrameAllocator,
        FrameDeallocator,
        Mapper,          // 添加缺失的 trait 导入
    },
    VirtAddr,
};



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
pub const KSTACK_DEF_PAGE: u64 = 8;
pub const KSTACK_DEF_SIZE: u64 = KSTACK_DEF_PAGE * crate::memory::PAGE_SIZE;

pub const KSTACK_INIT_BOT: u64 = KSTACK_MAX - KSTACK_DEF_SIZE;
pub const KSTACK_INIT_TOP: u64 = KSTACK_MAX - 8;

const KSTACK_INIT_PAGE: Page<Size4KiB> = Page::containing_address(VirtAddr::new(KSTACK_INIT_BOT));
const KSTACK_INIT_TOP_PAGE: Page<Size4KiB> =
    Page::containing_address(VirtAddr::new(KSTACK_INIT_TOP));

pub struct Stack {
    pub(super) range: PageRange<Size4KiB>,
    pub(super) usage: u64,
    is_kernel: bool,
}

impl Stack {
    pub fn new(top: Page, size: u64, is_kernel: bool) -> Self {
        Self {
            range: Page::range(top - size + 1, top + 1),
            usage: size,
            is_kernel,
        }
    }

    pub const fn empty() -> Self {
        Self {
            range: Page::range(STACK_INIT_TOP_PAGE, STACK_INIT_TOP_PAGE),
            usage: 0,
            is_kernel: false,
        }
    }

    pub const fn kstack() -> Self {
        Self {
            range: Page::range(KSTACK_INIT_PAGE, KSTACK_INIT_TOP_PAGE),
            usage: KSTACK_DEF_PAGE,
            is_kernel: true,
        }
    }

    pub fn start_address(&self) -> VirtAddr {
        self.range.start.start_address()
    }

    // 提取共用的映射逻辑为私有辅助函数
    fn map_pages(
        &self,
        range_start: Page<Size4KiB>,
        page_count: u64,
        mapper: MapperRef,
        alloc: FrameAllocatorRef,
    ) -> Result<PageRange<Size4KiB>, MapToError<Size4KiB>> {
        let mut flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        if !self.is_kernel {
            flags |= PageTableFlags::USER_ACCESSIBLE;
        }

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
        self.range = self
            .map_pages(range_start, STACK_DEF_PAGE, mapper, alloc)
            .unwrap();
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
        let new_range = self.map_pages(new_start_page, pages_to_map_count, mapper, alloc)?;
        
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

    /// Clone a range of memory
    ///
    /// - `src_addr`: the address of the source memory
    /// - `dest_addr`: the address of the target memory
    /// - `size`: the count of pages to be cloned
    fn clone_range(cur_addr: u64, dest_addr: u64, size: u64) {
        trace!("Clone range: {:#x} -> {:#x}", cur_addr, dest_addr);
        unsafe {
            core::ptr::copy_nonoverlapping::<u64>(
                cur_addr as *mut u64,
                dest_addr as *mut u64,
                (size * Size4KiB::SIZE / 8) as usize,
            );
        }
    }

    pub fn fork(
        &self,
        mapper: MapperRef,
        alloc: FrameAllocatorRef,
        stack_offset_count: u64, // Number of existing children, used to offset the new stack
    ) -> Self {
        // 1. Calculate new stack range for the child 
        // Start with a base offset and try to find a free stack range
        let mut offset = stack_offset_count + 1; // Add 1 to ensure we don't overlap with parent
        let mut new_stack_range = None;
        let mut attempts = 0;

        while attempts < 10 { // Limit attempts to prevent infinite loop
            let current_addr = STACK_MAX - (offset * STACK_MAX_SIZE);
            
            if current_addr < STACK_MAX_SIZE { // Arbitrary lower bound to prevent issues
                panic!("Out of stack space for new process");
            }

            let top_page = Page::<Size4KiB>::containing_address(VirtAddr::new(current_addr));
            let start_page = top_page - self.usage + 1;
            let try_range = Page::range(start_page, top_page + 1);
            
            let mut range_is_free = true;
            // Check if any page in range is already mapped
            for page in try_range.clone() {
                if mapper.translate_page(page).is_ok() {
                    range_is_free = false;
                    break;
                }
            }
            
            if range_is_free {
                new_stack_range = Some(try_range);
                break;
            }
            
            offset += 1;
            attempts += 1;
        }

        let new_stack_range = new_stack_range.expect("Failed to find free stack space after 10 attempts");

        // 2. Allocate and map new stack for child
        
        // Map the free range we found
        let mut flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        if !self.is_kernel {
            flags |= PageTableFlags::USER_ACCESSIBLE;
        }
        for page in new_stack_range.clone() {
            let frame = alloc
                .allocate_frame()
                .ok_or(MapToError::<Size4KiB>::FrameAllocationFailed)
                .expect("Stack fork: Frame allocation failed for child stack");
            unsafe {
                mapper
                    .map_to(page, frame, flags, alloc)
                    .expect("Stack fork: Failed to map child stack page")
                    .flush();
            }
        }

        // 3. Copy the *entire stack* from parent to child
        let parent_stack_bottom_addr = self.range.start.start_address().as_u64();
        let child_stack_bottom_addr = new_stack_range.start.start_address().as_u64();
        
        Self::clone_range(parent_stack_bottom_addr, child_stack_bottom_addr, self.usage);

        // 4. Return the new stack
        Self {
            range: new_stack_range,
            usage: self.usage, // Child stack initially has the same usage as parent
            is_kernel: self.is_kernel,
        }
    }
    pub fn clean_up(
        &mut self,
        mapper: MapperRef,
        dealloc: FrameAllocatorRef,
    ) -> Result<(), UnmapError> {
        if self.usage == 0 {
            warn!("Stack is empty, no need to clean up.");
            return Ok(());
        }

        for page in self.range.clone() {
            let (frame, flusher) = mapper.unmap(page)?;
            unsafe {
                dealloc.deallocate_frame(frame);
            }
            flusher.flush();
        }

        self.usage = 0;
        self.range = Page::range(STACK_INIT_TOP_PAGE, STACK_INIT_TOP_PAGE);

        Ok(())
    }
}

impl core::fmt::Debug for Stack {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        f.debug_struct("Stack")
            .field("is_kernel", &self.is_kernel)
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
