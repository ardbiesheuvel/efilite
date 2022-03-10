// SPDX-License-Identifier: GPL-2.0
// Copyright 2022 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

use crate::efi::{Bool, status::*};
use crate::efi::devicepath::{VendorMedia, DevicePath};
use crate::efi::{guid, Guid, Handle, new_handle, install_protocol, uninstall_protocol};
use crate::efi::devicepath::EFI_DEVICE_PATH_PROTOCOL_GUID;
use crate::efi::devicepath::{DevicePathType::*, DevicePathSubtype::*};

use crate::initrd::InitrdLoader;

pub const EFI_LOAD_FILE2_PROTOCOL_GUID: Guid = guid!(
    0x4006c0c1, 0xfcb3, 0x403e, [0x99, 0x6d, 0x4a, 0x6c, 0x87, 0x24, 0xe0, 0x6d]
);

type LoadFile =
    extern "C" fn(*mut LoadFile2, *const DevicePath, Bool, *mut usize, *mut ()) -> Status;

#[repr(C)]
pub struct LoadFile2<'a> {
    load_file: LoadFile,

    loader: &'a mut dyn InitrdLoader,
    handle: Handle,
}

pub fn new<'a>(loader: &'a mut dyn InitrdLoader) -> LoadFile2<'a> {
    let lf = LoadFile2 {
        load_file: load_file,
        loader: loader,
        handle: new_handle(),
    };
    install_protocol(lf.handle, &EFI_LOAD_FILE2_PROTOCOL_GUID, &lf);
    install_protocol(lf.handle, &EFI_DEVICE_PATH_PROTOCOL_GUID, &INITRD_DEV_PATH);
    lf
}

impl<'a> Drop for LoadFile2<'a> {
    fn drop(&mut self) {
        uninstall_protocol(self.handle, &EFI_LOAD_FILE2_PROTOCOL_GUID, &self);
        uninstall_protocol(self.handle, &EFI_DEVICE_PATH_PROTOCOL_GUID, &INITRD_DEV_PATH);
    }
}

#[repr(C)]
struct InitrdDevicePath {
    vendor: VendorMedia,
    end: DevicePath,
}
const LINUX_EFI_INITRD_MEDIA_GUID: Guid = guid!(
    0x5568e427, 0x68fc, 0x4f3d, [0xac, 0x74, 0xca, 0x55, 0x52, 0x31, 0xcc, 0x68]
);

static INITRD_DEV_PATH: InitrdDevicePath = InitrdDevicePath {
    vendor: VendorMedia {
        header: DevicePath {
            _type: EFI_DEV_MEDIA,
            subtype: EFI_DEV_MEDIA_VENDOR,
            size: core::mem::size_of::<VendorMedia>() as u16,
        },
        vendor_guid: LINUX_EFI_INITRD_MEDIA_GUID,
    },
    end: DevicePath {
        _type: EFI_DEV_END_PATH,
        subtype: EFI_DEV_END_ENTIRE,
        size: core::mem::size_of::<DevicePath>() as u16,
    },
};

extern "C" fn load_file(
    this: *mut LoadFile2,
    _file_path: *const DevicePath,
    _boot_policy: Bool,
    buffer_size: *mut usize,
    buffer: *mut (),
) -> Status {
    let this = unsafe { &mut *this };
    let bufsize = unsafe { *buffer_size };
    let initrdsize = this.loader.get_size();
    if bufsize < initrdsize || buffer.is_null() {
        unsafe { *buffer_size = initrdsize };
        return Status::EFI_BUFFER_TOO_SMALL;
    }

    let region = unsafe {
        core::slice::from_raw_parts_mut(buffer as *mut u8, initrdsize)
    };

    if let Ok(_) = this.loader.load_initrd_image(region) {
        Status::EFI_SUCCESS
    } else {
        Status::EFI_DEVICE_ERROR
    }
}
