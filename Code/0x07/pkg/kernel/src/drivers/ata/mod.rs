//! ATA Drive
//!
//! reference: https://wiki.osdev.org/IDE
//! reference: https://wiki.osdev.org/ATA_PIO_Mode
//! reference: https://github.com/theseus-os/Theseus/blob/HEAD/kernel/ata/src/lib.rs

mod bus;
mod consts;

use alloc::boxed::Box;
use bus::AtaBus;
use consts::AtaDeviceType;
use spin::Mutex;

lazy_static! {
    pub static ref BUSES: [Mutex<AtaBus>; 2] = {
        let buses = [
            Mutex::new(AtaBus::new(0, 14, 0x1F0, 0x3F6)),
            Mutex::new(AtaBus::new(1, 15, 0x170, 0x376)),
        ];

        info!("Initialized ATA Buses.");

        buses
    };
}

#[derive(Clone)]
pub struct AtaDrive {
    pub bus: u8,
    pub drive: u8,
    blocks: u32,
    model: Box<str>,
    serial: Box<str>,
}

impl AtaDrive {
    pub fn open(bus: u8, drive: u8) -> Option<Self> {
        trace!("Opening drive {}@{}...", bus, drive);

        // we only support PATA drives
        if let Ok(AtaDeviceType::Pata(res)) = BUSES[bus as usize].lock().identify_drive(drive) {
            // Convert u16 array to bytes, but keep the original byte order for strings
            // ATA strings are stored with bytes swapped within each 16-bit word
            let mut buf = [0u8; 512];
            for (i, &word) in res.iter().enumerate() {
                let bytes = word.to_le_bytes(); // Keep little-endian for proper string parsing
                buf[i * 2] = bytes[1];     // Swap bytes within each word for strings
                buf[i * 2 + 1] = bytes[0];
            }

            // Extract serial number (20 bytes starting at word 10, byte offset 20)
            let serial = {
                let serial_bytes = &buf[20..40];
                // Convert bytes to string, removing null terminators and trimming
                let serial_str = core::str::from_utf8(serial_bytes)
                    .unwrap_or("")
                    .trim_end_matches('\0')
                    .trim();
                serial_str.into()
            };

            // Extract model name (40 bytes starting at word 27, byte offset 54)
            let model = {
                let model_bytes = &buf[54..94];
                // Convert bytes to string, removing null terminators and trimming
                let model_str = core::str::from_utf8(model_bytes)
                    .unwrap_or("")
                    .trim_end_matches('\0')
                    .trim();
                model_str.into()
            };

            // Extract block count (4 bytes starting at word 60, byte offset 120)
            // For this numeric value, use the original word order
            let blocks = {
                let word_60 = res[60];
                let word_61 = res[61];
                ((word_61 as u32) << 16) | (word_60 as u32)
            };

            let ata_drive = Self {
                bus,
                drive,
                model,
                serial,
                blocks,
            };
            info!("Drive {} opened", ata_drive);
            Some(ata_drive)
        } else {
            warn!("Drive {}@{} is not a PATA drive", bus, drive);
            None
        }
    }

    fn block_size(&self) -> usize {
        512 // Standard ATA block size
    }

    fn block_count(&self) -> Result<usize, &'static str> {
        Ok(self.blocks as usize)
    }

    /// Read a block from the drive
    pub fn read_block_raw(&self, block: u32, buf: &mut [u8]) -> Result<(), &'static str> {
        BUSES[self.bus as usize].lock().read_pio(self.drive, block, buf)
    }

    /// Write a block to the drive
    pub fn write_block_raw(&self, block: u32, buf: &[u8]) -> Result<(), &'static str> {
        BUSES[self.bus as usize].lock().write_pio(self.drive, block, buf)
    }

    fn humanized_size(&self) -> (f32, &'static str) {
        let size = self.block_size();
        let count = self.block_count().unwrap();
        let bytes = size * count;

        crate::humanized_size(bytes as u64)
    }
}

impl core::fmt::Display for AtaDrive {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        let (size, unit) = self.humanized_size();
        write!(f, "{} {} ({} {})", self.model, self.serial, size, unit)
    }
}


use storage::{Block512, BlockDevice};

impl BlockDevice<Block512> for AtaDrive {
    fn block_count(&self) -> storage::FsResult<usize> {
        // Return the block count
        Ok(self.blocks as usize)
    }

    fn read_block(&self, offset: usize, block: &mut Block512) -> storage::FsResult {
        // Read the block
        // Use BUSES and self to get bus
        // Use read_pio to get data
        BUSES[self.bus as usize]
            .lock()
            .read_pio(self.drive, offset as u32, block.as_mut())
            .map_err(|_| storage::DeviceError::ReadError.into())
    }

    fn write_block(&self, offset: usize, block: &Block512) -> storage::FsResult {
        // Write the block
        // Use BUSES and self to get bus
        // Use write_pio to write data
        BUSES[self.bus as usize]
            .lock()
            .write_pio(self.drive, offset as u32, block.as_ref())
            .map_err(|_| storage::DeviceError::WriteError.into())
    }
}
