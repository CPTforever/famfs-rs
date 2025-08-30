use std::{mem::ManuallyDrop, path::Path};

use uuid::Uuid;

pub const FAMFS_SUPER_MAGIC: u64 = 0x87b282ff; // Memory superblock magic number
pub const FAMFS_STATFS_MAGIC_V1: u64 = 0x87b282fe; // v1 statfs magic number
pub const FAMFS_STATFS_MAGIC: u64 = 0x87b282fd; // fuse statfs magic number

pub const FAMFS_LOG_OFFSET: u64 = 0x200000; // 2MiB
pub const FAMFS_LOG_LEN: u64 = 0x800000; // 8MiB 

pub const FAMFS_SUPERBLOCK_SIZE: u64 = FAMFS_LOG_OFFSET;
pub const FAMFS_SUPERBLOCK_MAX_DAXDEVS: u64 = 1;

pub const FAMFS_ALLOC_UNIT: u64 = 0x200000; // 2MiB allocation unit

pub const FAMFS_DEVNAME_LEN: usize = 64;
pub const FAMFS_CURRENT_VERSION: u64 = 47;

pub const FAMFS_OMF_VER_MAJOR: usize = 2;
pub const FAMFS_OMF_VER_MINOR: usize = 1;

pub const FAMFS_PRIMARY_SB: usize = 1 << 0;
pub const FAMFS_SECONDARY_SB: usize = 1 << 0;

pub const FAMFS_MAX_SIMPLE_EXTENTS: usize = 16;
pub const FAMFS_MAX_INTERLEAVED_EXTENTS: usize = 1;
pub const FAMFS_MAX_NBUCKETS: usize = 64;

pub const FAMFS_MAX_PATHLEN: usize = 80;
pub const FAMFS_MAX_HOSTNAME_LEN: usize = 32;
pub const FAMFS_FM_BUF_LEN: usize = 512; 

pub const FAMFS_FM_ALL_HOSTS_RO: u32 = 1 << 0;
pub const FAMFS_FM_ALL_HOSTS_RW: u32 = 1 << 1;

pub const FAMFS_LOG_MAGIC: u64 = 0xbadcafef00d;

