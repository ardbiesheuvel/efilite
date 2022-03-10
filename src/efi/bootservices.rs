// SPDX-License-Identifier: GPL-2.0
// Copyright 2022 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

use crate::efi::{memorytype::*, tableheader::*, status::*};
use crate::efi::devicepath::DevicePath;
use crate::efi::{Bool, Char16, Handle, Guid, PhysicalAddress, Tpl, Event, EventNotify};
use crate::efi::{TPL_APPLICATION, UEFI_REVISION, ProtocolDb, PROTOCOL_DB};
use crate::efi::RTSDATA_ALLOCATOR;
use crate::efi::install_configtable;
use crate::efi::memmap;
use crate::efi::memmap::Placement;

use crate::efi::devicepath;
use crate::efi::bootservices::AllocateType::*;
use crate::efi::devicepath::EFI_DEVICE_PATH_PROTOCOL_GUID;
use crate::efi::EFI_LOADED_IMAGE_PROTOCOL_GUID;
use crate::efi::LoadedImage;
use crate::efi::loadedimage::exit_image;

use core::sync::atomic::{AtomicUsize, Ordering};
use core::{ptr, slice};
use core::ptr::NonNull;

use alloc::collections::BTreeMap;

const EFI_MEMORY_DESCRIPTOR_VERSION: u32 = 1;

#[allow(dead_code)]
#[derive(PartialEq)]
#[repr(C)]
pub enum AllocateType {
    AllocateAnyPages,
    AllocateMaxAddress,
    AllocateAddress,
}

#[allow(dead_code)]
#[repr(C)]
pub enum TimerDelay {
    TimerCancel,
    TimerPeriodic,
    TimerRelative
}

#[allow(dead_code)]
#[allow(non_camel_case_types)]
#[repr(C)]
pub enum InterfaceType {
    EFI_NATIVE_INTERFACE,
}

#[allow(dead_code)]
#[repr(C)]
pub enum LocateSearchType {
    AllHandles,
    ByRegisterNotify,
    ByProtocol,
}

#[repr(C)]
pub struct OpenProtocolInformationEntry {
    _agent_handle: Handle,
    _controller_handle: Handle,
    _attributes: u32,
    _open_count: u32,
}

type RaiseTpl = extern "C" fn(Tpl) -> Tpl;
type RestoreTpl = extern "C" fn(Tpl);

type AllocatePages = extern "C" fn(AllocateType, MemoryType, usize, *mut PhysicalAddress) -> Status;
type FreePages = extern "C" fn(PhysicalAddress, usize) -> Status;
type GetMemoryMap = extern "C" fn(*mut usize, *mut MemoryDescriptor, *mut usize, *mut usize, *mut u32) -> Status;
type AllocatePool = extern "C" fn(MemoryType, usize, *mut *mut ()) -> Status;
type FreePool = extern "C" fn(*mut ()) -> Status;

type CreateEvent = extern "C" fn(u32, Tpl, EventNotify, *const (), *mut Event) -> Status;
type SetTimer = extern "C" fn(Event, TimerDelay, u64) -> Status;
type WaitForEvent = extern "C" fn(usize, *const Event, *mut usize) -> Status;
type SignalOrCheckOrCloseEvent = extern "C" fn(Event) -> Status;

type InstallProtocolInterface = extern "C" fn(*mut Handle, *const Guid, InterfaceType, *const ()) -> Status;
type ReinstallProtocolInterface = extern "C" fn(Handle, *const Guid, *const (), *const()) -> Status;
type UninstallProtocolInterface = extern "C" fn(Handle, *const Guid, *const ()) -> Status;
type HandleProtocol = extern "C" fn(Handle, *const Guid, *mut *const ()) -> Status;
type RegisterProtocolNotify = extern "C" fn(*const Guid, Event, *mut *const ()) -> Status;
type LocateHandle = extern "C" fn(LocateSearchType, *const Guid, *const (), *mut usize, *mut Handle) -> Status;
type LocateDevicePath = extern "C" fn(*const Guid, *mut *const DevicePath, *mut Handle) -> Status;
type InstallConfigurationTable = extern "C" fn(*const Guid, *const ()) -> Status;

type LoadImage = extern "C" fn(Bool, Handle, *const DevicePath, *const (), usize, *mut Handle) -> Status;
type StartImage = extern "C" fn(Handle, *mut usize, *mut Char16) -> Status;
type Exit = extern "C" fn(Handle, Status, usize, *const Char16) -> Status;
type UnloadImage = extern "C" fn(Handle) -> Status;
type ExitBootServices = extern "C" fn(Handle, usize) -> Status;

