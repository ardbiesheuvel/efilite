// SPDX-License-Identifier: GPL-2.0
// Copyright 2022 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

use core::alloc::GlobalAlloc;
use core::arch::asm;

use crate::paging::PAGE_SIZE;
use crate::ALLOCATOR;

const DCZID_BS_MASK: u64 = 0xf;

pub fn get_zeroed_page() -> u64 {
    let layout =
        core::alloc::Layout::from_size_align(PAGE_SIZE, PAGE_SIZE).unwrap();
    let page = unsafe { ALLOCATOR.alloc(layout) };
    if page.is_null() {
        panic!("Out of memory!");
    }

    let dczid = unsafe {
        let mut l: u64;
        asm!("mrs {reg}, dczid_el0",
             reg = out(reg) l,
             options(pure, nomem, nostack, preserves_flags),
        );
        l
    };

    let line_shift = 2 + (dczid & DCZID_BS_MASK);
    let line_size: isize = 1 << line_shift;
    let num_lines = PAGE_SIZE >> line_shift;
    let mut offset: isize = 0;

    for _ in 0..num_lines {
        unsafe {
            asm!(
                "dc zva, {line}",
                 line = in(reg) page.offset(offset),
                 options(nostack, preserves_flags),
            );
        }
        offset += line_size;
    }
    page as u64
}
