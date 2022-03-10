// SPDX-License-Identifier: GPL-2.0
// Copyright 2022 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

use core::arch::asm;

const CTR_IDC: u64 = 1 << 28;

const CTR_DMINLINE_SHIFT: u64 = 16;
const CTR_DMINLINE_MASK: u64 = 0xf;

pub fn dcache_clean_to_pou(slice: &[u8]) {
    let ctr = unsafe {
        let mut l: u64;
        asm!("mrs {reg}, ctr_el0", // CTR: cache type register
            reg = out(reg) l,
            options(pure, nomem, nostack, preserves_flags),
        );
        l
    };

    // Perform the clean only if needed for coherency with the I side
    if (ctr & CTR_IDC) == 0 {
        let line_shift = 2 + ((ctr >> CTR_DMINLINE_SHIFT) & CTR_DMINLINE_MASK);
        let line_size: usize = 1 << line_shift;
        let num_lines = (slice.len() + line_size - 1) >> line_shift;
        let mut offset: usize = 0;

        for _ in 0..num_lines {
            unsafe {
                let base = slice.as_ptr();
                asm!("dc cvau, {reg}",
                    reg = in(reg) base.offset(offset as isize),
                    options(nomem, nostack, preserves_flags),
                );
            }
            offset += line_size;
        }
    }
}
