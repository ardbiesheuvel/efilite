// SPDX-License-Identifier: GPL-2.0
// Copyright 2022-2023 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

use alloc::{boxed::*, collections::*, vec::*};
use core::{cell::*, fmt, marker::*, mem::*, pin::pin, slice, str::from_utf8, sync::atomic::*};
use efiloader::*;
use efiloader::{memmap::*, memorytype::*};
use mmio::*;

struct FwCfgMmio {
    // read-only data register
    data: VolBox<u64, Allow, Deny>,

    // write-only selector register
    selector: VolBox<u16, Deny, Allow>,

    // write-only DMA register
    dmacontrol: VolBox<u64, Deny, Allow>,
}

pub struct FwCfg(RefCell<FwCfgMmio>);

// SAFETY: EFI boot services are single threaded, and FwCfg uses RefCells for interior mutability,
// which ensures that mutable references taken from the same thread will cause a panic. Such
// references can only be taken by the private API - no mutable FwCfg objects are exposed outside
// of it.
unsafe impl Sync for FwCfg {}

const CFG_KERNEL_SIZE: u16 = 0x08;
const CFG_KERNEL_DATA: u16 = 0x11;

const CFG_INITRD_SIZE: u16 = 0x0b;
const CFG_INITRD_DATA: u16 = 0x12;

const CFG_FILE_DIR: u16 = 0x19;

const CFG_DMACTL_DONE: u32 = 0;
const CFG_DMACTL_ERROR: u32 = 1;
const CFG_DMACTL_READ: u32 = 2;
const CFG_DMACTL_SKIP: u32 = 4;

#[repr(C)]
struct DmaTransfer {
    control: u32,
    length: u32,
    address: u64,
    pin: PhantomPinned,
}

type FwCfgFilename = [u8; 56];

#[derive(Copy, Clone)]
#[repr(C)]
struct FwCfgFile {
    _size: u32,
    _select: u16,
    reserved: u16,
    filename: FwCfgFilename,
}

impl FwCfgFile {
    pub fn size(&self) -> usize {
        u32::to_be(self._size) as usize
    }

    pub fn select(&self) -> u16 {
        u16::to_be(self._select)
    }
}

struct FwCfgFileIterator<'a, T> {
    count: u32,
    next: u32,
    select: u16,
    offset: u32,
    fwcfg: &'a FwCfg,
    phantom: PhantomData<T>,
}

impl<'a, T> FwCfgFileIterator<'a, T> {
    pub fn to_vec(&self) -> Option<Vec<T>> {
        let len = self.count as usize;
        let mut v = Vec::<T>::with_capacity(len);
        let size = len * size_of::<T>();
        let mut mmio = self.fwcfg.0.borrow_mut();

        mmio.selector.write(u16::to_be(self.select));
        fence(Ordering::Release);

        if self.offset > 0 {
            mmio.dma_transfer(0, self.offset as u32).ok()?;
        }
        mmio.dma_transfer(v.as_mut_ptr() as u64, size as u32).ok()?;
        unsafe {
            v.set_len(len);
        }
        Some(v)
    }
}

impl<T: Copy> Iterator for FwCfgFileIterator<'_, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next == self.count {
            return None;
        }

        struct PinnedItem<T> {
            item: MaybeUninit<T>,
            _pin: PhantomPinned,
        }

        let itemsz = size_of::<Self::Item>() as u32;
        let offset = self.offset + self.next * itemsz;
        let out = pin!(PinnedItem {
            item: MaybeUninit::<T>::uninit(),
            _pin: PhantomPinned
        });
        let mut mmio = self.fwcfg.0.borrow_mut();

        mmio.selector.write(u16::to_be(self.select));
        fence(Ordering::Release);

        if offset > 0 {
            mmio.dma_transfer(0, offset as u32).ok()?;
        }
        mmio.dma_transfer(&out.item as *const _ as u64, itemsz as u32)
            .ok()?;
        self.next += 1;

        unsafe { Some(out.item.assume_init()) }
    }
}

impl FwCfgMmio {
    fn new(addr: *const u8) -> FwCfgMmio {
        unsafe {
            FwCfgMmio {
                data: VolBox::<u64, Allow, Deny>::new(addr as *mut u64),
                selector: VolBox::<u16, Deny, Allow>::new(addr.offset(8) as *mut u16),
                dmacontrol: VolBox::<u64, Deny, Allow>::new(addr.offset(16) as *mut u64),
            }
        }
    }