type GetNextMonotonicCount = extern "C" fn(*mut u64) -> Status;
type Stall = extern "C" fn(usize) -> Status;
type SetWatchdogTimer = extern "C" fn(usize, u64, usize, *const Char16) -> Status;

type ConnectController = extern "C" fn(Handle, Handle, *const DevicePath, Bool) -> Status;
type DisconnectController = extern "C" fn(Handle, Handle, Handle) -> Status;

type OpenProtocol = extern "C" fn(Handle, *const Guid, *mut *const (), Handle, Handle, u32) -> Status;
type CloseProtocol = extern "C" fn(Handle, *const Guid, Handle, Handle) -> Status;
type OpenProtocolInformation = extern "C" fn(Handle, *const Guid, *mut *const OpenProtocolInformationEntry, *mut usize) -> Status;

type ProtocolPerHandle = extern "C" fn(Handle, *mut *const *const Guid, *mut usize) -> Status;
type LocateHandleBuffer = extern "C" fn(LocateSearchType, *const Guid, *const (), *mut usize, *mut *const Handle) -> Status;
type LocateProtocol = extern "C" fn(*const Guid, *const (), *mut *const ())-> Status;

#[repr(C)]
pub struct BootServices {
    hdr: TableHeader,

    raise_tpl: RaiseTpl,
    restore_tpl: RestoreTpl,

    allocate_pages: AllocatePages,
    free_pages: FreePages,
    get_memory_map: GetMemoryMap,
    allocate_pool: AllocatePool,
    free_pool: FreePool,

    create_event: CreateEvent,
    set_timer: SetTimer,
    wait_for_event: WaitForEvent,
    signal_event: SignalOrCheckOrCloseEvent,
    close_event: SignalOrCheckOrCloseEvent,
    check_event: SignalOrCheckOrCloseEvent,

    install_protocol_interface: InstallProtocolInterface,
    reinstall_protocol_interface: ReinstallProtocolInterface,
    uninstall_protocol_interface: UninstallProtocolInterface,
    handle_protocol: HandleProtocol,
    reserved: *const (),
    register_protocol_notify: RegisterProtocolNotify,
    locate_handle: LocateHandle,
    locate_device_path: LocateDevicePath,
    install_configuration_table: InstallConfigurationTable,

    load_image: LoadImage,
    start_image: StartImage,
    exit: Exit,
    unload_image: UnloadImage,
    exit_boot_services: ExitBootServices,

    get_next_monotonic_count: GetNextMonotonicCount,
    stall: Stall,
    set_watchdog_timer: SetWatchdogTimer,

    connect_controller: ConnectController,
    disconnect_controller: DisconnectController,

    open_protocol: OpenProtocol,
    close_protocol: CloseProtocol,
    open_protocol_information: OpenProtocolInformation,

    protocols_per_handle: ProtocolPerHandle,
    locate_handle_buffer: LocateHandleBuffer,
    locate_protocol: LocateProtocol,
    // TODO remaining fields
}

static mut BS: BootServices = BootServices {
    hdr: TableHeader {
        signature: [b'B', b'O', b'O', b'T', b'S', b'E', b'R', b'V'],
        revision: UEFI_REVISION,
        header_size: core::mem::size_of::<BootServices>() as u32,
        crc32: 0,
        reserved: 0,
    },
    raise_tpl: raise_tpl,
    restore_tpl: restore_tpl,

    allocate_pages: allocate_pages,
    free_pages: free_pages,
    get_memory_map: get_memory_map,
    allocate_pool: allocate_pool,
    free_pool: free_pool,

    create_event: create_event,
    set_timer: set_timer,
    wait_for_event: wait_for_event,
    signal_event: signal_event,
    close_event: close_event,
    check_event: check_event,

    install_protocol_interface: install_protocol_interface,
    reinstall_protocol_interface: reinstall_protocol_interface,
    uninstall_protocol_interface: uninstall_protocol_interface,
    handle_protocol: handle_protocol,
    reserved: ptr::null(),
    register_protocol_notify: register_protocol_notify,
    locate_handle: locate_handle,
    locate_device_path: locate_device_path,
    install_configuration_table: install_configuration_table,

    load_image: load_image,
    start_image: start_image,
    exit: exit,
    unload_image: unload_image,
    exit_boot_services: exit_boot_services,

    get_next_monotonic_count: get_next_monotonic_count,
    stall: stall,
    set_watchdog_timer: set_watchdog_timer,

    connect_controller: connect_controller,
    disconnect_controller: disconnect_controller,

    open_protocol: open_protocol,
    close_protocol: close_protocol,
    open_protocol_information: open_protocol_information,

    protocols_per_handle: protocols_per_handle,
    locate_handle_buffer: locate_handle_buffer,
    locate_protocol: locate_protocol,
};

