use core::ptr::NonNull;
use super::{App, AppList}; // Use super to refer to lib.rs items
use arrayvec::{ArrayString, ArrayVec};
use core::mem::MaybeUninit;
use uefi::boot::*;
// Be more explicit with file imports
use uefi::proto::media::file::{Directory, FileAttribute, FileInfo, FileMode, FileType, RegularFile};
use uefi::proto::media::file::File; // Import the File trait
use uefi::proto::media::fs::SimpleFileSystem;
use xmas_elf::ElfFile;

/// Open root directory
pub fn open_root() -> Directory {
    let handle = uefi::boot::get_handle_for_protocol::<SimpleFileSystem>()
        .expect("Failed to get handle for SimpleFileSystem");
    let mut fs = uefi::boot::open_protocol_exclusive::<SimpleFileSystem>(handle)
        .expect("Failed to get FileSystem");

    fs.open_volume().expect("Failed to open volume")
}

/// Open file at `path`
pub fn open_file(path: &str) -> RegularFile {
    let mut buf = [0; 64];
    let cstr_path = uefi::CStr16::from_str_with_buf(path, &mut buf).unwrap();

    let handle = open_root()
        .open(cstr_path, FileMode::Read, FileAttribute::empty())
        .expect("Failed to open file");

    match handle.into_type().expect("Failed to into_type") {
        FileType::Regular(regular) => regular,
        _ => panic!("Invalid file type"),
    }
}

/// Load file to new allocated pages
pub fn load_file(file: &mut RegularFile) -> &'static mut [u8] {
    let mut info_buf = [0u8; 0x100];
    let info = file
        .get_info::<FileInfo>(&mut info_buf)
        .expect("Failed to get file info");

    let pages = info.file_size() as usize / 0x1000 + 1;

    let mem_start =
        uefi::boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, pages)
            .expect("Failed to allocate pages");

    let buf = unsafe { core::slice::from_raw_parts_mut(mem_start.as_ptr(), pages * 0x1000) };
    let len = file.read(buf).expect("Failed to read file");

    info!(
        "Load file \"{}\" to memory, size = {}",
        info.file_name(),
        len
    );

    &mut buf[..len]
}

/// Free ELF files for which the buffer was created using 'load_file'
pub fn free_elf(elf: ElfFile) {
    let buffer = elf.input;
    let pages = buffer.len() / 0x1000 + 1;
    let mem_start = NonNull::new(buffer.as_ptr() as *mut u8).expect("Invalid pointer");

    info!("Free ELF file, pages = {}, addr = {:#x?}", pages, mem_start);

    unsafe {
        uefi::boot::free_pages(mem_start, pages).expect("Failed to free pages");
    }
}

/// Load apps into memory, when no fs implemented in kernel
///
/// List all file under "APP" and load them.
pub fn load_apps() -> AppList {
    let mut root = open_root();
    let mut buf = [0; 8];
    let cstr_path = uefi::CStr16::from_str_with_buf("\\APP\\", &mut buf).unwrap();

    let app_dir_handle = root
        .open(cstr_path, FileMode::Read, FileAttribute::READ_ONLY)
        .expect("Failed to open APP directory");

    let mut app_dir = match app_dir_handle.into_type().expect("Failed to into_type") {
        FileType::Dir(dir) => dir, // Use FileType::Dir
        _ => panic!("\\APP\\ is not a directory"),
    };

    let mut apps = ArrayVec::new();
    let mut entry_buf = [0u8; 0x100];

    loop {
        let info = app_dir
            .read_entry(&mut entry_buf)
            .expect("Failed to read entry");

        match info {
            Some(entry) => {
                // Skip "." and ".." by comparing CStr16
                if entry.file_name() == uefi::cstr16!(".") || entry.file_name() == uefi::cstr16!("..") {
                    continue;
                }

                let file_handle = app_dir
                    .open(entry.file_name(), FileMode::Read, FileAttribute::READ_ONLY)
                    .expect("Failed to open app file");

                let mut file = match file_handle.into_type().expect("Failed to into_type") {
                    FileType::Regular(regular) => regular,
                    // Skip directories within APP
                    FileType::Dir(_) => continue, // Use FileType::Dir
                };

                // Check if it's a directory using get_info (more reliable)
                let mut file_info_buf = [0u8; 0x100];
                let file_info = file
                    .get_info::<FileInfo>(&mut file_info_buf)
                    .expect("Failed to get file info for app");

                if file_info.attribute().contains(FileAttribute::DIRECTORY) {
                    continue;
                }


                let elf_data = load_file(&mut file);
                let elf = ElfFile::new(elf_data).expect("Failed to parse ELF file");

                let mut name = ArrayString::<16>::new();
                entry.file_name().as_str_in_buf(&mut name).unwrap();

                apps.push(App { name, elf });
            }
            None => break,
        }
    }

    info!("Loaded {} apps", apps.len());

    apps
}