// SPDX-License-Identifier: GPL-2.0
// Copyright 2022 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

use crate::efi::{systemtable::*, loadedimage::*, memorytype::*};
use crate::efi::MemoryType::{EfiBootServicesCode, EfiBootServicesData};
use crate::efi::loadedimage::EFI_LOADED_IMAGE_PROTOCOL_GUID;
use crate::efi::configtable::ConfigurationTable;
use crate::DTB;

use crate::pecoff::Parser;

use core::sync::atomic::{AtomicUsize, Ordering};
use core::mem::MaybeUninit;

extern crate alloc;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use linked_list_allocator::LockedHeap;

const UEFI_REVISION: u32 = (2 << 16) | 90; // 2.90

pub mod bootservices;
mod configtable;
mod devicepath;
pub mod initrdloadfile2;
mod loadedimage;
pub mod memmap;
pub mod memorytype;
mod runtimeservices;
mod simpletext;
pub mod status;
mod systemtable;
mod tableheader;
pub mod rng;

pub type Bool = u8;
pub type Char16 = u16;
pub type PhysicalAddress = u64;
pub type VirtualAddress = u64;
pub type Handle = usize;
pub type Tpl = usize;
pub type Event = *mut ();
pub type EventNotify = extern "C" fn(Event, *const ());

pub fn new_handle() -> usize {
    static COUNTER: AtomicUsize = AtomicUsize::new(1);
    COUNTER.fetch_add(1, Ordering::AcqRel)
}

const TPL_APPLICATION: Tpl = 4;
#[allow(dead_code)]
const TPL_CALLBACK: Tpl = 8;
#[allow(dead_code)]
const TPL_NOTIFY: Tpl = 16;
#[allow(dead_code)]
const TPL_HIGH_LEVEL: Tpl = 31;

#[derive(PartialEq,PartialOrd,Eq,Ord,Clone,Copy)]
#[repr(C)]
pub struct Guid {
    data1: u32,
    data2: u16,
    data3: u16,
    data4: [u8; 8],
}

macro_rules! guid {
    ($a:literal, $b:literal, $c: literal, $d:expr) => {
        Guid { data1: $a, data2: $b, data3: $c, data4: $d, }
    };
}

pub const DTB_GUID: Guid = guid!(
    0xb1b621d5, 0xf19c, 0x41a5, [0x83, 0x0b, 0xd9, 0x15, 0x2c, 0x69, 0xaa, 0xe0]
);

pub const EFI_RT_PROPERTIES_TABLE_GUID: Guid = guid!(
    0xeb66918a, 0x7eef, 0x402a, [0x84, 0x2e, 0x93, 0x1d, 0x21, 0xc3, 0x8a, 0xe9]
);

const EFI_RT_SUPPORTED_GET_TIME: u32                   = 0x0001;
const EFI_RT_SUPPORTED_GET_VARIABLE: u32               = 0x0010;
const EFI_RT_SUPPORTED_GET_NEXT_VARIABLE_NAME: u32     = 0x0020;
const EFI_RT_SUPPORTED_RESET_SYSTEM: u32               = 0x0400;

#[repr(C)]
struct RtPropertiesTable {
    version: u16,
    length: u16,
    supported_mask: u32,
}

static RT_PROPERTIES_TABLE: RtPropertiesTable = RtPropertiesTable {
    version: 1,
    length: core::mem::size_of::<RtPropertiesTable>() as _,
    supported_mask:
        EFI_RT_SUPPORTED_GET_TIME |
        EFI_RT_SUPPORTED_GET_VARIABLE |
        EFI_RT_SUPPORTED_GET_NEXT_VARIABLE_NAME |
        EFI_RT_SUPPORTED_RESET_SYSTEM,
};

pub(crate) use guid;

type ProtocolDb = BTreeMap::<(Handle, Guid), *const ()>;

static mut PROTOCOL_DB: ProtocolDb = BTreeMap::new();
static mut CONFIGTABLE_DB: BTreeMap::<Guid, ConfigurationTable> = BTreeMap::new();

static RTSDATA_ALLOCATOR: LockedHeap = LockedHeap::empty();

pub fn init() {
    unsafe {
        extern "C" {
            static mut RTSPOOL: [MaybeUninit<u8>; 0xf000];
        }
        RTSDATA_ALLOCATOR.lock().init_from_slice(&mut RTSPOOL);
    }
    install_configtable(&EFI_RT_PROPERTIES_TABLE_GUID,
                        &RT_PROPERTIES_TABLE as *const _ as *const ());
    install_configtable(&DTB_GUID,
                        unsafe { DTB.as_ptr() } as *const ());
}

pub fn install_protocol<T>(
    handle: Handle,
    guid: &'static Guid,
    protocol: &T
) {
    let db = unsafe { &mut PROTOCOL_DB };
    db.insert((handle, *guid), protocol as *const _ as *const());
}

pub fn uninstall_protocol<T>(
    handle: Handle,
    guid: &'static Guid,
    _protocol: &T
) {
    let db = unsafe { &mut PROTOCOL_DB };
    db.remove(&(handle, *guid));
}

pub fn install_configtable(
    guid: &Guid,
    table: *const ()
) {
    let db = unsafe { &mut CONFIGTABLE_DB };
    let entry = ConfigurationTable {
        vendor_guid: *guid,
        vendor_table: table,
    };
    db.insert(*guid, entry);

    let array: Vec<ConfigurationTable> = db.values().cloned().collect();
    SystemTable::update_config_table_array(array.as_slice());
}

pub fn load_image<'a>(
    pe_image: &'a Parser,
    cmdline: &'a Vec<u16>,
    randomized: bool
) -> LoadedImage<'a> {
    LoadedImage::new(
        pe_image.get_image(),
        cmdline.as_slice(),
        EfiBootServicesCode,
        EfiBootServicesData,
        pe_image.get_entrypoint(),
        randomized)
}
