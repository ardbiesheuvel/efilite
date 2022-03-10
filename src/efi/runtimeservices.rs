// SPDX-License-Identifier: GPL-2.0
// Copyright 2022 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

use crate::efi::{memorytype::*, tableheader::*, status::*};
use crate::efi::{Bool, Char16, Guid, PhysicalAddress};
use crate::efi::UEFI_REVISION;
use crate::psci;

#[repr(C)]
struct Time {
     year: u16,
     month: u8,
     date: u8,
     hour: u8,
     minute: u8,
     second: u8,
     pad1: u8,
     nanosecond: u32,
     timezone: u16,
     daylight: u8,
     pad2: u8,
}

#[repr(C)]
struct TimeCapabilities {
     resolution: u32,
     accuracy: u32,
     sets_to_zero: Bool,
}

#[allow(dead_code)]
#[repr(C)]
enum ResetType {
     EfiResetCold,
     EfiResetWarm,
     EfiResetShutdown,
     EfiResetPlatformSpecific
}

#[repr(C)]
struct CapsuleHeader {
    capsule_guid: Guid,
    header_size: u32,
    flags: u32,
    capsule_image_size: u32,
}

type GetTime =
    extern "C" fn(
        _time: *mut Time,
        _capabilities: *mut TimeCapabilities
    ) -> Status;

type SetTime =
    extern "C" fn(
        _time: *const Time
    ) -> Status;

type GetWakeupTime =
    extern "C" fn(
        _enabled: *mut Bool,
        _pending: *mut Bool,
        _time: *mut Time
    ) -> Status;

type SetWakeupTime =
    extern "C" fn(
        _enable: Bool,
        _time: *const Time
    ) -> Status;

type SetVirtualAddressMap =
    extern "C" fn(
        _memory_map_size: usize,
        _descriptor_size: usize,
        _descriptor_version: u32,
        _virtual_map: *const MemoryDescriptor
    ) -> Status;

type ConvertPointer =
    extern "C" fn(
        _debug_disposition: usize,
        _address: *const *mut ()
    ) -> Status;

type GetVariable =
    extern "C" fn(
        _variable_name: *const Char16,
        _vendor_guid: *const Guid,
        _attributes: *mut u32,
        _data_size: *mut usize,
        _data: *mut ()
    ) -> Status;

type GetNextVariableName =
    extern "C" fn(
        _variable_name_size: *mut usize,
        _variable_name: *mut Char16,
        _vendor_guid: *mut Guid
    ) -> Status;

type SetVariable =
    extern "C" fn(
        _variable_name: *const Char16,
        _vendor_guid: *const Guid,
        _attributes: *const u32,
        _data_size: *const usize,
        _data: *const ()
    ) -> Status;

type GetNextHighMonotonicCount =
    extern "C" fn(
        _high_count: *mut u32
    ) -> Status;

type ResetSystem =
    extern "C" fn(
        _reset_type: ResetType,
        _reset_status: Status,
        _data_size: usize,
        _reset_data: *const ()
    ) -> Status;

type UpdateCapsule =
    extern "C" fn(
        _capsule_header_array: *const *const CapsuleHeader,
        _capsule_count: usize,
        _scatter_gather_list: PhysicalAddress,
    ) -> Status;

type QueryCapsuleCapabilities =
    extern "C" fn(
        _capsule_header_array: *const *const CapsuleHeader,
        _capsule_count: usize,
        _maximum_capsule_size: *mut u64,
        _reset_type: *mut ResetType,
    ) -> Status;

type QueryVariableInfo =
    extern "C" fn(
        _attributes: u32,
        _maximum_variable_storage_size: *mut u64,
        _remaining_variable_storage_size: *mut u64,
        _maximum_variable_size: *mut u64,
    ) -> Status;

#[repr(C)]
pub struct RuntimeServices {
    hdr: TableHeader,
    get_time: GetTime,
    set_time: SetTime,
    get_wakeup_time: GetWakeupTime,
    set_wakeup_time: SetWakeupTime,

    set_virtual_address_map: SetVirtualAddressMap,
    convert_pointer: ConvertPointer,

    get_variable: GetVariable,
    get_next_variable_name: GetNextVariableName,
    set_variable: SetVariable,

    get_next_high_mono_count: GetNextHighMonotonicCount,
    reset_system: ResetSystem,

