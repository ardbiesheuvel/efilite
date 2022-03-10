// SPDX-License-Identifier: GPL-2.0
// Copyright 2022 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

use mmio::{Allow, Deny, VolBox};
use crate::initrd;

pub struct FwCfg {
    // read-only data register
    data: VolBox<u64, Allow, Deny>,

    // write-only selector register
    selector: VolBox<u16, Deny, Allow>,

    // write-only DMA register
    dmacontrol: VolBox<u64, Deny, Allow>,
}

const CFG_KERNEL_SIZE: u16 = 0x08;
const CFG_KERNEL_DATA: u16 = 0x11;

const CFG_INITRD_SIZE: u16 = 0x0b;
const CFG_INITRD_DATA: u16 = 0x12;

const CFG_DMACTL_DONE: u32 = 0;
const CFG_DMACTL_ERROR: u32 = 1;
const CFG_DMACTL_READ: u32 = 2;

#[repr(C)]
struct DmaTransfer {
    control: u32,
    length: u32,
    address: u64,
}

impl FwCfg {
    pub fn from_fdt_node(node: fdt::node::FdtNode) -> Option<FwCfg> {
        if let Some(iter) = node.reg() {
            iter.last().map(|reg| {
                let addr = reg.starting_address;
                unsafe {
                    FwCfg {
                        data: VolBox::<u64, Allow, Deny>::new(addr as *mut u64),
                        selector: VolBox::<u16, Deny, Allow>::new(addr.offset(8) as *mut u16),
                        dmacontrol: VolBox::<u64, Deny, Allow>::new(addr.offset(16) as *mut u64),
                    }
                }
            })
        } else {
            None
        }
    }

    fn dma_transfer(
        &mut self,
        loadbuffer: &mut [u8],
        size: usize,
        config_item: u16,
    ) -> Result<(), &str> {
        let addr = loadbuffer.as_ptr() as u64;
        let xfer = DmaTransfer {
            control: u32::to_be(CFG_DMACTL_READ),
            length: u32::to_be(size as u32),
            address: u64::to_be(addr),
        };
        self.selector.write(u16::to_be(config_item));
        self.dmacontrol.write(u64::to_be(&xfer as *const _ as u64));

        unsafe {
            let control =
                VolBox::<u32, Allow, Deny>::new(&xfer.control as *const _ as *mut u32);
            loop {
                match control.read() {
                    CFG_DMACTL_DONE => return Ok(()),
                    CFG_DMACTL_ERROR => return Err("fwcfg DMA error"),
                    _ => (), // keep polling
                }
            }
        }
    }

    pub fn get_kernel_size(&mut self) -> usize {
        self.selector.write(u16::to_be(CFG_KERNEL_SIZE));
        self.data.read() as usize
    }

    pub fn load_kernel_image(&mut self, loadbuffer: &mut [u8]) -> Result<(), &str> {
        let size = self.get_kernel_size();
        if size > 0 {
            self.dma_transfer(loadbuffer,
                              core::cmp::min(size, loadbuffer.len()),
                              CFG_KERNEL_DATA)
        } else {
            Err("No kernel image provided by fwcfg")
        }
    }
}

impl initrd::InitrdLoader for FwCfg {
    fn get_size(&mut self) -> usize {
        self.selector.write(u16::to_be(CFG_INITRD_SIZE));
        self.data.read() as usize
    }

    fn load_initrd_image(&mut self, loadbuffer: &mut[u8]) -> Result<(), &str> {
        let size = self.get_size();
        if size > 0 {
            self.dma_transfer(loadbuffer,
                              core::cmp::min(size, loadbuffer.len()),
                              CFG_INITRD_DATA)
        } else {
            Err("No initrd image provided by fwcfg")
        }
    }
}
