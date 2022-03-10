// SPDX-License-Identifier: GPL-2.0
// Copyright 2022 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

use core::ops::Range;
use alloc::boxed::Box;
use crate::pagealloc;

pub const PAGE_SHIFT: usize = 12;
pub const PAGE_SIZE: usize = 1 << PAGE_SHIFT;

const BITS_PER_LEVEL: usize = PAGE_SHIFT - 3;

#[repr(C)]
pub struct RootTable {
    table: Box<PageTable>,
    level: usize,
}

impl RootTable {
    pub fn new(level: usize) -> RootTable {
        RootTable {
            table: unsafe {
                let p = pagealloc::get_zeroed_page();
                Box::<PageTable>::from_raw(p as *mut _)
            },
            level: level,
        }
    }

    pub fn clone_raw_entry(&mut self, other: &PageTable, index: usize) {
        self.table.entries[index] = other.entries[index]
    }

    pub fn map_range(&mut self, range: &Chunk, pa: u64, flags: Attributes) {
        self.table.map_range(range, pa, flags, self.level);
    }

    pub fn as_ptr(&self) -> *const () {
        self.table.as_ptr()
    }
}

pub struct Chunk(pub Range<usize>);

struct ChunkIterator<'a> {
    range: &'a Chunk,
    granularity: usize,
    start: usize,
}

impl Iterator for ChunkIterator<'_> {
    type Item = Chunk;

    fn next(&mut self) -> Option<Chunk> {
        if !self.range.0.contains(&self.start) {
            return None
        }
        let end = self.range.0.end.min((self.start | (self.granularity - 1)) + 1);
        let c = Chunk(self.start..end);
        self.start = end;
        Some(c)
    }
}

impl Chunk {
    fn split(&self, level: usize) -> ChunkIterator {
        ChunkIterator {
            range: self,
            granularity: PAGE_SIZE << ((3 - level) * BITS_PER_LEVEL),
            start: self.0.start,
        }
    }

    // Whether this chunk covers an entire block mapping at this pagetable level
    fn is_block(&self, level: usize) -> bool {
        let gran = PAGE_SIZE << ((3 - level) * BITS_PER_LEVEL);
        (self.0.start | self.0.end) & (gran - 1) == 0
    }
}

bitflags! {
    pub struct Attributes: u64 {
        const VALID         = 1 << 0;
        const TABLE_OR_PAGE = 1 << 1;

        const DEVICE_NGNRE  = 0 << 2;
        const NORMAL        = 1 << 2 | 3 << 8; // inner shareable

        const READ_ONLY     = 1 << 7;
        const ACCESSED      = 1 << 10;
        const NON_GLOBAL    = 1 << 11;
        const EXECUTE_NEVER = 3 << 53;
    }
}

#[allow(dead_code)]
impl Attributes {
    pub fn read_only(mut self) -> Self {
        self.insert(Attributes::READ_ONLY);
        self
    }

    pub fn non_global(mut self) -> Self {
        self.insert(Attributes::NON_GLOBAL);
        self
    }

    pub fn execute_disable(mut self) -> Self {
        self.insert(Attributes::EXECUTE_NEVER);
        self
    }

    fn valid(mut self) -> Self {
        self.insert(Attributes::VALID);
        self
    }

    fn accessed(mut self) -> Self {
        self.insert(Attributes::ACCESSED);
        self
    }

    fn page(mut self) -> Self {
        self.insert(Attributes::TABLE_OR_PAGE);
        self
    }
}

#[repr(C, align(4096))]
pub struct PageTable {
    entries: [Descriptor; 1 << BITS_PER_LEVEL],
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Descriptor(u64);

impl Descriptor {
    fn output_address(&self) -> u64 {
        self.0 & (!(PAGE_SIZE - 1) & !(0xffff << 48)) as u64
    }

    fn flags(self) -> Attributes {
        Attributes {
            bits: self.0 & ((PAGE_SIZE - 1) | (0xffff << 48)) as u64
        }
    }

    fn is_valid(&self) -> bool {
        (self.0 & Attributes::VALID.bits()) != 0
    }

    fn is_table(&self) -> bool {
        return self.is_valid() &&
            (self.0 & Attributes::TABLE_OR_PAGE.bits()) != 0
    }

    fn set(&mut self, pa: u64, flags: Attributes) {
        self.0 = pa | flags.valid().bits();
    }

    fn subtable(&self) -> &mut PageTable {
        unsafe { &mut *(self.output_address() as *mut PageTable) }
    }
}

impl PageTable {
    pub fn as_ptr(&self) -> *const () {
        self as *const _ as *const ()
    }

    fn get_entry_mut(&mut self, va: usize, level: usize) -> &mut Descriptor {
        let shift = PAGE_SHIFT + (3 - level) * BITS_PER_LEVEL;
        let index = (va >> shift) % (1 << BITS_PER_LEVEL);
        &mut self.entries[index]
    }

    fn map_range(&mut self, range: &Chunk, pa: u64, flags: Attributes, level: usize) {
        assert!(level <= 3);
        let flags = if level == 3 { flags.page() } else { flags };
        let mut pa = pa;

        for chunk in range.split(level) {
            let entry = self.get_entry_mut(chunk.0.start, level);

            if level == 3 || (chunk.is_block(level) && !entry.is_table()) {
                // Rather than leak the entire subhierarchy, only put down
                // a block mapping if the region is not already covered by
                // a table mapping
                entry.set(pa, flags.accessed());
            } else {
                if !entry.is_table() {
                    let old = *entry;
                    let page = pagealloc::get_zeroed_page();
                    entry.set(page, Attributes::TABLE_OR_PAGE);
                    if old.is_valid() {
                        let gran = PAGE_SIZE << ((3 - level) * BITS_PER_LEVEL);
                        // Old was a valid block entry, so we need to split it
                        // Recreate the entire block in the newly added table
                        let a = align_down!(chunk.0.start, gran);
                        let b = align_up!(chunk.0.end, gran);
                        entry.subtable()
                            .map_range(&Chunk(a..b), old.output_address(), old.flags(), level + 1);
                    }
                }
                entry.subtable().map_range(&chunk, pa, flags, level + 1);
            }
            pa += chunk.0.len() as u64;
        }
    }
}
