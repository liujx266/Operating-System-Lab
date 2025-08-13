use super::*;
use alloc::vec::Vec;
use alloc::string::ToString;

impl Fat16Impl {
    pub fn new(inner: impl BlockDevice<Block512>) -> Self {
        let mut block = Block::default();
        let _block_size = Block512::size();

        inner.read_block(0, &mut block).unwrap();
        let bpb = Fat16Bpb::new(block.as_ref()).unwrap();

        trace!("Loading Fat16 Volume: {:#?}", bpb);

        // HINT: FirstDataSector = BPB_ResvdSecCnt + (BPB_NumFATs * FATSz) + RootDirSectors;
        let fat_start = bpb.reserved_sector_count() as usize;

        // Calculate root directory size in sectors
        // RootDirSectors = ((BPB_RootEntCnt * 32) + (BPB_BytsPerSec – 1)) / BPB_BytsPerSec
        let root_dir_size = ((bpb.root_entries_count() as usize * DirEntry::LEN) +
                            (bpb.bytes_per_sector() as usize - 1)) / bpb.bytes_per_sector() as usize;

        // First root directory sector = reserved sectors + (number of FATs * sectors per FAT)
        let first_root_dir_sector = fat_start + (bpb.fat_count() as usize * bpb.sectors_per_fat() as usize);

        // First data sector = first root dir sector + root dir size
        let first_data_sector = first_root_dir_sector + root_dir_size;

        Self {
            bpb,
            inner: Box::new(inner),
            fat_start,
            first_data_sector,
            first_root_dir_sector,
        }
    }

    pub fn cluster_to_sector(&self, cluster: &Cluster) -> usize {
        match *cluster {
            Cluster::ROOT_DIR => self.first_root_dir_sector,
            Cluster(c) => {
                // HINT: FirstSectorofCluster = ((N – 2) * BPB_SecPerClus) + FirstDataSector;
                // Clusters 0 and 1 are reserved, so data clusters start from 2
                if c < 2 {
                    panic!("Invalid cluster number: {}", c);
                }
                ((c - 2) * self.bpb.sectors_per_cluster() as u32) as usize + self.first_data_sector
            }
        }
    }

    /// Read the FAT table to get the next cluster in the chain
    pub fn get_next_cluster(&self, cluster: &Cluster) -> FsResult<Cluster> {
        match *cluster {
            Cluster::ROOT_DIR => Err(FsError::InvalidOperation),
            Cluster(c) => {
                // Each FAT entry is 2 bytes in FAT16
                let fat_offset = c as usize * 2;
                let fat_sector = self.fat_start + (fat_offset / BLOCK_SIZE);
                let fat_entry_offset = fat_offset % BLOCK_SIZE;

                let mut block = Block512::default();
                self.inner.read_block(fat_sector, &mut block)?;

                let fat_entry = u16::from_le_bytes([
                    block.as_ref()[fat_entry_offset],
                    block.as_ref()[fat_entry_offset + 1],
                ]);

                // Check for end of file or bad cluster
                if fat_entry >= 0xFFF8 {
                    Ok(Cluster::END_OF_FILE)
                } else if fat_entry == 0xFFF7 {
                    Ok(Cluster::BAD)
                } else if fat_entry == 0x0000 {
                    Ok(Cluster::EMPTY)
                } else {
                    Ok(Cluster(fat_entry as u32))
                }
            }
        }
    }

    /// Read all directory entries from a directory cluster
    pub fn read_dir_entries(&self, dir: &Directory) -> FsResult<Vec<DirEntry>> {
        let mut entries = Vec::new();
        let mut current_cluster = dir.cluster;

        loop {
            let sector_start = self.cluster_to_sector(&current_cluster);
            let sectors_per_cluster = if current_cluster == Cluster::ROOT_DIR {
                // Root directory size in sectors
                ((self.bpb.root_entries_count() as usize * DirEntry::LEN) +
                 (self.bpb.bytes_per_sector() as usize - 1)) / self.bpb.bytes_per_sector() as usize
            } else {
                self.bpb.sectors_per_cluster() as usize
            };

            // Read all sectors in this cluster
            for sector_offset in 0..sectors_per_cluster {
                let mut block = Block512::default();
                self.inner.read_block(sector_start + sector_offset, &mut block)?;

                // Parse directory entries from this sector
                let sector_data = block.as_ref();
                for entry_offset in (0..BLOCK_SIZE).step_by(DirEntry::LEN) {
                    if entry_offset + DirEntry::LEN > BLOCK_SIZE {
                        break;
                    }

                    let entry_data = &sector_data[entry_offset..entry_offset + DirEntry::LEN];

                    // Check if this is the end of directory entries
                    if entry_data[0] == 0x00 {
                        return Ok(entries);
                    }

                    // Skip deleted entries
                    if entry_data[0] == 0xE5 {
                        continue;
                    }

                    // Parse the directory entry
                    match DirEntry::parse(entry_data) {
                        Ok(entry) => {
                            if entry.is_valid() && !entry.is_long_name() {
                                entries.push(entry);
                            }
                        }
                        Err(_) => continue, // Skip invalid entries
                    }
                }
            }

            // Move to next cluster if not root directory
            if current_cluster == Cluster::ROOT_DIR {
                break;
            }

            current_cluster = self.get_next_cluster(&current_cluster)?;
            if current_cluster == Cluster::END_OF_FILE {
                break;
            }
        }

        Ok(entries)
    }