    fn dma_transfer(&mut self, addr: u64, size: u32) -> Result<(), ()> {
        let control = match addr {
            0 => CFG_DMACTL_SKIP,
            _ => CFG_DMACTL_READ,
        };

        let xfer = pin!(DmaTransfer {
            control: u32::to_be(control),
            length: u32::to_be(size),
            address: u64::to_be(addr),
            pin: PhantomPinned,
        });

        self.dmacontrol.write(u64::to_be(&*xfer as *const _ as u64));
        fence(Ordering::Release);

        loop {
            match unsafe { core::ptr::read_volatile(&xfer.control) } {
                CFG_DMACTL_DONE => return Ok(()),
                CFG_DMACTL_ERROR => return Err(()),
                _ => fence(Ordering::AcqRel), // keep polling
            }
        }
    }
}

#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub struct FwCfgLoaderAllocate {
    pub filename: FwCfgFilename,
    pub alignment: u32,
    pub zone: u8,
}

#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub struct FwCfgLoaderAddPointer {
    pub pointer: FwCfgFilename,
    pub pointee: FwCfgFilename,
    pub offset: u32,
    pub size: u8,
}

#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub struct FwCfgLoaderAddChecksum {
    pub filename: FwCfgFilename,
    pub result_offset: u32,
    pub start: u32,
    pub size: u32,
}

#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub struct FwCfgLoaderWritePointer {
    pub pointer: FwCfgFilename,
    pub pointee: FwCfgFilename,
    pub pointer_offset: u32,
    pub pointee_offset: u32,
    pub size: u8,
}

#[repr(C)]
pub union FwCfgLoaderUnion {
    pub allocate: FwCfgLoaderAllocate,
    pub add_pointer: FwCfgLoaderAddPointer,
    pub add_checksum: FwCfgLoaderAddChecksum,
    pub write_pointer: FwCfgLoaderWritePointer,
}

#[derive(Debug)]
#[allow(dead_code)]
#[repr(u32)]
pub enum FwCfgLoaderCmdType {
    FwCfgLoaderCmdUnused,
    FwCfgLoaderCmdAllocate,
    FwCfgLoaderCmdAddPointer,
    FwCfgLoaderCmdAddChecksum,
    FwCfgLoaderCmdWritePointer,
}

#[repr(C)]
pub struct FwCfgLoaderEntry {
    pub _type: FwCfgLoaderCmdType,
    pub u: FwCfgLoaderUnion,
}

impl fmt::Debug for FwCfgLoaderEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = f.debug_struct("FwCfgLoaderEntry");

        match self._type {
            FwCfgLoaderCmdType::FwCfgLoaderCmdAllocate => unsafe {
                out.field("u", &self.u.allocate);
            },
            FwCfgLoaderCmdType::FwCfgLoaderCmdAddPointer => unsafe {
                out.field("u", &self.u.add_pointer);
            },
            FwCfgLoaderCmdType::FwCfgLoaderCmdAddChecksum => unsafe {
                out.field("u", &self.u.add_checksum);
            },
            FwCfgLoaderCmdType::FwCfgLoaderCmdWritePointer => unsafe {
                out.field("u", &self.u.write_pointer);
            },
            _ => (),
        };
        out.finish()
    }
}

