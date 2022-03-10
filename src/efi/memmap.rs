// SPDX-License-Identifier: GPL-2.0
// Copyright 2022 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

use crate::efi::memorytype::*;
use crate::efi::MemoryType::{EfiConventionalMemory, EfiRuntimeServicesCode, EfiRuntimeServicesData};
use crate::efi::PhysicalAddress;
use crate::Placement::*;

use alloc::vec::Vec;
use alloc::collections::BTreeMap;
use core::sync::atomic::{AtomicUsize, Ordering};
use core::slice;
use core::ops::Range;

static mut MEMMAP: BTreeMap::<PhysicalAddress, MemoryDescriptor> = BTreeMap::new();

static MAP_KEY: AtomicUsize = AtomicUsize::new(1);

pub const EFI_PAGE_SHIFT: usize = 12;
pub const EFI_PAGE_MASK: usize = (1 << EFI_PAGE_SHIFT) - 1;

const EFI_MEMORY_WB: u64 = 0x8;
const EFI_MEMORY_RUNTIME: u64 = 0x8000_0000_0000_0000;

fn inc_map_key() {
    MAP_KEY.fetch_add(1, Ordering::Release);
}

fn declare_region(phys: u64, num_pages: u64, _type: MemoryType, attr: u64) {
    let mm = unsafe { &mut MEMMAP };
    let attr = attr | EFI_MEMORY_WB;

    // Check whether the created/updated entry ends right where an
    // entry of the same type starts. If so, remove it and add its
    // page count to the new entry.
    let num_pages = {
        let mut l = num_pages;
        if let Some(next) = mm
            .drain_filter(|p, d|
                    *p == phys + (num_pages << EFI_PAGE_SHIFT) &&
                    d.r#type == _type && d.attribute == attr)
            .nth(0) {
            l += next.1.number_of_pages;
        }
        l
    };

    // Check if an entry exists with the same type and attributes
    // that ends right where this one starts. If so, update it to
    // cover the newly declared region instead of creating a new
    // entry.
    if let Some(mut desc) = mm.values_mut()
        .find(|d| d.physical_start +
                      (d.number_of_pages << EFI_PAGE_SHIFT) == phys &&
                  d.r#type == _type && d.attribute == attr) {
        desc.number_of_pages += num_pages;
    } else {
        mm.insert(phys,
                  MemoryDescriptor {
                      r#type: _type,
                      physical_start: phys,
                      virtual_start: 0,
                      number_of_pages: num_pages,
                      attribute: attr,
                  });
    }
    inc_map_key();
}

pub fn declare_memory_region(region: &[u8]) {
    let phys = region.as_ptr() as PhysicalAddress;
    let pages = region.len() as u64 >> EFI_PAGE_SHIFT;
    declare_region(phys, pages, EfiConventionalMemory, 0);
}

pub fn declare_runtime_region(region: &[u8]) {
    let phys = region.as_ptr() as PhysicalAddress;
    let pages = region.len() as u64 >> EFI_PAGE_SHIFT;
    declare_region(phys, pages, EfiRuntimeServicesCode, EFI_MEMORY_RUNTIME);
}

pub fn size_to_pages(size: usize) -> usize {
    (size + EFI_PAGE_MASK) >> EFI_PAGE_SHIFT
}

fn split_region(phys: PhysicalAddress, size: usize, _type: Option<MemoryType>) {
    let mm = unsafe { &mut MEMMAP };
    if let Some(mut desc) = mm.values_mut()
        .find(|d| d.r#type == _type.unwrap_or(d.r#type) &&
                  d.physical_start < phys &&
                  d.physical_start + (d.number_of_pages << EFI_PAGE_SHIFT)
                            >= phys + size as u64) {

        let num_pages = (phys - desc.physical_start) >> EFI_PAGE_SHIFT;
        let mm = unsafe { &mut MEMMAP };

        mm.insert(phys,
                  MemoryDescriptor {
                      r#type: desc.r#type,
                      physical_start: phys,
                      virtual_start: 0,
                      number_of_pages: desc.number_of_pages - num_pages,
                      attribute: desc.attribute,
                  });
        desc.number_of_pages = num_pages;
        inc_map_key();
    }
}

fn convert_region(region: &[u8], from: Option<MemoryType>, to: MemoryType) -> Result<(),()> {
    let mm = unsafe { &mut MEMMAP };
    let phys = region.as_ptr() as PhysicalAddress;
    let pages = region.len() as u64 >> EFI_PAGE_SHIFT;
    let attr =
        if to == EfiRuntimeServicesCode || to == EfiRuntimeServicesData {
            EFI_MEMORY_RUNTIME
        } else {
            0
        };

    // If the start address does not appear in the map yet, find the
    // entry that covers the range and split it in two.
    if !mm.contains_key(&phys) {
        split_region(phys, region.len(), from);
    }

    // Take the entry that starts at the right address
    if let Some(mut desc) = mm.remove(&phys) {
        // If such an entry exists, check whether it is of the
        // expected size and type. If not, put it back into the
        // map and return an error.
        if desc.r#type != from.unwrap_or(desc.r#type) ||
            pages > desc.number_of_pages {
            mm.insert(desc.physical_start, desc);
            return Err(());
        }

        // Shrink the entry and increase its start address
        // accordingly. If it ends up empty, drop it.
        desc.number_of_pages -= pages;
        desc.physical_start += region.len() as u64;
        if desc.number_of_pages > 0 {
            mm.insert(desc.physical_start, desc);
        }

        // Create a new entry for the freed up region
        declare_region(phys, pages, to, attr);
        Ok(())
    } else {
        Err(())
    }
}

pub fn allocate_region(region: &[u8], _type: MemoryType) -> Result<(),()> {
    convert_region(region, Some(EfiConventionalMemory), _type)
}

pub fn free_pages(base: u64, pages: usize) -> Result<(),()> {
    let region = unsafe {
        slice::from_raw_parts_mut(base as *mut u8, pages << EFI_PAGE_SHIFT)
    };
    convert_region(region, None, EfiConventionalMemory)
}

pub fn allocate_pages_fixed(
    base: u64,
    pages: usize,
    _type: MemoryType
) -> Option<&'static mut [u8]> {
    let region = unsafe {
        slice::from_raw_parts_mut(base as *mut u8, pages << EFI_PAGE_SHIFT)
    };
    if let Ok(_) = allocate_region(region, _type) {
        Some(region)
    } else {
        None
    }
}

