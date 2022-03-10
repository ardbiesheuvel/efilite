// SPDX-License-Identifier: GPL-2.0
// Copyright 2022-2023 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

#![no_std]
#![no_main]

macro_rules! ldrange {
    ($start:ident, $end:ident) => {
        unsafe {
            extern "C" {
                static $start: u8;
                static $end: u8;
            }
            (&$start as *const _ as usize)..(&$end as *const _ as usize)
        }
    };
}

mod console;
mod fwcfg;
mod initrd;
mod mapper;
mod pl031;
mod psci;
mod rng;

use core::mem::MaybeUninit;
use core::{arch::global_asm, panic::PanicInfo};
use linked_list_allocator::LockedHeap;
use log::{debug, error, info};

extern crate alloc;
use alloc::vec::Vec;

use aarch64_paging::paging::Attributes;

use efiloader::*;
use efiloader::memmap::*;
use efiloader::memorytype::*;
use efiloader::memorytype::EfiMemoryType::*;

const DTB_GUID: Guid = guid!(
    0xb1b621d5,
    0xf19c,
    0x41a5,
    [0x83, 0x0b, 0xd9, 0x15, 0x2c, 0x69, 0xaa, 0xe0]
);

const RSDP_GUID: Guid = guid!(
    0x8868e871,
    0xe4f1,
    0x11d3,
    [0xbc, 0x22, 0x00, 0x80, 0xc7, 0x3c, 0x88, 0x81]
);

const SMBIOS3_GUID: Guid = guid!(
    0xf2fd1544,
    0x9794,
    0x4a2c,
    [0x99, 0x2e, 0xe5, 0xbb, 0xcf, 0x20, 0xe3, 0x94]
);

#[global_allocator]
pub static ALLOCATOR: LockedHeap = LockedHeap::empty();