impl FwCfg {
    fn attach(addr: *const u8) -> &'static FwCfg {
        Box::leak(Box::new(FwCfg(RefCell::new(FwCfgMmio::new(addr)))))
    }

    fn files(&self) -> FwCfgFileIterator<FwCfgFile> {
        let size = u32::to_be(self.get_file_size(CFG_FILE_DIR) as u32);
        FwCfgFileIterator {
            count: size,
            next: 0,
            select: CFG_FILE_DIR,
            offset: size_of::<u32>() as u32,
            fwcfg: self,
            phantom: PhantomData,
        }
    }

    fn loader(&self) -> Option<Vec<FwCfgLoaderEntry>> {
        let f = self.files().find(|f| {
            from_utf8(&f.filename).map_or(false, |s| s.trim_end_matches("\0") == "etc/table-loader")
        })?;
        FwCfgFileIterator {
            count: (f.size() / size_of::<FwCfgLoaderEntry>()) as u32,
            next: 0,
            select: f.select(),
            offset: 0,
            fwcfg: self,
            phantom: PhantomData,
        }
        .to_vec()
    }

    pub fn from_fdt_node(node: fdt::node::FdtNode) -> Option<&'static FwCfg> {
        let addr = node.reg()?.nth(0)?.starting_address;
        Some(Self::attach(addr))
    }

    fn dma_read<T: Copy>(
        &self,
        loadbuffer: &mut [MaybeUninit<T>],
        offset: usize,
        size: usize,
        config_item: u16,
    ) -> Result<(), ()> {
        let mut mmio = self.0.borrow_mut();
        mmio.selector.write(u16::to_be(config_item));
        fence(Ordering::Release);

        if offset > 0 {
            mmio.dma_transfer(0, offset as u32).or(Err(()))?;
        }
        mmio.dma_transfer(loadbuffer.as_ptr() as u64, size as u32)
    }

    fn get_file_size(&self, size_cfg: u16) -> usize {
        let mut mmio = self.0.borrow_mut();
        mmio.selector.write(u16::to_be(size_cfg));
        fence(Ordering::Release);
        mmio.data.read() as usize
    }

    fn load_file<'a, T: Copy>(
        &self,
        loadbuffer: &'a mut [MaybeUninit<T>],
        offset: usize,
        size: usize,
        data_cfg: u16,
    ) -> Result<&'a [T], ()> {
        let size = size.min(loadbuffer.len() / size_of::<T>());
        self.dma_read(loadbuffer, offset, size, data_cfg)?;
        if size < loadbuffer.len() {
            loadbuffer.split_at_mut(size).1.fill(MaybeUninit::zeroed());
        }
        unsafe {
            Ok(slice::from_raw_parts(
                loadbuffer.as_ptr() as *const _,
                loadbuffer.len(),
            ))
        }
    }

    fn load_file_mut<'a>(
        &self,
        loadbuffer: &'a mut [MaybeUninit<u8>],
        offset: usize,
        size: usize,
        data_cfg: u16,
    ) -> Result<&'a mut [u8], &'static str> {
        let size = size.min(loadbuffer.len());
        self.dma_read(loadbuffer, offset, size, data_cfg)
            .or(Err("DMA read failed"))?;
        if size < loadbuffer.len() {
            loadbuffer.split_at_mut(size).1.fill(MaybeUninit::zeroed());
        }
        unsafe {
            Ok(slice::from_raw_parts_mut(
                loadbuffer.as_ptr() as *mut _,
                loadbuffer.len(),
            ))
        }
    }

    fn get_loader(
        &self,
        size_cfg: u16,
        data_cfg: u16,
        preload_bytes: usize,
    ) -> Option<FwCfgFileLoader> {
        let size = self.get_file_size(size_cfg);
        if size == 0 {
            return None;
        }
        Some(FwCfgFileLoader::new(size, data_cfg, self, preload_bytes))
    }

    pub fn get_kernel_loader(&self) -> Option<FwCfgFileLoader> {
        // Cache the first 1k of the image to ease random access to the PE header
        self.get_loader(CFG_KERNEL_SIZE, CFG_KERNEL_DATA, 1024)
    }

    pub fn get_initrd_loader(&self) -> Option<FwCfgFileLoader> {
        self.get_loader(CFG_INITRD_SIZE, CFG_INITRD_DATA, 0)
    }

    pub fn load_firmware_tables<'a>(&self, efi: &'a EfiContext) -> Result<*const u8, &'static str> {
        FwCfgTableLoader::new(self, efi).load_firmware_tables()
    }

    pub fn load_smbios_tables<'a>(&self, efi: &'a EfiContext) -> Result<*const u8, &'static str> {
        let v: Vec<_> = self
            .files()
            .filter_map(|f| {
                from_utf8(&f.filename).map_or(None, |s| match s.trim_end_matches("\0") {
                    "etc/smbios/smbios-anchor" => Some((0, f)),
                    "etc/smbios/smbios-tables" => Some((1, f)),
                    _ => None,
                })
            })
            .collect::<BTreeMap<_, _>>()
            .into_values()
            .collect();
        if v.len() < 2 {
            return Err("No SMBIOS tables available");
        }

        if v[0].size() < 24 {
            return Err("Unexpected anchor type");
        }

        let b = efi
            .allocate_pages(
                memmap::size_to_pages(v[0].size() + v[1].size()),
                EfiMemoryType::EfiACPIReclaimMemory,
                Placement::Anywhere,
            )
            .ok_or("Failed to allocate blob memory")?;

        let (a, t) = b.split_at_mut(v[0].size());
        let a = self.load_file_mut(a, 0, v[0].size(), v[0].select())?;
        let t = self.load_file_mut(t, 0, v[1].size(), v[1].select())?;

        // SAFETY: a[] is page aligned and at least 24 bytes in size so writing
        // 8 bytes at offset #16 is safe.
        unsafe {
            // Point the address field at offset #16 in the anchor to the table blob
            let p = a.as_mut_ptr().offset(16) as *mut *const u8;
            *p = t.as_ptr();
        }

        let size = a[6] as usize;
        let mut checksum = 0u8;

        for c in &a[..size] {
            checksum = checksum.wrapping_sub(*c);
        }
        a[5] = checksum;

        Ok(a.as_ptr() as *const u8)
    }
}