static CURRENT_TPL: AtomicUsize = AtomicUsize::new(TPL_APPLICATION);

extern "C" fn raise_tpl(new_tpl: Tpl) -> Tpl {
    CURRENT_TPL.swap(new_tpl, Ordering::AcqRel)
}

extern "C" fn restore_tpl(old_tpl: Tpl) {
    CURRENT_TPL.store(old_tpl, Ordering::Release);
}

extern "C" fn allocate_pages(
    _type: AllocateType,
    _memory_type: MemoryType,
    _pages: usize,
    _memory: *mut PhysicalAddress
) -> Status {
    let placement: Placement = match _type {
        AllocateAnyPages => Placement::Anywhere,
        AllocateMaxAddress => Placement::Max(unsafe { *_memory }),
        AllocateAddress => Placement::Fixed(unsafe { *_memory }),
    };

    if let Some(region) = memmap::allocate_pages(_pages, _memory_type, placement) {
        unsafe { *_memory = region.as_ptr() as u64; }
        Status::EFI_SUCCESS
    } else {
        Status::EFI_OUT_OF_RESOURCES
    }
}

extern "C" fn free_pages(
    memory: PhysicalAddress,
    pages: usize
) -> Status {
    if (memory as usize & memmap::EFI_PAGE_MASK) != 0 {
        return Status::EFI_INVALID_PARAMETER;
    }
    if let Ok(_) = memmap::free_pages(memory, pages) {
        Status::EFI_SUCCESS
    } else {
        Status::EFI_NOT_FOUND
    }
}

extern "C" fn get_memory_map(
    memory_map_size: *mut usize,
    memory_map: *mut MemoryDescriptor,
    map_key: *mut usize,
    descriptor_size: *mut usize,
    descriptor_version: *mut u32
) -> Status {
    let descsize = core::mem::size_of::<MemoryDescriptor>();
    let buffersize = unsafe { *memory_map_size };
    let maplen = memmap::len();
    let mapsize = maplen * descsize;
    unsafe { *memory_map_size = mapsize; }
    if buffersize < mapsize {
        return Status::EFI_BUFFER_TOO_SMALL;
    }

    let buffer = unsafe {
        &mut slice::from_raw_parts_mut(memory_map, maplen)
    };

    let key = memmap::copy_to_slice(buffer);
    unsafe {
        *map_key = key;
        *descriptor_size = descsize;
        *descriptor_version = EFI_MEMORY_DESCRIPTOR_VERSION;
    }
    Status::EFI_SUCCESS
}

static mut POOL_ALLOCATIONS: BTreeMap<u64, core::alloc::Layout> = BTreeMap::new();

extern "C" fn allocate_pool(
    _pool_type: MemoryType,
    size: usize,
    buffer: *mut *mut ()
) -> Status {
    let mut alloc = RTSDATA_ALLOCATOR.lock();
    let pool = unsafe { &mut POOL_ALLOCATIONS };

    // For simplicity, serve all pool allocations from the RuntimeServicesData region
    if let Ok(layout) = core::alloc::Layout::from_size_align(size, 8) {
        if let Ok(buf) = alloc.allocate_first_fit(layout) {
            let buf = buf.as_ptr();
            if buf.is_null() {
                return Status::EFI_OUT_OF_RESOURCES;
            }

            pool.insert(buf as u64, layout);

            unsafe { *buffer = buf as _ };
            return Status::EFI_SUCCESS;
        }
    }
    Status::EFI_INVALID_PARAMETER
}

extern "C" fn free_pool(
    buffer: *mut ()
) -> Status {
    let mut alloc = RTSDATA_ALLOCATOR.lock();
    let pool = unsafe { &mut POOL_ALLOCATIONS };
    let base = buffer as u64;

    if let Some(layout) = pool.remove(&base) {
        unsafe {
            alloc.deallocate(NonNull::new_unchecked(buffer as _), layout)
        };
        return Status::EFI_SUCCESS;
    }
    Status::EFI_INVALID_PARAMETER
}