#[no_mangle]
extern "C" fn efilite_main(base: *mut u8, used: isize, avail: usize) {
    // Grab the device tree blob that QEMU left for us in memory
    // Just return if we cannot parse it - no point in limping on
    let dtb = ldrange!(_dtb_start, _dtb_end);
    let fdt = match unsafe { fdt::Fdt::from_ptr(dtb.start as *const u8) } {
        Err(_) => {
            return;
        }
        Ok(f) => f,
    };

    #[cfg(debug_assertions)]
    log::set_max_level(log::LevelFilter::Debug);

    #[cfg(not(debug_assertions))]
    log::set_max_level(log::LevelFilter::Warn);

    // Use the stdout-path as the console - assume it refers to a UART
    // whose first 'reg' property describes a MMIO register that is
    // compatible with our SimpleConsole implementation.
    let con = fdt
        .chosen()
        .stdout()
        .map(|n| {
            let c = console::init_from_fdt_node(n)?;
            log::set_logger(c).ok()?;
            info!("Using {} for console output\n", n.name);
            Some(c)
        })
        .flatten();

    // Give the mapped but unused memory to the heap allocator
    unsafe {
        ALLOCATOR.lock().init(base.offset(used), avail);
    }
    info!("Heap allocator with {} KB of memory\n", avail / 1024);

    // Grab the command line from DT and convert it to UTF-16
    let cmdline = {
        let mut v = Vec::new();
        fdt.chosen().bootargs().map(|args| {
            info!("Using command line from /chosen/bootargs: {:?}\n", args);
            v = args.encode_utf16().collect::<Vec<u16>>()
        });
        v
    };

    let mut mapper = mapper::MemoryMapper::new();

    let (ro_flags, rw_flags, dev_flags) = {
        let flags = Attributes::VALID | Attributes::NON_GLOBAL;
        (
            flags | Attributes::NORMAL | Attributes::READ_ONLY,
            flags | Attributes::NORMAL | Attributes::EXECUTE_NEVER,
            flags | Attributes::DEVICE_NGNRE | Attributes::EXECUTE_NEVER,
        )
    };

    // Map the UART MMIO register into the ID map
    con.map(|c| {
        let r = c.base..c.base + EFI_PAGE_SIZE;
        mapper.map_reserved_range(&r, dev_flags);
    });

    // Locate the fwcfg node and map its MMIO registers into the ID map
    let fwcfg = fdt
        .find_compatible(&["qemu,fw-cfg-mmio"])
        .map(|n| {
            info!("QEMU fwcfg node found: {}\n", n.name);
            let f = fwcfg::FwCfg::from_fdt_node(n)?;
            let b = n.reg()?.nth(0)?.starting_address as usize;
            let r = b..b + EFI_PAGE_SIZE;
            mapper.map_reserved_range(&r, dev_flags);
            Some(f)
        })
        .flatten()
        .expect("QEMU fwcfg node not found or unusable");

    // Check whether fwcfg exposes a kernel image - no need to proceed otherwise
    let kloader = fwcfg.get_kernel_loader().expect("No kernel image provided");

    // Create a new EFI memory map
    let memmap = MemoryMap::new();

    // Locate the PL031 RTC node and map its MMIO registers.
    // Map it into the EFI memory map too so it can be used under the OS as well.
    let pl031 = fdt
        .find_compatible(&["arm,pl031"])
        .map(|n| {
            info!("PL031 RTC node found: {}\n", n.name);
            let b = n.reg()?.nth(0)?.starting_address as usize;
            let r = b..b + EFI_PAGE_SIZE;
            mapper.map_reserved_range(&r, dev_flags);
            memmap
                .declare_runtime_region(&r, EfiMemoryMappedIO, EFI_MEMORY_UC, 0)
                .ok()?;
            Some(r)
        })
        .flatten();

    info!("Mapping all DRAM regions found in the DT:\n");
    for reg in fdt.memory().regions() {
        let b = reg.starting_address as usize;
        let r = b..b + reg.size.unwrap_or(0);
        mapper.map_range(&r, rw_flags);
        memmap
            .declare_memory_region(&r)
            .unwrap_or_else(|e| log::warn!("Unsupported RAM region {:x?}\n", e));
    }

    info!("Remapping statically allocated regions:\n");
    mapper.map_reserved_range(&ldrange!(_rtcode_start, _rtcode_end), ro_flags);
    mapper.map_reserved_range(&ldrange!(_dtb_end, _rtdata_end), rw_flags);
    mapper.map_range(&dtb, ro_flags | Attributes::EXECUTE_NEVER);

    // Switch to the new ID map so we can use all of DRAM
    mapper.activate();

    // Declare the flash code region as a runtime code region so all code
    // is callable while running under the OS
    memmap
        .declare_runtime_region(
            &ldrange!(_rtcode_start, _rtcode_end),
            EfiRuntimeServicesCode,
            EFI_MEMORY_WT,
            EFI_MEMORY_RO,
        )
        .unwrap();

    // Allocate the static RAM used by this firmware as boot services data
    memmap
        .allocate_region(
            &ldrange!(_bsdata_start, _bsdata_end),
            EfiBootServicesData,
            0,
        )
        .unwrap();

    // Back the EfiBootServicesData pool (which is used for the stacks) with statically allocated
    // memory that is covered by the static initial mapping in NOR flash. This ensures that we can
    // deactivate/activate the IdMap in calls into the EFI memory attributes protocol.
    const BSPOOL_SIZE: usize = 1024 * 1024;
    unsafe {
        static mut BSPOOL: [MaybeUninit<u8>; BSPOOL_SIZE] = [MaybeUninit::uninit(); BSPOOL_SIZE];
        memmap
            .declare_pool(EfiBootServicesData, &mut BSPOOL)
            .expect("Failed to declare memory pool");
    }

    // Allocate the EFI .rtdata section as runtime data
    memmap
        .allocate_region(
            &ldrange!(_rtdata_start, _rtdata_end),
            EfiRuntimeServicesData,
            EFI_MEMORY_XP,
        )
        .unwrap();

    // Back the EfiRuntimeServicesData pool (which is used internally to allocate the system table
    // and runtime services table) by a statically allocated region which is covered by the static
    // initial mapping in NOR flash.
    const RTPOOL_SIZE: usize = 32 * 1024;
    unsafe {
        #[link_section = ".rtdata"]
        static mut RTPOOL: [MaybeUninit<u8>; RTPOOL_SIZE] = [MaybeUninit::uninit(); RTPOOL_SIZE];
        memmap
            .declare_pool(EfiRuntimeServicesData, &mut RTPOOL)
            .expect("Failed to declare memory pool");
    }

    let con = con.map(|c| c as &(dyn SimpleConsole));
    let rng = Some(rng::Random::new());
    let efi = efiloader::init(con, memmap, mapper, rng).expect("Failed to init EFI runtime");

    // Register our PSCI based ResetSystem implementation
    efi.override_reset_handler(psci::reset_system);

    // Try loading the ACPI tables from QEMU
    let tbl = fwcfg.load_firmware_tables(efi);
    if let Ok(rsdp) = tbl {
        info!("Booting in ACPI mode\n");
        efi.install_configtable(&RSDP_GUID, rsdp as *const ());

        // ACPI does not describe the RTC as a device, so we need to expose
        // it via the GetTime EFI runtime service
        pl031.map(|r| {
            let f = pl031::init(r.start);
            efi.override_time_handler(f, None);
        });
    } else {
        debug!("ACPI tables unavailable: {}\n", tbl.err().unwrap());
        info!("Booting in DT mode\n");
        efi.install_configtable(&DTB_GUID, dtb.start as *const ());
    }

    if let Ok(anchor) = fwcfg.load_smbios_tables(efi) {
        info!("Installing SMBIOS tables\n");
        efi.install_configtable(&SMBIOS3_GUID, anchor as *const ());
    }

    fwcfg.get_initrd_loader().map(|i| efi.set_initrd_loader(i));

    if let Some(mut li) = efi.load_image(&kloader) {
        li.set_load_options(cmdline);

        info!("Starting loaded EFI program\n");
        let ret = li.start_image();
        info!("EFI program exited with return value {:?}\n", ret);
    } else {
        info!("Failed to load image\n");
    }
}

#[no_mangle]
extern "C" fn handle_exception(esr: u64, elr: u64, far: u64) -> ! {
    panic!(
        "Unhandled exception: ESR = 0x{:X}, ELR = 0x{:X}, FAR = 0x{:X}.",
        esr, elr, far
    );
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    log::set_max_level(log::LevelFilter::Error);
    error!("{}\n", info);
    loop {}
}

global_asm!(include_str!("entry.s"));
global_asm!(include_str!("ttable.s"));
