// SPDX-License-Identifier: GPL-2.0
// Copyright 2023 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

use aarch64_paging::{paging::*, *};
use efiloader::memorytype::*;

use alloc::vec::Vec;
use core::ops::Range;
use spinning_top::Spinlock;

const ASID: usize = 1;
const PAGING_ROOT_LEVEL: usize = 1; // must match the page tables in flash

pub(crate) struct MemoryMapper {
    idmap: Spinlock<idmap::IdMap>,
    reserved: Vec<Range<usize>>,
}

impl MemoryMapper {
    pub(crate) fn new() -> MemoryMapper {
        MemoryMapper {
            idmap: Spinlock::new(idmap::IdMap::new(ASID, PAGING_ROOT_LEVEL)),
            reserved: Vec::new(),
        }
    }

    pub(crate) fn activate(&mut self) {
        unsafe { self.idmap.lock().activate() }
    }

    fn match_efi_attributes(attributes: u64) -> Attributes {
        match attributes & (EFI_MEMORY_RO | EFI_MEMORY_XP) {
            0 => Attributes::empty(),
            EFI_MEMORY_RO => Attributes::READ_ONLY,
            EFI_MEMORY_XP => Attributes::EXECUTE_NEVER,
            _ => Attributes::EXECUTE_NEVER | Attributes::READ_ONLY,
        }
    }

    fn get_attr_from_flags(flags: usize) -> u64 {
        let mut ret: u64 = 0;
        if flags & Attributes::READ_ONLY.bits() != 0 {
            ret |= EFI_MEMORY_RO;
        }
        if flags & Attributes::EXECUTE_NEVER.bits() != 0 {
            ret |= EFI_MEMORY_XP;
        }
        ret
    }

    pub(crate) fn map_range(&self, r: &Range<usize>, flags: Attributes) {
        let mr = MemoryRegion::new(r.start, r.end);
        self.idmap
            .lock()
            .map_range(&mr, flags)
            .unwrap_or_else(|e| log::error!("Failed to map range {e}\n"));
        log::info!("[{mr}] {flags:?}\n");
    }

    pub(crate) fn map_reserved_range(&mut self, r: &Range<usize>, flags: Attributes) {
        self.reserved.push(r.start..r.end);
        self.map_range(r, flags)
    }

    fn range_is_reserved(&self, r: &Range<usize>) -> bool {
        for res in &self.reserved {
            if r.start < res.end && res.start < r.end {
                return true;
            }
        }
        false
    }
}

impl efiloader::MemoryMapper for MemoryMapper {
    fn remap_range(&self, range: &Range<usize>, set: u64, clr: u64) -> Result<(), &str> {
        let r = MemoryRegion::new(range.start, range.end);
        let set = Self::match_efi_attributes(set);
        let clr = Self::match_efi_attributes(clr);

        if self.range_is_reserved(range) {
            return Err("Cannot remap reserved range");
        }

        let c = |_: &MemoryRegion, d: &mut Descriptor, _: usize| Ok(d.modify_flags(set, clr));

        let mut idmap = self.idmap.lock();
        idmap
            .modify_range(&r, &c)
            .or_else(|e| match e {
                MapError::BreakBeforeMakeViolation(_) => {
                    // SAFETY: this code and the current stack are covered by the initial
                    // mapping in NOR flash so deactivating this mapping is safe
                    unsafe {
                        idmap.deactivate();
                    }

                    let e = idmap.modify_range(&r, &c);

                    // SAFETY: we are reactivating the ID mapping we deactivated just now
                    // and we double checked that the reserved regions were left untouched
                    unsafe {
                        idmap.activate();
                    }
                    e
                }
                _ => Err(e),
            })
            .or(Err("Failed to remap range"))
    }

    fn query_range(&self, range: &Range<usize>) -> Option<u64> {
        let r = MemoryRegion::new(range.start, range.end);
        let mask = Attributes::READ_ONLY | Attributes::EXECUTE_NEVER;
        let mut any = Attributes::empty();
        let mut all = mask;

        if self.range_is_reserved(range) {
            return None;
        }

        let mut c = |_: &MemoryRegion, d: &Descriptor, _: usize| {
            if d.is_valid() {
                d.flags().map_or(Err(()), |f| {
                    any |= f & mask;
                    all &= f;
                    if any != all {
                        return Err(());
                    };
                    Ok(())
                })
            } else {
                Err(())
            }
        };

        self.idmap.lock().walk_range(&r, &mut c).ok()?;
        Some(Self::get_attr_from_flags(any.bits()))
    }
}
