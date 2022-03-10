// SPDX-License-Identifier: GPL-2.0
// Copyright 2022 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

#![no_std]
#![no_main]
#![feature(default_alloc_error_handler)]

// needed by the paging code
#![feature(new_uninit)]

// for the protocol db
#![feature(const_btree_new)]

// for the EFI memory map
#![feature(btree_drain_filter)]

macro_rules! align_down {
    ($value:expr, $alignment:expr) => {
        ($value) & !($alignment - 1)
    };
}

macro_rules! align_up {
    ($value:expr, $alignment:expr) => {
        (($value - 1) | ($alignment - 1)) + 1
    };
}

mod cmo;
mod console;
mod cstring;
mod efi;
mod fwcfg;
mod idmap;
mod initrd;
mod pagealloc;
mod paging;
mod pecoff;
mod psci;
mod rng;

use core::{arch::global_asm, panic::PanicInfo, slice};
use linked_list_allocator::LockedHeap;
use log::{error, info};

extern crate alloc;
use alloc::vec::Vec;

use crate::paging::Attributes;
use crate::efi::memorytype::*;
use crate::efi::memmap::Placement;
use crate::MemoryType::{EfiBootServicesData,EfiRuntimeServicesData};

#[macro_use]
extern crate bitflags;

#[global_allocator]
pub static ALLOCATOR: LockedHeap = LockedHeap::empty();

extern "C" {
    static RTSCODE: [u8; 0x1f0000];
    static DTB: [u8; 0x200000];

    static BSDATA: [u8; 0x3f0000];
    static RTSDATA: [u8; 0x10000];
}

#[no_mangle]
extern "C" fn efilite_main(base: *const u8, mapped: usize, used: usize, avail: usize) {
    #[cfg(debug_assertions)]
    log::set_logger(&console::OUT)
        .map(|()| log::set_max_level(log::LevelFilter::Info))
        .unwrap();

    // Give the mapped but unused memory to the heap allocator
    info!("Heap allocator with {} KB of memory\n", avail / 1024);
    unsafe { ALLOCATOR.lock().init(base as usize + used, avail); }

    // use of extern static is unsafe
    let dtb = unsafe { &DTB };

    let fdt = fdt::Fdt::new(dtb).expect("Failed to parse device tree");
    let cmdline = {
        let mut v = Vec::new();
        fdt.chosen()
            .bootargs()
            .map(|args| {
                 v = args.encode_utf16().collect::<Vec<u16>>();
                 info!("/chosen/bootargs: {:?}\n", args)
            });
        v
    };

    let mut idmap = idmap::IdMap::new();

    efi::init();

    let rw_flags = Attributes::NORMAL.non_global().execute_disable();

    info!("Mapping all DRAM regions found in the DT:\n");
    for reg in fdt.memory().regions() {
        if let Some(size) = reg.size {
            let range = unsafe { slice::from_raw_parts(reg.starting_address, size) };
            idmap.map_range(range, rw_flags);
            efi::memmap::declare_memory_region(range);
        }
    }

    info!("Remapping initial DRAM regions:\n");

    // Ensure that the initial DRAM region remains mapped
    let range = unsafe { slice::from_raw_parts(base, mapped) };
    idmap.map_range(range, rw_flags);

    // Ensure that the DT retains its global R/O mapping
    let ro_flags = Attributes::NORMAL.read_only();
    idmap.map_range(dtb, ro_flags);

    // Switch to the new ID map so we can use all of DRAM
    idmap.activate();

    let compat = ["qemu,fw-cfg-mmio"];
    let fwcfg_node = fdt
        .find_compatible(&compat)
        .expect("QEMU fwcfg node not found");

    info!("QEMU fwcfg node found: {}\n", fwcfg_node.name);

    let mut fwcfg = fwcfg::FwCfg::from_fdt_node(fwcfg_node)
        .expect("Failed to open fwcfg device");

    let (placement, randomized) =
        if let Some(seed) = rng::get_random_u64() {
            (Placement::Random(seed as u32, 0x20000), true)
        } else {
            (Placement::Aligned(0x20000), false)
        };

    let loadbuffer = {
        let mut buf: [u8; 256] = [0; 256];
        fwcfg.load_kernel_image(&mut buf).expect("Failed to load image header");

        let size = pecoff::Parser::get_image_size(&buf)
                    .expect("Failed to parse PE/COFF header");

        let buf = efi::memmap::allocate_pages(efi::memmap::size_to_pages(size),
                                              MemoryType::EfiLoaderCode,
                                              placement)
                    .expect("Failed to allocate memory for EFI program");

        fwcfg.load_kernel_image(buf).expect("Failed to load kernel image");
        buf
    };

    let pe_image = pecoff::Parser::from_slice(loadbuffer)
        .expect("Failed to parse PE/COFF image");

    // Clean the code region of the loaded image to the PoU so we
    // can safely fetch instructions from it once the PXN/UXN
    // attributes are cleared
    let code = pe_image.get_code_region();
    cmo::dcache_clean_to_pou(code);

    // Switch back to the initial ID map so we can remap
    // the loaded kernel image with different permissions
    idmap.deactivate();

    // Remap the text/rodata part of the image read-only so we will
    // be able to execute it with WXN protections enabled
    idmap.map_range(code, ro_flags.non_global());
    idmap.activate();

    unsafe { // use of extern statics is unsafe
        efi::memmap::declare_runtime_region(&RTSCODE);
        efi::memmap::allocate_region(&BSDATA, EfiBootServicesData).ok();
        efi::memmap::allocate_region(&RTSDATA, EfiRuntimeServicesData).ok();
    }

    let _initrd = efi::initrdloadfile2::new(&mut fwcfg);

    let mut li = efi::load_image(&pe_image, &cmdline, randomized);
    let ret = li.start_image();

    info!("EFI program exited with return value {:?}\n", ret);
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
    #[cfg(not(debug_assertions))]
    log::set_logger(&console::OUT)
        .map(|()| log::set_max_level(log::LevelFilter::Info)).ok();
    error!("{}\n", info);
    loop {}
}

global_asm!(include_str!("entry.s"));
global_asm!(include_str!("ttable.s"));
