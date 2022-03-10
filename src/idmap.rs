// SPDX-License-Identifier: GPL-2.0
// Copyright 2022 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

use crate::paging::*;

use core::arch::asm;

// Use a different ASID for the full ID map, as well as non-global attributes
// for all its DRAM mappings. This way, we can ignore break-before-make rules
// entirely when breaking down block mappings, as long as we don't do so while
// the full ID map is active.
const ASID: u64 = 1;

// We use 4k pages with a VA range of 39-bits. This gives us a VA space of 512 GB,
// which is plenty for our identity mapping, and allow us to start at level 1.
const ROOT_LEVEL: usize = 1;

extern "C" {
    // Root level of the initial ID map in NOR flash
    static idmap: PageTable;
}

pub struct IdMap {
    root: RootTable,
}

impl IdMap {
    pub fn new() -> IdMap {
        let mut id = IdMap {
            root: RootTable::new(ROOT_LEVEL)
        };
        unsafe { // accessing static externs is unsafe
            id.root.clone_raw_entry(&idmap, 0);
        }
        id
    }

    pub fn activate(&self) {
        unsafe { // inline asm is unsafe
            asm!(
                "msr   ttbr0_el1, {ttbrval}",
                "isb",
                ttbrval = in(reg) self.root.as_ptr() as u64 | (ASID << 48),
                options(preserves_flags),
            );
        }
    }

    pub fn deactivate(&self) {
        unsafe { // inline asm is unsafe
            asm!(
                "msr   ttbr0_el1, {ttbrval}",
                "isb",
                "tlbi  aside1, {asid}",
                "dsb   nsh",
                "isb",
                asid = in(reg) ASID << 48,
                ttbrval = in(reg) &idmap as *const _ as u64,
                options(preserves_flags),
            );
        }
    }

    pub fn map_range(&mut self, slice: &[u8], flags: Attributes) {
        let start = slice.as_ptr() as usize;
        let base = align_down!(start, PAGE_SIZE);
        let size = align_up!(start - base + slice.len(), PAGE_SIZE);

        log::info!(
            "Mapping memory at [0x{:X} - 0x{:X}] {:?}\n",
            base,
            base + size - 1,
            flags
        );
        self.root.map_range(&Chunk(base..base + size), base as u64, flags);
    }
}

impl Drop for IdMap {
    fn drop(&mut self) {
        self.deactivate();
    }
}
