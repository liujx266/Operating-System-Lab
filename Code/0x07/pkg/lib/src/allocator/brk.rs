use linked_list_allocator::LockedHeap;
use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicUsize, Ordering};

use crate::*;

const INITIAL_HEAP_SIZE: usize = 8 * 1024; // 8 KiB 初始大小
const MAX_HEAP_SIZE: usize = 8 * 1024 * 1024; // 8 MiB 最大大小
const EXTEND_SIZE: usize = 4 * 1024; // 每次扩容 4 KiB

#[global_allocator]
static ALLOCATOR: BrkAllocator = BrkAllocator::empty();

struct BrkAllocator {
    allocator: LockedHeap,
    current_size: AtomicUsize,
}

pub fn init() {
    ALLOCATOR.init();
}

impl BrkAllocator {
    pub const fn empty() -> Self {
        Self {
            allocator: LockedHeap::empty(),
            current_size: AtomicUsize::new(0),
        }
    }

    pub fn init(&self) {
        // 获取当前堆的起始地址
        let heap_start = sys_brk(None).unwrap();
        
        // 设置初始堆大小
        let initial_end = heap_start + INITIAL_HEAP_SIZE;
        let ret = sys_brk(Some(initial_end)).expect("Failed to allocate initial heap");
        
        assert!(ret == initial_end, "Failed to allocate initial heap");
        
        // 初始化 LockedHeap
        unsafe { 
            self.allocator.lock().init(heap_start as *mut u8, INITIAL_HEAP_SIZE);
        }
        
        // 记录当前堆大小
        self.current_size.store(INITIAL_HEAP_SIZE, Ordering::SeqCst);
    }

    pub unsafe fn extend(&self) -> bool {
        let current_size = self.current_size.load(Ordering::SeqCst);
        
        // 检查是否已达到最大大小
        if current_size >= MAX_HEAP_SIZE {
            return false;
        }
        
        // 计算新的堆大小，确保不超过最大限制
        let new_size = core::cmp::min(current_size + EXTEND_SIZE, MAX_HEAP_SIZE);
        
        if new_size == current_size {
            return false;
        }
        
        // 获取当前堆的起始地址
        let heap_start = match sys_brk(None) {
            Some(start) => start,
            None => return false,
        };
        
        // 计算新的堆结束地址
        let new_end = heap_start + new_size;
        
        // 尝试扩展堆
        match sys_brk(Some(new_end)) {
            Some(actual_end) if actual_end == new_end => {
                // 扩展成功，重新初始化分配器以包含新的堆大小
                unsafe {
                    self.allocator.lock().init(heap_start as *mut u8, new_size);
                }
                
                // 更新当前堆大小
                self.current_size.store(new_size, Ordering::SeqCst);
                true
            }
            _ => false,
        }
    }
}

unsafe impl GlobalAlloc for BrkAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // 首先尝试分配
        let mut ptr = unsafe { self.allocator.alloc(layout) };
        
        // 如果分配失败，尝试扩展堆然后再次分配
        if ptr.is_null() {
            if unsafe { self.extend() } {
                ptr = unsafe { self.allocator.alloc(layout) };
            }
        }
        
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { self.allocator.dealloc(ptr, layout) }
    }
}

#[cfg(not(test))]
#[alloc_error_handler]
fn alloc_error_handler(layout: alloc::alloc::Layout) -> ! {
    panic!("Allocation error: {:?}", layout)
}
