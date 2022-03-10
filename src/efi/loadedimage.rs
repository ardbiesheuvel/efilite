// SPDX-License-Identifier: GPL-2.0
// Copyright 2022 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

use crate::efi::guid;
use crate::efi::{Guid, Handle, status::*, memorytype::*, systemtable::*};
use crate::efi::new_handle;
use crate::efi::install_protocol;
use crate::efi::uninstall_protocol;

use core::ptr;
use core::marker::PhantomData;

pub const EFI_LOADED_IMAGE_PROTOCOL_GUID: Guid = guid!(
    0x5B1B31A1, 0x9562, 0x11d2, [0x8E, 0x3F, 0x00, 0xA0, 0xC9, 0x69, 0x72, 0x3B]
);

pub const LINUX_EFI_LOADED_IMAGE_RAND_GUID: Guid = guid!(
    0xf5a37b6d, 0x3344, 0x42a5, [0xb6, 0xbb, 0x97, 0x86, 0x48, 0xc1, 0x89, 0x0a]
);

const EFI_LOADED_IMAGE_PROTOCOL_REVISION: u32 = 0x1000;

type ImageUnload = extern "C" fn(Handle) -> Status;

extern "C" fn unload(_handle: Handle) -> Status {
    Status::EFI_UNSUPPORTED
}

#[repr(C)]
pub struct LoadedImage<'a> {
    revision: u32,
    parent_handle: Handle,
    system_table: *const SystemTable,
    device_handle: Handle,
    file_path: *const (), //DevicePath,
    pub reserved: usize,
    load_options_size: u32,
    load_options: *const (),
    image_base: *const (),
    image_size: u64,
    image_code_type: MemoryType,
    image_data_type: MemoryType,
    unload: ImageUnload,

    // Private fields
    image_handle: Handle,
    entrypoint: *const u8,
    randomized: bool,
    marker: PhantomData<&'a ()>
}

impl<'a> LoadedImage<'a> {
    pub fn new(
        buffer: &'a[u8],
        load_options: &'a[u16],
        code_type: MemoryType,
        data_type: MemoryType,
        entrypoint: *const u8,
        randomized: bool,
    ) -> LoadedImage<'a> {
        let handle: Handle = new_handle();
        let li = LoadedImage {
            revision: EFI_LOADED_IMAGE_PROTOCOL_REVISION,
            parent_handle: 0,
            system_table: &*SystemTable::get(),
            device_handle: 0,
            file_path: ptr::null(),
            reserved: usize::MAX,
            load_options_size: (load_options.len() * core::mem::size_of::<u16>()) as u32,
            load_options: load_options.as_ptr() as *const (),
            image_base: buffer.as_ptr() as *const (),
            image_size: buffer.len() as u64,
            image_code_type: code_type,
            image_data_type: data_type,
            unload: unload,
            image_handle: handle,
            entrypoint: entrypoint,
            randomized: randomized,
            marker: PhantomData
        };

        install_protocol(handle, &EFI_LOADED_IMAGE_PROTOCOL_GUID, &li);
        if randomized {
            install_protocol(handle, &LINUX_EFI_LOADED_IMAGE_RAND_GUID, &li);
        }
        li
    }
}

impl Drop for LoadedImage<'_> {
    fn drop(&mut self) {
        uninstall_protocol(self.image_handle, &EFI_LOADED_IMAGE_PROTOCOL_GUID, &self);
        if self.randomized {
            uninstall_protocol(self.image_handle, &LINUX_EFI_LOADED_IMAGE_RAND_GUID, &self);
        }
    }
}

impl LoadedImage<'_> {
    pub fn start_image(&mut self) -> Status {
        unsafe {
            start_image(self.image_handle,
                        &*SystemTable::get(),
                        self.entrypoint as _,
                        &mut self.reserved as *mut _)
        }
    }
}

extern "C" {
    fn start_image(
        image_handle: Handle,
        system_table: *const SystemTable,
        entrypoint: *const (),
        sp_buffer: *mut usize
    ) -> Status;

    pub fn exit_image(
        status: Status,
        sp: usize,
    ) -> !;
}

core::arch::global_asm!(include_str!("start_image.s"));