    update_capsule: UpdateCapsule,
    query_capsule_capabilities: QueryCapsuleCapabilities,

    query_variable_info: QueryVariableInfo,
}

extern "C" fn get_time(
    _time: *mut Time,
    _capabilities: *mut TimeCapabilities
) -> Status {
    Status::EFI_UNSUPPORTED
}

extern "C" fn set_time(
    _time: *const Time
) -> Status {
    Status::EFI_UNSUPPORTED
}

extern "C" fn get_wakeup_time(
    _enabled: *mut Bool,
    _pending: *mut Bool,
    _time: *mut Time
) -> Status {
    Status::EFI_UNSUPPORTED
}

extern "C" fn set_wakeup_time(
    _enable: Bool,
    _time: *const Time
) -> Status {
    Status::EFI_UNSUPPORTED
}

extern "C" fn set_virtual_address_map(
    _memory_map_size: usize,
    _descriptor_size: usize,
    _descriptor_version: u32,
    _virtual_map: *const MemoryDescriptor
) -> Status {
    Status::EFI_UNSUPPORTED
}

extern "C" fn convert_pointer(
    _debug_disposition: usize,
    _address: *const *mut ()
) -> Status {
    Status::EFI_UNSUPPORTED
}

extern "C" fn get_variable(
    _variable_name: *const Char16,
    _vendor_guid: *const Guid,
    _attributes: *mut u32,
    _data_size: *mut usize,
    _data: *mut ()
) -> Status {
    Status::EFI_NOT_FOUND
}

extern "C" fn get_next_variable_name(
    _variable_name_size: *mut usize,
    _variable_name: *mut Char16,
    _vendor_guid: *mut Guid
) -> Status {
    Status::EFI_NOT_FOUND
}

extern "C" fn set_variable(
    _variable_name: *const Char16,
    _vendor_guid: *const Guid,
    _attributes: *const u32,
    _data_size: *const usize,
    _data: *const ()
) -> Status {
    Status::EFI_UNSUPPORTED
}

extern "C" fn get_next_high_monotonic_count(
    _high_count: *mut u32
) -> Status {
    Status::EFI_UNSUPPORTED
}

extern "C" fn reset_system(
    _reset_type: ResetType,
    _reset_status: Status,
    _data_size: usize,
    _reset_data: *const ()
) -> Status {
    match _reset_type {
        ResetType::EfiResetShutdown => psci::poweroff(),
        _ => psci::reboot(),
    }
}

extern "C" fn update_capsule(
    _capsule_header_array: *const *const CapsuleHeader,
    _capsule_count: usize,
    _scatter_gather_list: PhysicalAddress,
) -> Status {
    Status::EFI_UNSUPPORTED
}

extern "C" fn query_capsule_capabilities(
    _capsule_header_array: *const *const CapsuleHeader,
    _capsule_count: usize,
    _maximum_capsule_size: *mut u64,
    _reset_type: *mut ResetType,
) -> Status {
    Status::EFI_UNSUPPORTED
}

extern "C" fn query_variable_info(
    _attributes: u32,
    _maximum_variable_storage_size: *mut u64,
    _remaining_variable_storage_size: *mut u64,
    _maximum_variable_size: *mut u64,
) -> Status {
    Status::EFI_UNSUPPORTED
}

#[link_section = ".rtsdata"]
static mut RT: RuntimeServices = RuntimeServices {
    hdr: TableHeader {
        signature: [b'R', b'U', b'N', b'T', b'S', b'E', b'R', b'V'],
        revision: UEFI_REVISION,
        header_size: core::mem::size_of::<RuntimeServices>() as u32,
        crc32: 0,
        reserved: 0,
    },

    get_time: get_time,
    set_time: set_time,
    get_wakeup_time: get_wakeup_time,
    set_wakeup_time: set_wakeup_time,

    set_virtual_address_map: set_virtual_address_map,
    convert_pointer: convert_pointer,

    get_variable: get_variable,
    get_next_variable_name: get_next_variable_name,
    set_variable: set_variable,

    get_next_high_mono_count: get_next_high_monotonic_count,
    reset_system: reset_system,

    update_capsule: update_capsule,
    query_capsule_capabilities: query_capsule_capabilities,

    query_variable_info: query_variable_info,
};

impl RuntimeServices {
    pub fn get() -> &'static Self {
        unsafe {
            RT.hdr.update_crc();
            &RT
        }
    }
}
