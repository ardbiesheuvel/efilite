// SPDX-License-Identifier: GPL-2.0
// Copyright 2022 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

use crate::efi::{Guid, guid};

pub const EFI_DEVICE_PATH_PROTOCOL_GUID: Guid = guid!(
    0x9576e91, 0x6d3f, 0x11d2, [0x8e, 0x39, 0x0, 0xa0, 0xc9, 0x69, 0x72, 0x3b]
);

#[allow(non_camel_case_types)]
#[derive(PartialEq,Debug)]
#[repr(u8)]
pub enum DevicePathType {
    EFI_DEV_MEDIA = 4,
    EFI_DEV_END_PATH = 0x7f,
}

#[allow(non_camel_case_types)]
#[derive(PartialEq,Debug)]
#[repr(u8)]
pub enum DevicePathSubtype {
    EFI_DEV_MEDIA_VENDOR = 3,
    EFI_DEV_END_ENTIRE = 0xff,
}

#[derive(PartialEq,Debug)]
#[repr(C)]
pub struct DevicePath {
    pub _type: DevicePathType,
    pub subtype: DevicePathSubtype,
    pub size: u16,
}

#[repr(C)]
pub struct VendorMedia {
    pub header: DevicePath,
    pub vendor_guid: Guid,
}

// Check whether path has pfx as its prefix
// If so, return the number of bytes matched
pub fn is_prefix(pfx: &DevicePath, path: &DevicePath) -> isize {
    let mut ret = 0;
    let mut l = pfx;
    let mut r = path;

    while *l == *r {
        if l._type == DevicePathType::EFI_DEV_END_PATH ||
            l.size != r.size {
            break;
        }

        let p1 = l as *const _ as *const u8;
        let p2 = r as *const _ as *const u8;
        let s = l.size as isize;
        let (s1, s2) = unsafe {(
                core::slice::from_raw_parts(p1, s as usize),
                core::slice::from_raw_parts(p2, s as usize)
        )};
        if s1 != s2 {
            return 0;
        }

        // Advance to the next node
        l = unsafe { &*(p1.offset(s) as *const DevicePath) };
        r = unsafe { &*(p2.offset(s) as *const DevicePath) };
        ret += s;
    }

    // Return a positive number iff we matched the prefix
    // until the end node
    if l._type == DevicePathType::EFI_DEV_END_PATH {
        ret
    } else {
        0
    }
}