struct FwCfgTableLoader<'a> {
    loaded_tables: BTreeMap<FwCfgFilename, &'a mut [u8]>,
    fwcfg: &'a FwCfg,
    efi: &'a EfiContext,
}

impl<'a> FwCfgTableLoader<'a> {
    pub fn new(fwcfg: &'a FwCfg, efi: &'a EfiContext) -> Self {
        FwCfgTableLoader {
            loaded_tables: BTreeMap::new(),
            fwcfg: fwcfg,
            efi: efi,
        }
    }

    fn allocate(&mut self, allocate: &FwCfgLoaderAllocate) -> Result<(), &'static str> {
        let f = self
            .fwcfg
            .files()
            .find(|f| f.filename == allocate.filename)
            .ok_or("Failed to locate blob file")?;
        let b = self
            .efi
            .allocate_pages(
                memmap::size_to_pages(f.size()),
                EfiMemoryType::EfiACPIReclaimMemory,
                Placement::Anywhere,
            )
            .ok_or("Failed to allocate blob memory")?;
        let b = self.fwcfg.load_file_mut(b, 0, f.size(), f.select())?;
        self.loaded_tables.insert(f.filename, b);
        Ok(())
    }

    fn add_pointer(&mut self, add_pointer: &FwCfgLoaderAddPointer) -> Result<(), &'static str> {
        let tables = &mut self.loaded_tables;
        let addend = tables
            .get(&add_pointer.pointee)
            .ok_or("Unknown pointee blob")?
            .as_ptr();
        let pointer = tables
            .get_mut(&add_pointer.pointer)
            .ok_or("Unknown pointer blob")?;
        let offset = add_pointer.offset as usize;

        match add_pointer.size {
            1 => add_value_at_offset(addend as u8, offset, pointer),
            2 => add_value_at_offset(addend as u16, offset, pointer),
            4 => add_value_at_offset(addend as u32, offset, pointer),
            8 => add_value_at_offset(addend as u64, offset, pointer),
            _ => (),
        }
        Ok(())
    }

    fn add_checksum(&mut self, add_checksum: &FwCfgLoaderAddChecksum) -> Result<(), &'static str> {
        let tables = &mut self.loaded_tables;
        let start = add_checksum.start as usize;
        let end = start + add_checksum.size as usize;
        let offset = add_checksum.result_offset as usize;
        let table = tables
            .get_mut(&add_checksum.filename)
            .ok_or("Unknown blob for checksum")?;
        let mut checksum = 0u8;
        table[start..end]
            .iter()
            .for_each(|&c| checksum = checksum.wrapping_sub(c));
        table[offset] = checksum;
        Ok(())
    }

    pub fn load_firmware_tables(&mut self) -> Result<*const u8, &'static str> {
        for entry in self.fwcfg.loader().ok_or("Failed to access table loader")? {
            match entry._type {
                FwCfgLoaderCmdType::FwCfgLoaderCmdAllocate => unsafe {
                    self.allocate(&entry.u.allocate)
                },
                FwCfgLoaderCmdType::FwCfgLoaderCmdAddPointer => unsafe {
                    self.add_pointer(&entry.u.add_pointer)
                },
                FwCfgLoaderCmdType::FwCfgLoaderCmdAddChecksum => unsafe {
                    self.add_checksum(&entry.u.add_checksum)
                },
                //FwCfgLoaderCmdType::FwCfgLoaderCmdWritePointer => unsafe {
                //},
                _ => Err("Unsupported table loader command"),
            }?;
        }
        let rsdp = self
            .loaded_tables
            .iter()
            .find(|(&k, &ref _v)| {
                from_utf8(&k).map_or(false, |s| s.trim_end_matches("\0") == "etc/acpi/rsdp")
            })
            .ok_or("Failed to locate RSDP table")?
            .1;
        Ok(rsdp.as_ptr())
    }
}

