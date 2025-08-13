use core::sync::atomic::{AtomicU64, Ordering};

use alloc::sync::Arc;
use x86_64::{
    structures::paging::{mapper::UnmapError, FrameDeallocator, FrameAllocator, Mapper, Page},
    VirtAddr,
};

use super::{FrameAllocatorRef, MapperRef};

// user process runtime heap
// 0x100000000 bytes -> 4GiB
// from 0x0000_2000_0000_0000 to 0x0000_2000_ffff_fff8
pub const HEAP_START: u64 = 0x2000_0000_0000;
pub const HEAP_PAGES: u64 = 0x100000;
pub const HEAP_SIZE: u64 = HEAP_PAGES * crate::memory::PAGE_SIZE;
pub const HEAP_END: u64 = HEAP_START + HEAP_SIZE - 8;

/// User process runtime heap
///
/// always page aligned, the range is [base, end)
pub struct Heap {
    /// the base address of the heap
    ///
    /// immutable after initialization
    base: VirtAddr,

    /// the current end address of the heap
    ///
    /// use atomic to allow multiple threads to access the heap
    end: Arc<AtomicU64>,
}

impl Heap {
    pub fn empty() -> Self {
        Self {
            base: VirtAddr::new(HEAP_START),
            end: Arc::new(AtomicU64::new(HEAP_START)),
        }
    }

    pub fn fork(&self) -> Self {
        Self {
            base: self.base,
            end: self.end.clone(),
        }
    }

    pub fn brk(
        &self,
        new_end: Option<VirtAddr>,
        mapper: MapperRef,
        alloc: FrameAllocatorRef,
    ) -> Option<VirtAddr> {
        use x86_64::structures::paging::{PageTableFlags, Page, Size4KiB};
        use core::sync::atomic::Ordering;
        
        // 如果参数为 None，返回当前的堆区结束地址
        if new_end.is_none() {
            return Some(VirtAddr::new(self.end.load(Ordering::SeqCst)));
        }
        
        let target_addr = new_end.unwrap();
        
        // 检查目标地址是否合法，即是否在 [HEAP_START, HEAP_END] 区间内
        if target_addr.as_u64() < HEAP_START || target_addr.as_u64() > HEAP_END {
            return None;
        }
        
        let current_end = self.end.load(Ordering::SeqCst);
        let target_end = target_addr.as_u64();
        
        // 将目标地址向上对齐到页边界
        let target_end_aligned = (target_end + crate::memory::PAGE_SIZE - 1) & !(crate::memory::PAGE_SIZE - 1);
        
        // 计算当前结束地址和目标地址的差异
        let current_end_aligned = (current_end + crate::memory::PAGE_SIZE - 1) & !(crate::memory::PAGE_SIZE - 1);
        
        // 打印堆区差异用于调试
        log::debug!("brk: current_end={:#x}, target_end={:#x}, current_aligned={:#x}, target_aligned={:#x}",
                   current_end, target_end, current_end_aligned, target_end_aligned);
        
        // 设置页面标志：存在、可写、用户可访问
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;
        
        if target_end == self.base.as_u64() {
            // 用户希望释放整个堆区：目标地址为 base，释放所有页面，end 重置为 base
            if current_end > self.base.as_u64() {
                let start_page = Page::containing_address(self.base);
                let end_page = Page::containing_address(VirtAddr::new(current_end_aligned - 1));
                
                for page in Page::range_inclusive(start_page, end_page) {
                    if let Ok((frame, flusher)) = mapper.unmap(page) {
                        unsafe {
                            alloc.deallocate_frame(frame);
                        }
                        flusher.flush();
                    }
                }
            }
            
            // 重置 end 为 base
            self.end.store(self.base.as_u64(), Ordering::SeqCst);
            return Some(self.base);
            
        } else if target_end_aligned < current_end_aligned {
            // 用户希望缩小堆区：目标地址比当前 end 小，释放多余的页面
            let start_page = Page::containing_address(VirtAddr::new(target_end_aligned));
            let end_page = Page::containing_address(VirtAddr::new(current_end_aligned - 1));
            
            for page in Page::range_inclusive(start_page, end_page) {
                if let Ok((frame, flusher)) = mapper.unmap(page) {
                    unsafe {
                        alloc.deallocate_frame(frame);
                    }
                    flusher.flush();
                }
            }
            
        } else if target_end_aligned > current_end_aligned {
            // 用户希望扩大堆区：目标地址比当前 end 大，分配新的页面
            let start_page = Page::containing_address(VirtAddr::new(current_end_aligned));
            let end_page = Page::containing_address(VirtAddr::new(target_end_aligned - 1));
            
            for page in Page::range_inclusive(start_page, end_page) {
                let frame = match alloc.allocate_frame() {
                    Some(frame) => frame,
                    None => return None, // 分配失败
                };
                
                unsafe {
                    match mapper.map_to(page, frame, flags, alloc) {
                        Ok(flusher) => flusher.flush(),
                        Err(_) => {
                            // 映射失败，释放已分配的帧
                            alloc.deallocate_frame(frame);
                            return None;
                        }
                    }
                }
            }
        }
        
        // 更新 end 地址
        self.end.store(target_end, Ordering::SeqCst);
        Some(VirtAddr::new(target_end))
    }

    pub(super) fn clean_up(
        &self,
        mapper: MapperRef,
        dealloc: FrameAllocatorRef,
    ) -> Result<(), UnmapError> {
        if self.memory_usage() == 0 {
            return Ok(());
        }

        let end = self.end.swap(self.base.as_u64(), Ordering::SeqCst);
        let start_page = Page::containing_address(self.base);
        let end_page = Page::containing_address(VirtAddr::new(end - 1));

        for page in Page::range_inclusive(start_page, end_page) {
            if let Ok((frame, flusher)) = mapper.unmap(page) {
                unsafe {
                    dealloc.deallocate_frame(frame);
                }
                flusher.flush();
            }
        }

        Ok(())
    }

    pub fn memory_usage(&self) -> u64 {
        self.end.load(Ordering::Relaxed) - self.base.as_u64()
    }
}

impl core::fmt::Debug for Heap {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Heap")
            .field("base", &format_args!("{:#x}", self.base.as_u64()))
            .field(
                "end",
                &format_args!("{:#x}", self.end.load(Ordering::Relaxed)),
            )
            .finish()
    }
}