use super::ata::*;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::ToString;
use storage::fat16::Fat16;
use storage::mbr::*;
use storage::*;

pub static ROOTFS: spin::Once<Mount> = spin::Once::new();

pub fn get_rootfs() -> &'static Mount {
    ROOTFS.get().unwrap()
}

pub fn init() {
    info!("Opening disk device...");

    let drive = AtaDrive::open(0, 0).expect("Failed to open disk device");

    // only get the first partition
    let part = MbrTable::parse(drive)
        .expect("Failed to parse MBR")
        .partitions()
        .expect("Failed to get partitions")
        .remove(0);

    info!("Mounting filesystem...");

    ROOTFS.call_once(|| Mount::new(Box::new(Fat16::new(part)), "/".into()));

    trace!("Root filesystem: {:#?}", ROOTFS.get().unwrap());

    info!("Initialized Filesystem.");
}

pub fn ls(root_path: &str) {
    let iter = match get_rootfs().read_dir(root_path) {
        Ok(iter) => iter,
        Err(err) => {
            warn!("{:?}", err);
            return;
        }
    };

    // Print table header
    println!("{:<20} {:>8} {:>20}", "Name", "Size", "Modified");
    println!("{:-<50}", "");

    // Iterate over the entries and format them
    for meta in iter {
        let name = if meta.is_dir() {
            format!("{}/", meta.name)
        } else {
            meta.name.clone()
        };

        let (size, unit) = if meta.is_dir() {
            (0.0, "")
        } else {
            crate::humanized_size_short(meta.len as u64)
        };

        let size_str = if meta.is_dir() {
            "<DIR>".to_string()
        } else {
            format!("{:.1}{}", size, unit)
        };

        let modified_str = if let Some(modified) = meta.modified {
            format!("{}", modified.format("%Y-%m-%d %H:%M"))
        } else {
            "Unknown".to_string()
        };

        println!("{:<20} {:>8} {:>20}", name, size_str, modified_str);
    }
}

pub fn cat(file_path: &str) {
    let mut file_handle = match get_rootfs().open_file(file_path) {
        Ok(handle) => handle,
        Err(err) => {
            warn!("Failed to open file '{}': {:?}", file_path, err);
            return;
        }
    };

    let mut buffer = alloc::vec![0u8; file_handle.meta.len];
    match file_handle.read(&mut buffer) {
        Ok(bytes_read) => {
            if let Ok(content) = alloc::string::String::from_utf8(buffer[..bytes_read].to_vec()) {
                print!("{}", content);
            } else {
                warn!("File '{}' contains non-UTF8 data", file_path);
            }
        }
        Err(err) => {
            warn!("Failed to read file '{}': {:?}", file_path, err);
        }
    }
}
