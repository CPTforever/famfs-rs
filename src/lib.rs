pub mod meta;
pub mod internal;
pub mod bitmap;

use std::{collections::HashMap, io::{Read, Write}, path::Path, ptr::NonNull};
use meta::{famfs_superblock, famfs_log, FAMFS_MAX_PATHLEN};


trait FamfsMetadataInterface {
    fn superblock(&mut self) -> NonNull<famfs_superblock>;

    fn log(&mut self) -> NonNull<famfs_log>;

    fn commit(&mut self);
}

enum DirtyPages {
    superblock(usize),
    log(usize)
}
struct MMAPed {
    superblock: NonNull<famfs_superblock>,
    log: NonNull<famfs_log>,
    dirty_pages: Vec<DirtyPages>
}

struct Famfs {
    interface: Box<dyn FamfsMetadataInterface>
}

impl Famfs {
    fn new(interface: Box<dyn FamfsMetadataInterface>) -> Self {
        Self {
            interface: interface
        }
    }
}

pub struct FamfsFile {
    pub base: *mut u8,
    pub len: usize,
    pub cur: usize
}

// todo make safe
impl Read for FamfsFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.cur > self.len {
            return Err(std::io::Error::from_raw_os_error(0));
        }
        let bytes_read = std::cmp::min(buf.len(), self.len - self.cur);
        let src_slice = unsafe {
            std::slice::from_raw_parts(
                self.base.add(self.cur),
                bytes_read
            )
        };

        buf[..bytes_read].copy_from_slice(src_slice);
        
        self.cur+=bytes_read;
        Ok(bytes_read)
    }
}

// todo make safe (and correct)
impl Write for FamfsFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.cur > self.len {
            return Err(std::io::Error::from_raw_os_error(-1));
        }
        let bytes_written = std::cmp::min(buf.len(), self.len - self.cur);
        let src_slice = unsafe {
            std::slice::from_raw_parts_mut(
                self.base.offset(self.cur as isize),
                bytes_written
            )
        };

        src_slice[..bytes_written].copy_from_slice(&buf[..bytes_written]);

        Ok(bytes_written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