pub enum Placement {
    Max(u64),
    Fixed(u64),
    Anywhere,

    Random(u32, u64),
    Aligned(u64),
    MaxAlignMask(u64, u64),
}

pub fn allocate_pages(
    pages: usize,
    _type: MemoryType,
    placement: Placement
) -> Option<&'static mut [u8]> {
    let mm = unsafe { &mut MEMMAP };
    let p = pages as u64;

    // Narrow down the placement
    let placement = match placement {
        Max(max) => MaxAlignMask(max, EFI_PAGE_MASK as u64),
        Anywhere => MaxAlignMask(u64::MAX, EFI_PAGE_MASK as u64),
        Aligned(align) => MaxAlignMask(u64::MAX, align - 1),
        pl => pl,
    };

    let base = match placement {

        // Look for the descriptor that is the highest up in memory
        // that covers a sufficient number of pages below 'max' from
        // its started address aligned up to the requested alignment
        MaxAlignMask(max, mask) => if let Some(desc) = mm.values()
            .take_while(|d| ((d.physical_start - 1) | mask) + (p << EFI_PAGE_SHIFT) <= max)
            .filter(|d| {
               let num_pages = p + (mask - ((d.physical_start - 1) & mask) >> EFI_PAGE_SHIFT);
               d.r#type == EfiConventionalMemory && d.number_of_pages >= num_pages
            })
            .last() {

            // Find the highest possible base resulting from the limit in 'max'
            let highest_base = max - (p << EFI_PAGE_SHIFT) + 1;

            // Allocate from the top down
            let offset = (desc.number_of_pages - p) << EFI_PAGE_SHIFT;
            core::cmp::min(desc.physical_start + offset, highest_base) & !mask as u64
        } else {
            return None;
        },

        Placement::Random(seed, align) => {
            let mask = align - 1;

            // Get a list of (Range<u64>, descriptor) tuples describing all regions
            // that the randomized allocation may be served from.
            let mut slots: u64 = 0;
            let descs: Vec<(Range<u64>, &MemoryDescriptor)> = mm.values()
                .filter_map(|d| {
                    // Include the number of pages lost to alignment in the page count
                    let num_pages = p + (mask - ((d.physical_start - 1) & mask) >> EFI_PAGE_SHIFT);
                    if d.r#type == EfiConventionalMemory && d.number_of_pages >= num_pages {
                        let sl = 1 + ((d.number_of_pages - num_pages) << EFI_PAGE_SHIFT) / align;
                        let end = slots + sl;
                        let r = slots..end;
                        slots = end;
                        Some((r, d))
                    } else {
                        None
                    }
                })
                .collect();

            // Use the seed to generate a random index into the slot list
            let index = (slots * seed as u64) >> 32;
            if let Some(entry) = descs.into_iter()
                .find(|e: &(Range<u64>, &MemoryDescriptor)| e.0.contains(&index)) {
                let offset = (index - entry.0.start) * align;
                ((entry.1.physical_start - 1) | mask) + 1 + offset
            } else {
                return None;
            }
        },

        Placement::Fixed(base) => base,

        _ => {
            return None; // unreachable
        }

    };
    allocate_pages_fixed(base, pages, _type)
}

pub fn copy_to_slice(tbl: &mut [MemoryDescriptor]) -> usize {
    let mm = unsafe { &mut MEMMAP };
    let key = MAP_KEY.load(Ordering::Acquire);
    let vec = mm.values().cloned().collect::<Vec<_>>();
    tbl.copy_from_slice(vec.as_slice());
    key
}

pub fn len() -> usize {
    unsafe { MEMMAP.len() }
}

pub fn key() -> usize {
    MAP_KEY.load(Ordering::Relaxed)
}