extern "C" fn create_event(
    _type: u32,
    _notify_tpl: Tpl,
    _notify_function: EventNotify,
    _notify_context: *const (),
    _event: *mut Event
) -> Status {
    Status::EFI_OUT_OF_RESOURCES
}

extern "C" fn set_timer(
    _event: Event,
    _type: TimerDelay,
    _trigger_time: u64
) -> Status {
    Status::EFI_INVALID_PARAMETER
}

extern "C" fn wait_for_event(
    _number_of_events: usize,
    _event: *const Event,
    _index: *mut usize
) -> Status {
    Status::EFI_UNSUPPORTED
}

extern "C" fn signal_event(
    _event: Event
) -> Status {
    Status::EFI_SUCCESS
}

extern "C" fn close_event(
    _event: Event
) -> Status {
    Status::EFI_SUCCESS
}

extern "C" fn check_event(
    _event: Event
) -> Status {
    Status::EFI_NOT_READY
}

extern "C" fn install_protocol_interface(
    _handle: *mut Handle,
    _protocol: *const Guid,
    _interface_type: InterfaceType,
    _interface: *const ()
) -> Status {
    Status::EFI_OUT_OF_RESOURCES
}

extern "C" fn reinstall_protocol_interface(
    _handle: Handle,
    _protocol: *const Guid,
    _old_interface: *const (),
    _new_interface: *const()
) -> Status {
    Status::EFI_NOT_FOUND
}

extern "C" fn uninstall_protocol_interface(
    _handle: Handle,
    _protocol: *const Guid,
    _interface: *const ()
) -> Status {
    Status::EFI_NOT_FOUND
}

extern "C" fn handle_protocol(
    _handle: Handle,
    _protocol: *const Guid,
    _interface: *mut *const ()
) -> Status {
    let protocol = unsafe { &*_protocol };
    let key = (_handle, *protocol);
    if let Some(ptr) = unsafe { PROTOCOL_DB.get(&key) } {
        unsafe { *_interface = *ptr };
        Status::EFI_SUCCESS
    } else {
        Status::EFI_UNSUPPORTED
    }
}

extern "C" fn register_protocol_notify(
    _protocol: *const Guid,
    _event: Event,
    _registration: *mut *const ()
) -> Status {
    Status::EFI_OUT_OF_RESOURCES
}

extern "C" fn locate_handle(
    _search_type: LocateSearchType,
    _protocol: *const Guid,
    _search_key: *const (),
    _buffer_size: *mut usize,
    _buffer: *mut Handle
) -> Status {
    Status::EFI_NOT_FOUND
}

fn compare_device_path(entry: (&(usize, Guid), &*const ()),
                       protocol: &Guid,
                       device_path: &DevicePath,
                       db: &ProtocolDb,
) -> Option<(isize, (Handle, *const ()))> {
    // Check if this handle implements both the device path protocol
    // and the requested protocol
    let guid = &entry.0.1;
    let key = (entry.0.0, *protocol);
    if *guid != EFI_DEVICE_PATH_PROTOCOL_GUID || !db.contains_key(&key) {
        return None
    }

    // Check whether the device path in the protocol database
    // is a prefix of the provided device path
    let bytes_equal = devicepath::is_prefix(
        unsafe { &*(*entry.1 as *const DevicePath) },
        device_path,
        );
    if bytes_equal == 0 {
        return None
    }

    let devpathptr = unsafe {
        (device_path as *const _ as *const u8).offset(bytes_equal)
    };
    Some((bytes_equal, (entry.0.0, devpathptr as *const ())))
}

extern "C" fn locate_device_path(
    protocol: *const Guid,
    device_path: *mut *const DevicePath,
    device: *mut Handle
) -> Status {
    let db = unsafe { &PROTOCOL_DB };
    let protocol = unsafe { &*protocol };
    let devpath = unsafe { &**device_path };

    // Find all handles that have both the given protocol and
    // the DevicePath protocol installed, and classify them by
    // how many bytes the device path has in common with the
    // provided one, if any
    if let Some(entry) = db.iter()
        .filter_map(
            |entry: (&(usize, Guid), &*const ())|
                compare_device_path(entry, protocol, devpath, db))
        .max_by(|a, b| a.0.cmp(&b.0)) {

        unsafe {
            *device = entry.1.0;
            *device_path = entry.1.1 as _;
        }
        return Status::EFI_SUCCESS
    }
    Status::EFI_NOT_FOUND
}