pub(crate) const MIN_DEVSIZE: usize = 4 * 1024 * 1024 * 1024;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct famfs_daxdev {
    pub(crate) dd_size: usize, 
    pub(crate) dd_uuid: Uuid,
    pub(crate) daxdev: [u8; FAMFS_DEVNAME_LEN]
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct famfs_superblock {
    pub(crate) ts_magic:           u64,
    pub(crate) ts_version:         u64, 
    pub(crate) ts_log_offset:      u64, 
    pub(crate) ts_log_len:         u64, 
    pub(crate) ts_alloc_unit:      u64, 
    pub(crate) ts_omf_ver_major:   u32,
    pub(crate) ts_omf_ver_minor:   u32, 
    pub(crate) ts_uuid:            Uuid,
    pub(crate) ts_dev_uuid:        Uuid,
    pub(crate) ts_system_uuid:     Uuid,
    ts_crc:             u32,
    ts_pad:             u32, // change this later 
    pub(crate) ts_sb_flags:        u32,
    pub(crate) ts_daxdev:          famfs_daxdev
}

impl famfs_superblock {
    // We need to find a OS indepedent way to do this
    fn get_role() -> famfs_system_role {
        famfs_system_role::FAMFS_MASTER
    }

    fn regenerate_crc(&mut self) {
        self.ts_crc = self.generate_crc();
    }

    fn generate_crc(&self) -> u32 {
        let mut crc = crc32fast::Hasher::new();

        crc.update(&self.ts_magic.to_ne_bytes()); // is it native endian?
        crc.update(&self.ts_version.to_ne_bytes());
        crc.update(&self.ts_log_offset.to_ne_bytes());
        crc.update(&self.ts_log_len.to_ne_bytes());
        crc.update(&self.ts_alloc_unit.to_ne_bytes());
        crc.update(&self.ts_omf_ver_major.to_ne_bytes());
        crc.update(&self.ts_omf_ver_minor.to_ne_bytes());
        crc.update(self.ts_uuid.as_bytes());
        crc.update(self.ts_dev_uuid.as_bytes());
        crc.update(self.ts_system_uuid.as_bytes());

        crc.finalize()
    }
    
    // Returns true if the superblock is valid
    pub fn check_superblock(&self) -> bool {
        if self.ts_magic != FAMFS_SUPER_MAGIC {
            return false;
        }

        if self.ts_version != FAMFS_CURRENT_VERSION {
            return false;
        }

        if self.ts_crc != self.generate_crc() {
            return false;
        }

        if self.ts_alloc_unit != 4096 && self.ts_alloc_unit != 0x200000 {
            return false;
        }

        return true;
    }

    fn daxdev_uuid() {}

    pub fn daxdev_size(&self) -> usize {
        self.ts_daxdev.dd_size
    }
}

#[repr(u8)]
pub enum famfs_system_role {
    FAMFS_MASTER = 1,
    FAMFS_CLIENT,
    FAMFS_NOSUPER
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
enum famfs_log_ext_type {
    FAMFS_EXT_SIMPLE = 0,
    FAMFS_EXT_INTERLEAVE = 1
}


#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct famfs_simple_extent {
    pub se_devindex:    u64,
    pub se_offset:      u64, 
    pub se_len:         u64
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct famfs_interleaved_ext {
    pub ie_nstrips:     u64,
    pub ie_chunk_size:  u64,
    pub ie_strips: [famfs_simple_extent; FAMFS_MAX_SIMPLE_EXTENTS]
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct famfs_log_fmap_union_simple_extent {
    pub fmap_nextents: u32,
    pub se: [famfs_simple_extent; FAMFS_MAX_SIMPLE_EXTENTS]
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct famfs_log_fmap_union_interleaved_extent {
    pub fmap_niext: u32,
    pub se: [famfs_interleaved_ext; FAMFS_MAX_INTERLEAVED_EXTENTS]
}

#[repr(C)]
#[derive(Clone, Copy)]
union famfs_log_fmap_union {
    simple: std::mem::ManuallyDrop<famfs_log_fmap_union_simple_extent>,
    interleaved: std::mem::ManuallyDrop<famfs_log_fmap_union_interleaved_extent>
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct famfs_log_fmap {
    fmap_ext_type: famfs_log_ext_type,
    inner: famfs_log_fmap_union
}

impl famfs_log_fmap {
    pub fn generate_simple_fmap(size: u64, offset: u64) -> famfs_log_fmap {
        let mut simple_extent = famfs_log_fmap_union_simple_extent {
            fmap_nextents: 1,
            se: [famfs_simple_extent::default(); 16],
        };

        simple_extent.se[0] = famfs_simple_extent { 
            se_devindex: 0, // Must be 0 until multidevice support
            se_offset: offset, 
            se_len: size.div_ceil(FAMFS_ALLOC_UNIT) * FAMFS_ALLOC_UNIT
        };

        famfs_log_fmap {
            fmap_ext_type: famfs_log_ext_type::FAMFS_EXT_SIMPLE,
            inner: famfs_log_fmap_union {
                simple: {
                    ManuallyDrop::new(simple_extent)
                }
            },
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub enum famfs_log_entry_type {
    FAMFS_LOG_FILE,
    FAMFS_LOG_MKDIR,
    FAMFS_LOG_DELETE,
    FAMFS_LOG_INVALID
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct famfs_log_mkdir {
    md_uid: u32, 
    md_gid: u32, 
    md_mode: u32, 
    md_relpath: [u8; FAMFS_MAX_PATHLEN]
}

impl famfs_log_mkdir {
    pub fn relpath(&self) -> &str {
        std::str::from_utf8(&self.md_relpath).unwrap()
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct famfs_log_file_meta {
    pub fm_size: u64, 
    pub fm_flags: u32,

    pub fm_uid: u32,
    pub fm_gid: u32, 
    pub fm_mode: u32, 

    pub fm_relpath: [u8; FAMFS_MAX_PATHLEN],
    fm_fmap: famfs_log_fmap
}

impl famfs_log_file_meta {
    pub fn relpath(&self) -> &str {
        std::str::from_utf8(&self.fm_relpath).unwrap()
    }

    pub fn get_extent(&self) -> Extent {
        match self.fm_fmap.fmap_ext_type {
            famfs_log_ext_type::FAMFS_EXT_SIMPLE => Extent::Simple { extent: unsafe { *self.fm_fmap.inner.simple } },
            famfs_log_ext_type::FAMFS_EXT_INTERLEAVE =>  Extent::Interleaved { extent: unsafe { *self.fm_fmap.inner.interleaved } },
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
union famfs_log_entry_union {
    famfs_fm: std::mem::ManuallyDrop<famfs_log_file_meta>,
    famfs_md: std::mem::ManuallyDrop<famfs_log_mkdir>
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct famfs_log_entry {
    famfs_log_entry_seqnum: u64, 
    famfs_log_entry_type: famfs_log_entry_type, 
    famfs_log_entry_log: famfs_log_entry_union,
    famfs_log_entry_crc: u32,
    famfs_pad:           u32 // AHHH
}

#[derive(Debug, Clone, Copy)]
pub enum Extent {
    Simple {extent: famfs_log_fmap_union_simple_extent},
    Interleaved {extent: famfs_log_fmap_union_interleaved_extent}
}

pub enum LogEntry<'a> {
    File {file_meta: &'a famfs_log_file_meta},
    MakeDir {dir_meta: &'a famfs_log_mkdir},
    Delete,
    Invalid
}

impl famfs_log_entry {
    pub fn seqnum(&self) -> u64 {
        self.famfs_log_entry_seqnum
    }

    pub fn entry_type(&self) -> famfs_log_entry_type {
        self.famfs_log_entry_type
    }

    pub fn generate_crc(&self) -> u32 {
        let mut crc32 = crc32fast::Hasher::new();

        let ptr = (self as *const Self).cast::<u8>();
        let crc_bytes = size_of::<Self>() - size_of::<u64>();

        let raw_buf = unsafe { std::slice::from_raw_parts(ptr, crc_bytes) };

        crc32.update(raw_buf);

        crc32.finalize()
    }

    pub fn regenerate_crc(&mut self) {
        self.famfs_log_entry_crc = self.generate_crc();
    }

    pub fn check_crc(&self) -> bool {
        self.famfs_log_entry_crc == self.generate_crc()
    }

    pub fn get_entry_type(&self) -> LogEntry {
        match self.famfs_log_entry_type {
            famfs_log_entry_type::FAMFS_LOG_FILE => {
                LogEntry::File { file_meta: unsafe { &self.famfs_log_entry_log.famfs_fm } }
            },
            famfs_log_entry_type::FAMFS_LOG_MKDIR => LogEntry::MakeDir { 
                dir_meta: unsafe { &self.famfs_log_entry_log.famfs_md } 
            },
            famfs_log_entry_type::FAMFS_LOG_DELETE => LogEntry::Delete,
            famfs_log_entry_type::FAMFS_LOG_INVALID => LogEntry::Invalid,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct famfs_log {
    pub famfs_log_magic: u64, 
    pub famfs_log_len: u64,
    pub famfs_log_last_index: u64,
    famfs_log_crc: u32, 
    pub famfs_log_next_seqnum: u64,
    pub famfs_log_next_index: u64,
}

impl famfs_log {
    pub fn check_log(&self) -> bool {
        if self.famfs_log_magic != FAMFS_LOG_MAGIC {
            return false;
        }

        return true;
    }

    // this assumes that the famfs_log exists in a memory mapped z
    // adjacent to it's log entries which are also memory mapped
    unsafe fn get_entry(&self, i: usize) -> *const famfs_log_entry {
        let self_ptr = self as *const famfs_log;

        let entry_ptr: *const famfs_log_entry = unsafe {
            self_ptr.add(1).cast::<famfs_log_entry>().add(i)
        };

        entry_ptr
    }

    pub unsafe fn get_entry_ref(&self, i: usize) -> &famfs_log_entry {
        unsafe {self.get_entry(i).as_ref().unwrap()}
    }

    unsafe fn get_entry_mut(&mut self, i: usize) -> *mut famfs_log_entry {
        unsafe { 
            self.get_entry(i) as *mut famfs_log_entry
        }
    }

    pub unsafe fn get_entry_ref_mut(&mut self, i: usize) -> &mut famfs_log_entry {
        unsafe {self.get_entry_mut(i).as_mut().unwrap()}
    }

    pub fn byte_len(&self) -> u64 {
        self.famfs_log_len
    }

    pub fn len(&self) -> u64 {
        self.famfs_log_next_index
    }

    pub fn max_size(&self) -> u64 {
        self.famfs_log_last_index
    }

    pub fn log_full(&self) -> bool {
        self.famfs_log_next_index > self.famfs_log_last_index
    }

    // not thread safe or any other kind of safe
    pub unsafe fn append_entry(&mut self, mut entry: famfs_log_entry) {
        entry.famfs_log_entry_seqnum = self.famfs_log_next_seqnum;
        unsafe { *self.get_entry_mut(self.famfs_log_next_index as usize) = entry };
        self.famfs_log_next_index+=1;
        self.famfs_log_next_seqnum+=1;
    }

    // not reentrant
    pub unsafe fn log_file_create(
        &mut self, 
        fmap: &famfs_log_fmap, 
        path: &Path,
        mode_t: u32,
        uid_t: u32,
        gid_t: u32,
        size: u64
    ) -> Result<(), i64> {
        let path_bytes = path.as_os_str().as_encoded_bytes();
        let mut relpath: [u8; FAMFS_MAX_PATHLEN] = [0; 80];
        relpath[..path_bytes.len()].copy_from_slice(path_bytes);

        let mut le = famfs_log_entry {
            famfs_log_entry_seqnum: self.famfs_log_next_seqnum,
            famfs_log_entry_type: famfs_log_entry_type::FAMFS_LOG_FILE,
            famfs_log_entry_log: famfs_log_entry_union {
                famfs_fm: ManuallyDrop::new(
                    famfs_log_file_meta { 
                        fm_size:  size, 
                        fm_flags: FAMFS_FM_ALL_HOSTS_RW, // hard coded for now
                        fm_uid: uid_t, 
                        fm_gid: gid_t, 
                        fm_mode: mode_t, 
                        fm_relpath: relpath, 
                        fm_fmap: *fmap
                    })
            },
            famfs_log_entry_crc: 0,
            famfs_pad: 0
        };

        le.regenerate_crc();

        if self.log_full() {
            return Err(0);
        }
        unsafe { self.append_entry(le); }

        Ok(())
    }
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct famfs_interleave_param {
    pub(crate) nbuckets: u64,
    pub(crate) nstrips: u64,
    pub(crate) chunk_size: u64
}

impl famfs_interleave_param {
    fn validate_interleave_param(
        &self,
        alloc_unit: u64,
        dev_size: u64 
    ) -> bool {
        todo!()
    }
}