trait LeBytes<T, const N: usize> {
    fn from_le_bytes(val: &[u8; N]) -> T;
    fn to_le_bytes(val: T) -> [u8; N];
}

macro_rules! le_bytes {
    ($a: ty) => {
        impl LeBytes<$a, { size_of::<$a>() }> for $a {
            fn from_le_bytes(val: &[u8; { size_of::<$a>() }]) -> $a {
                <$a>::from_le_bytes(*val)
            }
            fn to_le_bytes(val: $a) -> [u8; { size_of::<$a>() }] {
                <$a>::to_le_bytes(val)
            }
        }
    };
}

le_bytes!(u8);
le_bytes!(u16);
le_bytes!(u32);
le_bytes!(u64);

fn add_value_at_offset<T: LeBytes<T, N> + core::ops::Add<Output = T>, const N: usize>(
    addend: T,
    offset: usize,
    buf: &mut [u8],
) {
    let end = offset + N;
    let val: T = T::from_le_bytes(buf[offset..end].try_into().unwrap()) + addend;
    buf[offset..end].copy_from_slice(&T::to_le_bytes(val));
}

pub struct FwCfgFileLoader<'a> {
    size: usize,
    data_cfg: u16,
    fwcfg: &'a FwCfg,
    preload: Box<[u8]>,
}

impl<'a> FwCfgFileLoader<'a> {
    fn new(size: usize, data_cfg: u16, fwcfg: &'a FwCfg, preload_size: usize) -> FwCfgFileLoader {
        let preload_size = size.min(preload_size);
        let mut preload = Vec::<u8>::new();
        if preload_size > 0 {
            let mut buf = Vec::<MaybeUninit<u8>>::new();
            buf.resize(preload_size, MaybeUninit::uninit());

            if let Ok(ret) = fwcfg.load_file(buf.as_mut_slice(), 0, preload_size, data_cfg) {
                preload = ret.to_vec();
            }
        }

        FwCfgFileLoader {
            size: size,
            data_cfg: data_cfg,
            fwcfg: fwcfg,
            preload: preload.into_boxed_slice(),
        }
    }
}

impl efiloader::FileLoader for FwCfgFileLoader<'_> {
    fn get_size(&self) -> usize {
        self.size
    }

    fn load_file<'a>(&self, loadbuffer: &'a mut [MaybeUninit<u8>]) -> Result<&'a [u8], &str> {
        self.fwcfg
            .load_file(loadbuffer, 0, self.size, self.data_cfg)
            .or(Err("Failed to load file from fwcfg"))
    }

    unsafe fn load_range<'a>(&self, ptr: *mut (), offset: usize, size: usize) -> Result<(), &str> {
        if offset > self.size {
            return Err("Offset out of range");
        }
        if offset + size <= self.preload.len() {
            log::trace!("Reading from preload vector {:?}\n", offset..offset + size);
            let p = self.preload.as_ptr();
            core::ptr::copy(p.offset(offset as isize), ptr as *mut u8, size);
            return Ok(());
        }
        let loadbuffer = slice::from_raw_parts_mut(ptr as *mut MaybeUninit<u8>, size);
        self.fwcfg
            .load_file(loadbuffer, offset, size, self.data_cfg)
            .or(Err("Failed to load range from fwcfg"))
            .and(Ok(()))
    }
}