extern "C" fn install_configuration_table(
    guid: *const Guid,
    table: *const ()
) -> Status {
    install_configtable(unsafe { &*guid }, table);
    Status::EFI_SUCCESS
}

extern "C" fn load_image(
    _boot_policy: Bool,
    _parent_image_handle: Handle,
    _device_path: *const DevicePath,
    _source_buffer: *const (),
    _source_size: usize,
    _image_handle: *mut Handle
) -> Status {
    Status::EFI_SUCCESS
}

extern "C" fn start_image(
    _handle: Handle,
    _exit_data_size: *mut usize,
    _exit_data: *mut Char16
) -> Status {
    Status::EFI_SUCCESS
}

extern "C" fn exit(
    image_handle: Handle,
    exit_status: Status,
    _exit_data_size: usize,
    _exit_data: *const Char16
) -> Status {
    let db = unsafe { &PROTOCOL_DB };
    let key = (image_handle, EFI_LOADED_IMAGE_PROTOCOL_GUID);
    if let Some(ptr) = db.get(&key) {
        unsafe {
            let li = &mut *(*ptr as *mut LoadedImage);
            if li.reserved != 0 {
                let sp = li.reserved;
                li.reserved = 0;
                exit_image(exit_status, sp);
            }
        }
    }
    Status::EFI_INVALID_PARAMETER
}

extern "C" fn unload_image(
    _image_handle: Handle
) -> Status {
    Status::EFI_UNSUPPORTED
}

extern "C" fn exit_boot_services(
    _image_handle: Handle,
    _map_key: usize
) -> Status {
    if _map_key != memmap::key() {
        return Status::EFI_INVALID_PARAMETER;
    }
    Status::EFI_SUCCESS
}

extern "C" fn get_next_monotonic_count(
    _count: *mut u64
) -> Status {
    Status::EFI_SUCCESS
}

extern "C" fn stall(
    _micro_seconds: usize
) -> Status {
    Status::EFI_SUCCESS
}

extern "C" fn set_watchdog_timer(
    _timeout: usize,
    _watchdog_code: u64,
    _data_size: usize,
    _watchdog_data: *const Char16
) -> Status {
    Status::EFI_SUCCESS
}

extern "C" fn connect_controller(
    _controller_handle: Handle,
    _driver_image_handle: Handle,
    _remaining_device_path: *const DevicePath,
    _recursive: Bool
) -> Status {
    Status::EFI_NOT_FOUND
}

extern "C" fn disconnect_controller(
    _controller_handle: Handle,
    _driver_image_handle: Handle,
    _child_handle: Handle
) -> Status {
    Status::EFI_SUCCESS
}

extern "C" fn open_protocol(
    _handle: Handle,
    _protocol: *const Guid,
    _interface: *mut *const (),
    _agent_handle: Handle,
    _controller_handle: Handle,
    _attributes: u32
) -> Status {
    Status::EFI_SUCCESS
}

extern "C" fn close_protocol(
    _handle: Handle,
    _protocol: *const Guid,
    _agent_handle: Handle,
    _controller_handle: Handle
) -> Status {
    Status::EFI_SUCCESS
}

extern "C" fn open_protocol_information(
    _handle: Handle,
    _protocol: *const Guid,
    _entry_buffer: *mut *const OpenProtocolInformationEntry,
    _entry_count: *mut usize
) -> Status {
    Status::EFI_SUCCESS
}

extern "C" fn protocols_per_handle(
    _handle: Handle,
    _protocol_buffer: *mut *const *const Guid,
    _protocol_buffer_count: *mut usize
) -> Status {
    Status::EFI_SUCCESS
}

extern "C" fn locate_handle_buffer(
    _search_type: LocateSearchType,
    _protocol: *const Guid,
    _search_key: *const (),
    _no_handles: *mut usize,
    _buffer: *mut *const Handle
) -> Status {
    Status::EFI_SUCCESS
}

extern "C" fn locate_protocol(
    _protocol: *const Guid,
    _registration: *const (),
    _interface: *mut *const ()
) -> Status {
    let db = unsafe { &PROTOCOL_DB };
    let protocol = unsafe { *_protocol };
    if let Some(entry) = db.iter()
        .find(|e: &(&(usize, Guid), &*const ())| e.0.1 == protocol) {
        unsafe { *_interface = *entry.1; }
        Status::EFI_SUCCESS
    } else {
        Status::EFI_NOT_FOUND
    }
}

impl BootServices {
    pub fn get() -> &'static Self {
        unsafe {
            BS.hdr.update_crc();
            &BS
        }
    }
}
