use famfs_rs::FamfsFile;
use memmap2::MmapOptions;
use std::io::{Write, Read};
use std::ops::Deref;
use std::path::{self, Path, PathBuf};
use std::fs::OpenOptions;

use famfs_rs::meta::{famfs_log, famfs_log_entry, famfs_superblock, FAMFS_LOG_LEN, FAMFS_LOG_OFFSET, FAMFS_SUPERBLOCK_SIZE, FAMFS_SUPER_MAGIC};
use famfs_rs::internal::famfs_locked_log;

fn main() {
    let file = OpenOptions::new()
                       .read(true)
                       .write(true)
                       .open("../foo/pmem-backing")
                       .unwrap();

    let mut mmap = unsafe { MmapOptions::new().offset(134217728).len(FAMFS_SUPERBLOCK_SIZE as usize).map_mut(&file).unwrap() };

    let superblock = mmap.as_mut_ptr().cast::<famfs_superblock>();
    let deref = unsafe {*superblock};

    let mut mmap = unsafe {
        MmapOptions::new()
            .offset(134217728 + FAMFS_LOG_OFFSET)
            .len(8573157376 - (134217728 + FAMFS_LOG_OFFSET) as usize)
            .map_mut(&file)
            .unwrap() 
    };

    let mut log = unsafe { mmap.as_mut_ptr().cast::<famfs_log>() };
    let mut log_ref = unsafe { log.as_mut().unwrap() };

    //log_ref.famfs_log_next_index-=1;
    //log_ref.famfs_log_next_seqnum-=1;

    //unsafe { log_ref.get_entry_ref_mut(34).regenerate_crc(); }
    let mut locked_log = unsafe { famfs_locked_log::from_log(log, superblock.as_ref().unwrap()) };

    let mut file = locked_log.get_file(Path::new("b.c")).unwrap();

    //locked_log.print_bitmap();
    //locked_log.make_file(Path::new("b.c"), 0o100644, 0, 0, 20000).unwrap();
    /*let mut opened_file = OpenOptions::new()
        .read(true)
        .write(true)
        .open("b.c")
        .unwrap();*/

    std::io::copy(&mut file, &mut std::io::stdout());
}
