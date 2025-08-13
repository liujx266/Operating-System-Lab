use crate::*;

/// The `Read` trait allows for reading bytes from a source.
pub trait Read {
    /// Pull some bytes from this source into the specified buffer, returning
    /// how many bytes were read.
    fn read(&mut self, buf: &mut [u8]) -> FsResult<usize>;

    /// Read all bytes until EOF in this source, placing them into `buf`.
    fn read_all(&mut self, buf: &mut Vec<u8>) -> FsResult<usize> {
        let start_len = buf.len();
        let mut total_read = 0;

        loop {
            // 确保缓冲区有足够的空间，每次增加4KB
            const CHUNK_SIZE: usize = 4096;
            if buf.len() - start_len - total_read < CHUNK_SIZE {
                buf.resize(buf.len() + CHUNK_SIZE, 0);
            }

            // 读取数据到缓冲区
            let bytes_read = match self.read(&mut buf[start_len + total_read..]) {
                Ok(0) => break, // EOF reached
                Ok(n) => n,
                Err(e) => return Err(e),
            };

            total_read += bytes_read;
        }

        // 调整缓冲区大小到实际读取的数据大小
        buf.truncate(start_len + total_read);

        Ok(total_read)
    }
}

/// The `Write` trait allows for writing bytes to a source.
///
/// NOTE: Leave here to ensure flexibility for the optional lab task.
pub trait Write {
    /// Write a buffer into this writer, returning how many bytes were written.
    fn write(&mut self, buf: &[u8]) -> FsResult<usize>;

    /// Flush this output stream, ensuring that all intermediately buffered
    /// contents reach their destination.
    fn flush(&mut self) -> FsResult;

    /// Attempts to write an entire buffer into this writer.
    fn write_all(&mut self, mut buf: &[u8]) -> FsResult {
        // not required for lab
        todo!()
    }
}

/// Enumeration of possible methods to seek within an I/O object.
#[derive(Copy, PartialEq, Eq, Clone, Debug)]
pub enum SeekFrom {
    /// Sets the offset to the provided number of bytes.
    Start(usize),

    /// Sets the offset to the size of this object plus the offset.
    End(isize),

    /// Sets the offset to the current position plus the offset.
    Current(isize),
}

/// The `Seek` trait provides a cursor within byte stream.
pub trait Seek {
    /// Seek to an offset, in bytes, in a stream.
    fn seek(&mut self, pos: SeekFrom) -> FsResult<usize>;
}

pub trait FileIO: Read + Write + Seek {}

impl<T: Read + Write + Seek> FileIO for T {}
