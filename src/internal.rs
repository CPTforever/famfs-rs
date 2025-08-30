use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{ffi::CString, ptr::NonNull};
use std::cell::OnceCell;

use crate::meta::{famfs_log_fmap, famfs_log_fmap_union_interleaved_extent, famfs_system_role, FAMFS_ALLOC_UNIT, FAMFS_LOG_OFFSET, FAMFS_MAX_PATHLEN, FAMFS_SUPERBLOCK_SIZE};
use crate::{Famfs, FamfsFile};
use super::meta::{famfs_interleave_param, famfs_log, famfs_superblock, Extent};
use super::bitmap::Bitmap;

#[repr(C)]
pub struct famfs_locked_log {
    devsize: u64, 
    logp: *mut famfs_log,
    lfd: i32, // why store the error code at all??
    famfs_type: famfs_system_role, 
    bitmap: OnceCell<Bitmap>,
    nbits: u64,
    alloc_unit: u64,
    cur_pos: u64, 
    interleave_param: famfs_interleave_param,
    mpt: PathBuf,
    shadow_root: PathBuf,
}

#[repr(C)]
pub(crate) struct famfs_log_stats {
    n_entries: u64, 
    bad_entries: u64,
    f_logged: u64,
    f_existed: u64,
    f_created: u64,
    f_errs: u64, 
    d_logged: u64,
    d_existed: u64,
    d_created: u64, 
    d_errs: u64,
    yaml_errs: u64,
    yaml_checked: u64
}

impl famfs_locked_log {
    // takes from the log, without locking...
    // though the synchronization required to actually lock 
    pub unsafe fn from_log(logp: *mut famfs_log, sb: &famfs_superblock) -> famfs_locked_log {
        famfs_locked_log {
            devsize: sb.ts_daxdev.dd_size as u64,
            logp: logp,
            lfd: 0,
            famfs_type: famfs_system_role::FAMFS_MASTER,
            bitmap: OnceCell::new(),
            nbits: 0,
            alloc_unit: FAMFS_ALLOC_UNIT,
            cur_pos: 0,
            interleave_param: famfs_interleave_param::default(),
            mpt: PathBuf::new(),
            shadow_root: PathBuf::new(),
        }
    }

    fn bitmap(&self) -> &Bitmap {
        { 
            self.bitmap.get_or_init(|| {
                Bitmap::build_bitmap(self.logp, self.alloc_unit, self.devsize)
            });
        }

        self.bitmap.get().unwrap()
    }

    fn bitmap_mut(&mut self) -> &mut Bitmap {
        self.bitmap();

        self.bitmap.get_mut().unwrap()
    }

    fn file_alloc_contiguous(&mut self, size: u64) -> Result<famfs_log_fmap, i64> {
        let mut cur_pos = self.cur_pos;
        let offset = self.bitmap_mut().alloc_contiguous(size, &mut cur_pos, 0).ok_or(0)?;
        self.cur_pos = cur_pos;

        Ok(famfs_log_fmap::generate_simple_fmap(size, offset))
    }

    fn file_alloc(&mut self, size: u64) -> Result<famfs_log_fmap, i64> {
        // todo add interleaved allocations
        self.file_alloc_contiguous(size)
    }

    pub fn make_file(        
        &mut self, 
        path: &Path,
        mode_t: u32,
        uid_t: u32,
        gid_t: u32,
        size: u64
    ) -> Result<(), i64> {
        let fmap = self.file_alloc(size)?;
        unsafe { (*self.logp).log_file_create(&fmap, path, mode_t, uid_t, gid_t, size)?; }

        Ok(())
    }

    pub fn get_file(&self, path: &Path) -> Option<FamfsFile> {
        let log = unsafe { self.logp.as_ref().unwrap() };

        for i in 0..log.len() {
            let entry = unsafe { log.get_entry_ref(i as usize) };
            let entry_type = entry.get_entry_type();
            match entry_type {
                crate::meta::LogEntry::File { file_meta } => {
                    let encoded_path = path.as_os_str().as_encoded_bytes();
                    if encoded_path.len() > FAMFS_MAX_PATHLEN {
                        return None;
                    }

                    if &file_meta.fm_relpath[..encoded_path.len()] == encoded_path {
                        let base = match file_meta.get_extent() {
                            Extent::Simple { extent } => {
                                println!("extent.se[0].se_offset {}", extent.se[0].se_offset);
                                unsafe {
                                    self.logp
                                        .cast::<u8>()
                                        .offset(-(FAMFS_SUPERBLOCK_SIZE as isize))
                                        .offset(extent.se[0].se_offset as isize)
                                }
                            },
                            _ => return None,
                        };
                        return Some(FamfsFile {
                            base: base,
                            len: file_meta.fm_size as usize,
                            cur: 0,
                        })
                    }
                },
                _ => continue
            }
        }

        None
    }

    pub fn print_bitmap(&self) {
        for i in 0..self.bitmap().len() {
            print!("{}", if self.bitmap().test(i) {1} else {0});
        }
        println!();
    }
}