// SPDX-License-Identifier: GPL-2.0
// Copyright 2022 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

use core::arch::asm;
use mmio::{Allow, Deny, VolBox};

#[repr(C, align(8))]
pub struct TableHeader {
    pub signature: [u8; 8],
    pub revision: u32,
    pub header_size: u32,
    pub crc32: u32,
    pub reserved: u32,
}

impl TableHeader {
    pub fn update_crc(&mut self) {
        let mut crc_field = unsafe {
            VolBox::<u32, Deny, Allow>::new(&self.crc32 as *const _ as *mut u32)
        };
        crc_field.write(0);

        let header: *const () = self as *const _ as _;
        let header_size: isize = self.header_size as _;
        let mut offset: isize = 0;
        let mut crc32: u32 = 0;

        while offset < header_size {
            let rem = header_size - offset;
            match rem {
                1 => unsafe {
                    asm!("crc32b {crc:w}, {crc:w}, {inp:w}",
                         crc = inout(reg) crc32,
                         inp = in(reg) *(header.offset(offset) as *const u8),
                         options(nomem, nostack, preserves_flags),
                         );
                    offset += 1;
                },
                2..=3 => unsafe {
                    asm!("crc32h {crc:w}, {crc:w}, {inp:w}",
                         crc = inout(reg) crc32,
                         inp = in(reg) *(header.offset(offset) as *const u16),
                         options(nomem, nostack, preserves_flags),
                         );
                    offset += 2;
                },
                4..=7 => unsafe {
                    asm!("crc32w {crc:w}, {crc:w}, {inp:w}",
                         crc = inout(reg) crc32,
                         inp = in(reg) *(header.offset(offset) as *const u32),
                         options(nomem, nostack, preserves_flags),
                         );
                    offset += 4;
                },
                _ => unsafe {
                    asm!("crc32x {crc:w}, {crc:w}, {inp}",
                         crc = inout(reg) crc32,
                         inp = in(reg) *(header.offset(offset) as *const u64),
                         options(nomem, nostack, preserves_flags),
                         );
                    offset += 8;
                },
            }
        }
        crc_field.write(crc32);
    }
}
