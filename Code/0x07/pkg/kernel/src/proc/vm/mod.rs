use alloc::{format, vec::Vec};
use x86_64::{
    structures::paging::{
        mapper::{CleanUp, UnmapError},
        page::*,
        *,
    },
    VirtAddr,
};
use xmas_elf::ElfFile;
use crate::{humanized_size, memory::*};
use log::{debug, error};

pub mod heap;
pub mod stack;

use self::{heap::Heap, stack::Stack};

use super::PageTableContext;

// See the documentation for the `KernelPages` type
// Ignore when you not reach this part
//
use boot::KernelPages;

type MapperRef<'a> = &'a mut OffsetPageTable<'static>;
type FrameAllocatorRef<'a> = &'a mut BootInfoFrameAllocator;

impl elf::FrameDeallocator<Size4KiB> for BootInfoFrameAllocator {
    fn deallocate_frame(&mut self, frame: PhysFrame) {
        unsafe { FrameDeallocator::deallocate_frame(self, frame) }
    }
}

pub struct ProcessVm {
    // page table is shared by parent and child
    pub(super) page_table: PageTableContext,

    // stack is pre-process allocated
    pub(super) stack: Stack,

    // heap is allocated by brk syscall
    pub(super) heap: Heap,

    // code is hold by the first process
    // these fields will be empty for other processes
    pub(super) code: Vec<PageRangeInclusive>,
    pub(super) code_usage: u64,
    pub(super) is_kernel: bool,
}

impl ProcessVm {
    pub fn new(page_table: PageTableContext, is_kernel: bool) -> Self {
        Self {
            page_table,
            stack: Stack::empty(),
            heap: Heap::empty(),
            code: Vec::new(),
            code_usage: 0,
            is_kernel,
        }
    }


    // See the documentation for the `KernelPages` type
    // Ignore when you not reach this part

    /// Initialize kernel vm
    ///
    /// NOTE: this function should only be called by the first process
    pub fn init_kernel_vm(mut self, pages: &KernelPages) -> Self {
        // FIXME: record kernel code usage
        self.code = pages.iter().cloned().collect();
        self.code_usage = pages
            .iter()
            .map(|range| range.count() as u64 * Page::<Size4KiB>::SIZE)
            .sum();

        self.stack = Stack::kstack();

        // ignore heap for kernel process as we don't manage it

        self
    }

    pub fn brk(&self, addr: Option<VirtAddr>) -> Option<VirtAddr> {
        self.heap.brk(
            addr,
            &mut self.page_table.mapper(),
            &mut get_frame_alloc_for_sure(),
        )
    }

    pub fn load_elf(&mut self, elf: &ElfFile) {
        let mapper = &mut self.page_table.mapper();

        let alloc = &mut *get_frame_alloc_for_sure();

        self.load_elf_code(elf, mapper, alloc);
        self.stack.init(mapper, alloc);
    }

    fn load_elf_code(&mut self, elf: &ElfFile, mapper: MapperRef, alloc: FrameAllocatorRef) {
        // FIXME: make the `load_elf` function return the code pages
        self.code =
            elf::load_elf(elf, *PHYSICAL_OFFSET.get().unwrap(), mapper, alloc, true).unwrap();

        // FIXME: calculate code usage
        self.code_usage = self
            .code
            .iter()
            .map(|range| range.count() as u64 * Page::<Size4KiB>::SIZE)
            .sum();
    }

    pub fn fork(&self, stack_offset_count: u64) -> Self {
        let owned_page_table = self.page_table.fork();
        let mapper = &mut owned_page_table.mapper();

        let alloc = &mut *get_frame_alloc_for_sure();

        Self {
            page_table: owned_page_table,
            stack: self.stack.fork(mapper, alloc, stack_offset_count),
            heap: self.heap.fork(),

            // do not share code info
            code: Vec::new(),
            code_usage: 0,
            is_kernel: self.is_kernel,
        }
    }

    pub fn handle_page_fault(&mut self, addr: VirtAddr) -> bool {
        let mapper = &mut self.page_table.mapper();
        let alloc = &mut *get_frame_alloc_for_sure();

        self.stack.handle_page_fault(addr, mapper, alloc)
    }

    pub(super) fn memory_usage(&self) -> u64 {
        self.stack.memory_usage() + self.heap.memory_usage() + self.code_usage
    }

    pub(super) fn clean_up(&mut self) -> Result<(), UnmapError> {
        debug!("ProcessVm::clean_up called, page table using_count: {}", self.page_table.using_count());

        let mapper = &mut self.page_table.mapper();
        let dealloc = &mut *get_frame_alloc_for_sure();

        // statistics for logging and debugging
        // NOTE: you may need to implement `frames_recycled` by yourself
        let start_count = dealloc.frames_recycled();
        debug!("Starting cleanup with {} recycled frames", start_count);

        // 1. 释放栈区：调用 Stack 的 clean_up 函数
        self.stack.clean_up(mapper, dealloc)?;

        // 2. 如果当前页表被引用次数为 1，则进行共享内存的释放，否则跳过至第 7 步
        if self.page_table.using_count() == 1 {
            // 3. 释放堆区：调用 Heap 的 clean_up 函数
            self.heap.clean_up(mapper, dealloc)?;

            // 4. 释放 ELF 文件映射的内存区域：根据记录的 code 页面范围数组，依次调用 elf::unmap_range 函数
            for page_range in self.code.iter() {
                elf::unmap_range(*page_range, mapper, dealloc, true)?;
            }

            // 5. 清理页表：调用 mapper 的 clean_up 函数，这将清空全部无页面映射的一至三级页表
            // 6. 清理四级页表：直接回收 PageTableContext 的 reg.addr 所指向的页面
            unsafe {
                // free P1-P3
                mapper.clean_up(dealloc);

                // free P4
                dealloc.deallocate_frame(self.page_table.reg.addr);
            }
        }

        // 7. 统计内存回收情况，并打印调试信息
        let end_count = dealloc.frames_recycled();

        debug!(
            "Recycled {}({:.3} MiB) frames, {}({:.3} MiB) frames in total.",
            end_count - start_count,
            ((end_count - start_count) * 4) as f32 / 1024.0,
            end_count,
            (end_count * 4) as f32 / 1024.0
        );

        Ok(())
    }
}

impl Drop for ProcessVm {
    fn drop(&mut self) {
        debug!("ProcessVm::drop called, page table using_count: {}", self.page_table.using_count());
        if let Err(err) = self.clean_up() {
            error!("Failed to clean up process memory: {:?}", err);
        } else {
            debug!("ProcessVm::drop completed successfully");
        }
    }
}

impl core::fmt::Debug for ProcessVm {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        let (size, unit) = humanized_size(self.memory_usage());

        f.debug_struct("ProcessVm")
            .field("is_kernel", &self.is_kernel)
            .field("stack", &self.stack)
            .field("heap", &self.heap)
            .field("memory_usage", &format!("{} {}", size, unit))
            .field("page_table", &self.page_table)
            .finish()
    }
}