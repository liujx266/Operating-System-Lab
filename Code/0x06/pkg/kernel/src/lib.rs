#![no_std]
#![allow(dead_code)]
#![feature(naked_functions)]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]
#![feature(type_alias_impl_trait)]
#![feature(map_try_insert)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::result_unit_err)]

extern crate alloc;
#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate bitflags;
extern crate libm;

#[macro_use]
pub mod utils;
pub use utils::*;

#[macro_use]
pub mod drivers;
pub use drivers::*;

pub mod memory;
pub mod interrupt;
pub mod proc; // 添加进程模块

pub use alloc::format;


use boot::BootInfo;
use uefi::{Status, runtime::ResetType};

pub fn init(boot_info: &'static BootInfo) {
    unsafe {
        // set uefi system table
        uefi::table::set_system_table(boot_info.system_table.cast().as_ptr());
    }

    serial::init(); // init serial output
    logger::init(boot_info.log_level); // 使用从 bootloader 传递的日志级别
    memory::address::init(boot_info);
    memory::gdt::init(); // init gdt
    memory::allocator::init(); // init kernel heap allocator
    interrupt::init(); // init interrupts
    memory::init(boot_info); // init memory manager
    
    proc::init(boot_info); // 初始化进程管理器，在内存初始化之后，启用中断之前

    x86_64::instructions::interrupts::enable();
    info!("Interrupts Enabled.");

    // Test ATA drive
    test_ata_drive();

    // Initialize filesystem
    drivers::filesystem::init();

    info!("YatSenOS initialized.");
}

fn test_ata_drive() {
    info!("Testing ATA drive...");
    match drivers::ata::AtaDrive::open(0, 0) {
        Some(drive) => {
            info!("ATA drive test successful!");

            // Test reading the first sector (MBR)
            info!("Testing MBR read...");
            let mut buffer = [0u8; 512];
            match drive.read_block_raw(0, &mut buffer) {
                Ok(()) => {
                    info!("MBR read successful!");
                    // Check MBR signature
                    if buffer[510] == 0x55 && buffer[511] == 0xAA {
                        info!("Valid MBR signature found!");

                        // Parse first partition entry using our MbrPartition implementation
                        let partition_offset = 446;
                        let partition_data: [u8; 16] = buffer[partition_offset..partition_offset + 16]
                            .try_into()
                            .expect("Failed to extract partition data");

                        // Use our existing MbrPartition implementation
                        let partition = storage::mbr::MbrPartition::parse(&partition_data);

                        info!("First partition info (using MbrPartition):");
                        info!("  Active: {}", partition.is_active());
                        info!("  Type: 0x{:02X}", partition.partition_type());
                        info!("  Start LBA: {}", partition.begin_lba());
                        info!("  Size (sectors): {}", partition.total_lba());
                        info!("  Size (MB): {}", (partition.total_lba() as u64 * 512) / (1024 * 1024));
                        info!("  CHS Begin: C{}/H{}/S{}",
                              partition.begin_cylinder(),
                              partition.begin_head(),
                              partition.begin_sector());
                        info!("  CHS End: C{}/H{}/S{}",
                              partition.end_cylinder(),
                              partition.end_head(),
                              partition.end_sector());
                    } else {
                        warn!("Invalid MBR signature: 0x{:02X}{:02X}", buffer[511], buffer[510]);
                    }
                }
                Err(e) => {
                    warn!("MBR read failed: {}", e);
                }
            }
        }
        None => {
            warn!("ATA drive test failed - no drive found");
        }
    }
}

pub fn wait(init: proc::ProcessId) {
    loop {
        if proc::still_alive(init) {
            // 使用hlt指令让CPU空闲，减少CPU资源占用
            x86_64::instructions::hlt();
        } else {
            break;
        }
    }
}

pub fn shutdown() -> ! {
    info!("YatSenOS shutting down.");
    uefi::runtime::reset(ResetType::SHUTDOWN, Status::SUCCESS, None);
}
