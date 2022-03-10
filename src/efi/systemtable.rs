// SPDX-License-Identifier: GPL-2.0
// Copyright 2022 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

use crate::efi::{Char16, Handle, configtable::*, tableheader::*, simpletext::*, runtimeservices::*, bootservices::*};
use crate::efi::UEFI_REVISION;
use crate::efi::new_handle;
use crate::efi::RTSDATA_ALLOCATOR;

use core::{ptr, slice};
use core::ptr::NonNull;
use const_utf16::encode_null_terminated;

#[repr(C)]
pub struct SystemTable {
    hdr: TableHeader,
    firmware_vendor: *const Char16,
    firmware_revision: u32,
    console_in_handle: Handle,
    con_in: *const SimpleTextInput,
    console_out_handle: Handle,
    con_out: *const SimpleTextOutput,
    standard_error_handle: Handle,
    stderr: *const SimpleTextOutput,
    runtime_services: *const RuntimeServices,
    boot_services: *const BootServices,
    number_of_table_entries: usize,
    configuration_table: *const ConfigurationTable,
}

#[link_section = ".rtsdata"]
static mut ST: SystemTable = SystemTable {
    hdr: TableHeader {
        signature: [b'I', b'B', b'I', b' ', b'S', b'Y', b'S', b'T'],
        revision: UEFI_REVISION,
        header_size: core::mem::size_of::<SystemTable>() as u32,
        crc32: 0,
        reserved: 0,
    },
    firmware_vendor: encode_null_terminated!("Google").as_ptr(),
    firmware_revision: UEFI_REVISION,
    console_in_handle: 0,
    con_in: ptr::null(),
    console_out_handle: 0,
    con_out: ptr::null(),
    standard_error_handle: 0,
    stderr: ptr::null(),
    runtime_services: ptr::null(),
    boot_services: ptr::null(),
    number_of_table_entries: 0,
    configuration_table: ptr::null(),
};

impl SystemTable {
    pub fn get() -> &'static SystemTable {
        unsafe {
            if ST.console_out_handle == 0 {
                let handle = new_handle();
                let inp = SimpleTextInput::get(handle);
                let out = SimpleTextOutput::get(handle);

                ST.console_in_handle = handle;
                ST.console_out_handle = handle;
                ST.standard_error_handle = handle;

                ST.con_in = &*inp;
                ST.con_out = &*out;
                ST.stderr = &*out;

                ST.runtime_services = &*RuntimeServices::get();
                ST.boot_services = &*BootServices::get();
            }

            if ST.hdr.crc32 == 0 {
                ST.hdr.update_crc();
            }
            &ST
        }
    }

    pub fn update_config_table_array(new: &[ConfigurationTable]) {
        let mut alloc = RTSDATA_ALLOCATOR.lock();
        let st = unsafe { &mut ST };

        let entsize = core::mem::size_of::<ConfigurationTable>();
        if st.number_of_table_entries != 0 {
            // drop the old array first
            let size = st.number_of_table_entries * new.len();
            let layout = core::alloc::Layout::from_size_align(size, 8).unwrap();
            unsafe {
                alloc.deallocate(NonNull::new_unchecked(st.configuration_table as *mut u8), layout);
            }
        }

        let size = entsize * new.len();
        let layout = core::alloc::Layout::from_size_align(size, 8).unwrap();
        let ptr = alloc.allocate_first_fit(layout).ok().unwrap().as_ptr();
        // TODO check ptr
        let tbl = unsafe { slice::from_raw_parts_mut(ptr as *mut ConfigurationTable, new.len()) };
        tbl.copy_from_slice(new);

        st.configuration_table = ptr as _;
        st.number_of_table_entries = new.len();
        st.hdr.update_crc();
    }
}
