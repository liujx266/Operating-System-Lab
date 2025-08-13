//! File
//!
//! reference: <https://wiki.osdev.org/FAT#Directories_on_FAT12.2F16.2F32>

use super::*;

#[derive(Debug, Clone)]
pub struct File {
    /// The current offset in the file
    offset: usize,
    /// The current cluster of this file
    current_cluster: Cluster,
    /// DirEntry of this file
    entry: DirEntry,
    /// The file system handle that contains this file
    handle: Fat16Handle,
}

impl File {
    pub fn new(handle: Fat16Handle, entry: DirEntry) -> Self {
        Self {
            offset: 0,
            current_cluster: entry.cluster,
            entry,
            handle,
        }
    }

    pub fn length(&self) -> usize {
        self.entry.size as usize
    }
}

impl Read for File {
    fn read(&mut self, buf: &mut [u8]) -> FsResult<usize> {
        // Check if we've reached the end of the file
        if self.offset >= self.length() {
            return Ok(0);
        }

        // Calculate how much we can actually read
        let remaining_file_bytes = self.length() - self.offset;
        let bytes_to_read = buf.len().min(remaining_file_bytes);

        if bytes_to_read == 0 {
            return Ok(0);
        }

        let mut bytes_read = 0;
        let mut current_offset = self.offset;

        while bytes_read < bytes_to_read {
            // Calculate which cluster and sector we need
            let cluster_size = self.handle.bpb.sectors_per_cluster() as usize * BLOCK_SIZE;
            let cluster_offset = current_offset % cluster_size;
            let sector_offset_in_cluster = cluster_offset / BLOCK_SIZE;
            let byte_offset_in_sector = cluster_offset % BLOCK_SIZE;

            // Get the sector to read from
            let sector = self.handle.cluster_to_sector(&self.current_cluster) + sector_offset_in_cluster;

            // Read the sector
            let mut block = Block512::default();
            self.handle.inner.read_block(sector, &mut block)?;

            // Calculate how much to copy from this sector
            let bytes_remaining_in_sector = BLOCK_SIZE - byte_offset_in_sector;
            let bytes_remaining_to_read = bytes_to_read - bytes_read;
            let bytes_to_copy = bytes_remaining_in_sector.min(bytes_remaining_to_read);

            // Copy data from sector to buffer
            let src_start = byte_offset_in_sector;
            let src_end = src_start + bytes_to_copy;
            let dst_start = bytes_read;
            let dst_end = dst_start + bytes_to_copy;

            buf[dst_start..dst_end].copy_from_slice(&block.as_ref()[src_start..src_end]);

            bytes_read += bytes_to_copy;
            current_offset += bytes_to_copy;

            // Check if we need to move to the next cluster
            if cluster_offset + bytes_to_copy >= cluster_size && bytes_read < bytes_to_read {
                // Move to next cluster
                self.current_cluster = self.handle.get_next_cluster(&self.current_cluster)?;

                // Check for end of file
                if self.current_cluster == Cluster::END_OF_FILE {
                    break;
                }
            }
        }

        // Update the file offset
        self.offset += bytes_read;

        Ok(bytes_read)
    }
}

// NOTE: `Seek` trait is not required for this lab
impl Seek for File {
    fn seek(&mut self, _pos: SeekFrom) -> FsResult<usize> {
        unimplemented!()
    }
}

// NOTE: `Write` trait is not required for this lab
impl Write for File {
    fn write(&mut self, _buf: &[u8]) -> FsResult<usize> {
        unimplemented!()
    }

    fn flush(&mut self) -> FsResult {
        unimplemented!()
    }
}
