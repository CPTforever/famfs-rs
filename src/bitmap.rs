use std::vec::Vec;
use crate::meta::famfs_log;
use crate::meta::FAMFS_SUPERBLOCK_SIZE;

const BYTE_SIZE: u64 = (size_of::<u8>() as u64) * 8;
const BYTE_SHIFT: u64 = 3;

pub(crate) struct Bitmap {
    backing: Vec<u8>,
    alloc_unit: u64,
    len: u64, // the number of bits 
}

impl Bitmap {
    pub fn build_bitmap(
        log: *mut famfs_log,
        alloc_unit: u64,
        dev_size_in: u64
    ) -> Bitmap {
        let mut logr = unsafe {log.as_mut().unwrap()};
        let nbits = dev_size_in.div_ceil(alloc_unit) as usize;
        let slots = (nbits + BYTE_SIZE as usize) >> (BYTE_SHIFT); // find the number of slots requried for the bitmap

        let backing = vec![0; slots];
        let mut errors = 0;
        let mut alloc_sum = 0;

        let mut bm = Bitmap {
            backing: backing,
            alloc_unit: alloc_unit,
            len: nbits as u64
        };
        bm.insert_meta_files(logr.byte_len(), &mut alloc_sum);

        for i in 0..logr.len() {
            let le = unsafe { logr.get_entry_ref(i as usize) };
            match le.get_entry_type() {
                super::meta::LogEntry::File { file_meta } => {
                    let extent = file_meta.get_extent();

                    match extent {
                        super::meta::Extent::Simple { extent } => {
                            for j in 0..(extent.fmap_nextents as usize) {
                                let indexed_extent = extent.se[j];
                                let offset = indexed_extent.se_offset;
                                let len = indexed_extent.se_len;

                                debug_assert!(offset % alloc_unit == 0);

                                let rc = bm.set_extent(offset, len, &mut alloc_sum);
                                errors += rc;
                            }
                        },
                        super::meta::Extent::Interleaved { extent } => {
                            let nstripes = extent.fmap_niext;
                            for j in 0..(nstripes as usize) {
                                let stripe = extent.se[j];
                                for k in 0..(stripe.ie_nstrips as usize) {
                                    let indexed_stripe = stripe.ie_strips[k];
                                    
                                    let offset = indexed_stripe.se_offset;
                                    let len = indexed_stripe.se_len;

                                    let rc = bm.set_extent(offset, len, &mut alloc_sum);
                                    errors += rc;
                                }
                            }
                        },
                    }
                },
                super::meta::LogEntry::MakeDir { dir_meta: _ } => continue,
                super::meta::LogEntry::Delete => panic!("invalid file_ext type"),
                super::meta::LogEntry::Invalid => panic!("invalid file_ext type"),
            }
        }

        bm
    }
    
    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn alloc_unit(&self) -> u64 {
        self.alloc_unit
    }

    pub fn set(&mut self, index: u64) {
        let byte_num = (index >> BYTE_SHIFT) as usize;
        let bit_num = (index % BYTE_SIZE) as usize;

        self.backing[byte_num] |= 1 << bit_num;
    }

    pub fn test(&self, index: u64) -> bool {
        let byte_num = (index >> BYTE_SHIFT) as usize;
        let bit_num = (index % BYTE_SIZE) as usize;

        (self.backing[byte_num] & (1 << bit_num)) > 0
    }

    pub fn test_and_set(&mut self, index: u64) -> bool {
        let byte_num = (index >> BYTE_SHIFT) as usize;
        let bit_num = (index % BYTE_SIZE) as usize;

        if (self.backing[byte_num] & (1 << bit_num)) > 0 {
            return false;
        }

        self.backing[byte_num] |= 1 << bit_num;

        true
    }

    pub fn test_and_clear(&mut self, index: u64) -> bool {
        let byte_num = (index >> BYTE_SHIFT) as usize;
        let bit_num = (index % BYTE_SIZE) as usize;

        if !(self.backing[byte_num] & (1 << bit_num)) > 0 {
            return false;
        }

        let and_val: u8 = 0xff ^ (1 << bit_num);
        self.backing[byte_num] &= and_val;

        true
    }

    pub fn insert_meta_files(&mut self, log_len: u64, alloc_sum: &mut u64) -> u64 {
        self.set_extent(0, FAMFS_SUPERBLOCK_SIZE + log_len, alloc_sum)
    }

    pub fn set_extent(&mut self, offset: u64, len: u64, alloc_sum: &mut u64) -> u64 {
        let page_num = offset / self.alloc_unit;
        let np = (len + self.alloc_unit - 1) / self.alloc_unit;
        let mut errors = 0;

        for k in page_num..(page_num + np) {
            let rc = self.test_and_set(k);
            if rc {
                *alloc_sum += self.alloc_unit;
            }
            else {
                errors+=1;
            }
        }

        errors
    }
    
    pub fn alloc_is_interleaved() -> bool {
        todo!()
    }

    /// 
    /// * `alloc_size` - The size to allocate in bytes
    /// * `cur_pos` -    Starting offset to search from
    /// * `range_size` - size (bytes) of range to allocate from (starting from `cur_pos`)
    ///                  (zero means alloc from the whole bitmap)
    ///                  (used for strided/striped allocations)
    /// 
    /// Returns the Some(offset) in bytes 
    /// Otherwise returns none if it fails to allocate
    pub fn alloc_contiguous(
        &mut self,
        alloc_size: u64, 
        cur_pos: &mut u64,
        range_size: u64
    ) -> Option<u64> {
        let alloc_bits = alloc_size.div_ceil(self.alloc_unit);
        let start_index = *cur_pos / self.alloc_unit;
        let range_size_bits = if range_size == 0 {self.len} else {range_size.div_ceil(self.alloc_unit)};

        'label: for i in start_index..self.len {
            if self.test(i) {continue}

            let rem = start_index + range_size_bits - i;

            if alloc_bits > rem {return None}

            for j in i..(i + alloc_bits) {
                if self.test(j) {continue 'label}
            }

            for j in i..(i + alloc_bits) {
                self.set(j);
            } 

            *cur_pos = (i + alloc_bits) * self.alloc_unit;

            return Some(i * self.alloc_unit);
        }
        
        None
    }

    pub fn free_contiguous(
        &mut self,
        offset: u64,
        len: u64
    ) {
        let start_bit = offset / self.alloc_unit;
        let nbits_free = len.div_ceil(self.alloc_unit);

        for i in start_bit..(start_bit + nbits_free) {
            assert!(self.test_and_clear(i));
        }
    }
}