    /// Find a directory entry by name in the given directory
    pub fn find_dir_entry(&self, dir: &Directory, name: &str) -> FsResult<DirEntry> {
        let entries = self.read_dir_entries(dir)?;
        let target_sfn = ShortFileName::parse(name)?;

        for entry in entries {
            if entry.filename.matches(&target_sfn) {
                return Ok(entry);
            }
        }

        Err(FsError::FileNotFound)
    }

    /// Parse a path and navigate to the target file or directory
    pub fn parse_path(&self, path: &str) -> FsResult<DirEntry> {
        // Start from root directory
        let mut current_dir = Directory::root();

        // Handle root path
        if path == "/" || path.is_empty() {
            return Err(FsError::NotAFile); // Root is a directory, not a file
        }

        // Split path into components
        let components: Vec<&str> = path.trim_start_matches('/').split('/').filter(|s| !s.is_empty()).collect();

        if components.is_empty() {
            return Err(FsError::NotAFile);
        }

        // Navigate through path components
        for (i, component) in components.iter().enumerate() {
            let entry = self.find_dir_entry(&current_dir, component)?;

            // If this is the last component, return it
            if i == components.len() - 1 {
                return Ok(entry);
            }

            // Otherwise, it must be a directory to continue
            if !entry.is_directory() {
                return Err(FsError::NotADirectory);
            }

            // Move to the next directory
            current_dir = Directory::from_entry(entry);
        }

        Err(FsError::FileNotFound)
    }

    /// Parse a path and return the directory containing the target
    pub fn parse_path_to_dir(&self, path: &str) -> FsResult<Directory> {
        // Handle root path
        if path == "/" || path.is_empty() {
            return Ok(Directory::root());
        }

        // Split path into components
        let components: Vec<&str> = path.trim_start_matches('/').split('/').filter(|s| !s.is_empty()).collect();

        if components.is_empty() {
            return Ok(Directory::root());
        }

        // If only one component, return root
        if components.len() == 1 {
            return Ok(Directory::root());
        }

        // Navigate to parent directory
        let mut current_dir = Directory::root();
        for component in &components[..components.len() - 1] {
            let entry = self.find_dir_entry(&current_dir, component)?;

            if !entry.is_directory() {
                return Err(FsError::NotADirectory);
            }

            current_dir = Directory::from_entry(entry);
        }

        Ok(current_dir)
    }
}

impl FileSystem for Fat16 {
    fn read_dir(&self, path: &str) -> FsResult<Box<dyn Iterator<Item = Metadata> + Send>> {
        // Get the directory to read
        let dir = if path == "/" || path.is_empty() {
            Directory::root()
        } else {
            // Try to find the directory entry first
            match self.handle.parse_path(path) {
                Ok(entry) => {
                    if !entry.is_directory() {
                        return Err(FsError::NotADirectory);
                    }
                    Directory::from_entry(entry)
                }
                Err(_) => return Err(FsError::FileNotFound),
            }
        };

        // Read directory entries and convert to metadata
        let entries = self.handle.read_dir_entries(&dir)?;
        let metadata_vec: Vec<Metadata> = entries.iter().map(|entry| entry.into()).collect();

        Ok(Box::new(metadata_vec.into_iter()))
    }

    fn open_file(&self, path: &str) -> FsResult<FileHandle> {
        // Parse the path to get the file entry
        let entry = self.handle.parse_path(path)?;

        // Make sure it's a file, not a directory
        if entry.is_directory() {
            return Err(FsError::NotAFile);
        }

        // Create file handle
        let file = File::new(self.handle.clone(), entry.clone());
        let metadata = Metadata::from(&entry);

        Ok(FileHandle::new(metadata, Box::new(file)))
    }

    fn metadata(&self, path: &str) -> FsResult<Metadata> {
        // Handle root directory
        if path == "/" || path.is_empty() {
            return Ok(Metadata::new(
                "/".to_string(),
                FileType::Directory,
                0,
                None,
                None,
                None,
            ));
        }

        // Parse the path to get the entry
        let entry = self.handle.parse_path(path)?;
        Ok(Metadata::from(&entry))
    }

    fn exists(&self, path: &str) -> FsResult<bool> {
        // Handle root directory
        if path == "/" || path.is_empty() {
            return Ok(true);
        }

        // Try to parse the path
        match self.handle.parse_path(path) {
            Ok(_) => Ok(true),
            Err(FsError::FileNotFound) => Ok(false),
            Err(e) => Err(e),
        }
    }
}
