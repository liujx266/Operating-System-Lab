use alloc::{collections::BTreeMap, sync::Arc};
use spin::RwLock;
use super::sync::SemaphoreSet;

use crate::utils::ResourceSet;

use super::*;

#[derive(Debug, Clone)]
pub struct ProcessData {
    // shared data
    pub(super) env: Arc<RwLock<BTreeMap<String, String>>>,
    // 文件描述符表
    pub(super) resources: Arc<RwLock<ResourceSet>>,
    // Memory usage tracking
    pub(super) code_bytes: u64, // Bytes used by code/data segments
    pub(super) stack_pages: u64, // Pages used by stack
    pub(super) total_pages: u64, // Total pages used (code + stack + others if any)
    pub(super) semaphores: Arc<RwLock<SemaphoreSet>>,
}

impl Default for ProcessData {
    fn default() -> Self {
        Self {
            env: Arc::new(RwLock::new(BTreeMap::new())),
            resources: Arc::new(RwLock::new(ResourceSet::default())),
            code_bytes: 0,
            stack_pages: 0,
            total_pages: 0,
            semaphores: Arc::new(RwLock::new(SemaphoreSet::default())),
        }
    }
}

impl ProcessData {
    pub fn new() -> Self {
        Self::default()
    }

    // Updates memory usage based on loaded ELF segments and stack size
    pub(super) fn update_memory_usage(&mut self, code_bytes: u64, stack_pages: u64) {
        self.code_bytes = code_bytes;
        self.stack_pages = stack_pages;
        // Calculate total pages (assuming PAGE_SIZE is 4KiB)
        let code_pages = (code_bytes + crate::memory::PAGE_SIZE - 1) / crate::memory::PAGE_SIZE;
        self.total_pages = code_pages + stack_pages;
    }

    // Returns total memory usage in bytes
    pub fn memory_usage_bytes(&self) -> u64 {
        self.total_pages * crate::memory::PAGE_SIZE
    }

    // Returns total memory usage in pages
    pub fn memory_usage_pages(&self) -> u64 {
        self.total_pages
    }

    pub fn env(&self, key: &str) -> Option<String> {
        self.env.read().get(key).cloned()
    }

    pub fn set_env(&mut self, key: &str, val: &str) {
        self.env.write().insert(key.into(), val.into());
    }
    
    // 添加读取资源的方法
    pub fn read(&self, fd: u8, buf: &mut [u8]) -> isize {
        self.resources.read().read(fd, buf)
    }

    // 添加写入资源的方法
    pub fn write(&self, fd: u8, buf: &[u8]) -> isize {
        self.resources.read().write(fd, buf)
    }

    // 添加打开文件的方法
    pub fn open_resource(&self, resource: crate::utils::Resource) -> u8 {
        self.resources.write().open(resource)
    }

    // 添加关闭文件的方法
    pub fn close_resource(&self, fd: u8) -> bool {
        self.resources.write().close(fd)
    }
}
