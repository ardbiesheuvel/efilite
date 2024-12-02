// SPDX-License-Identifier: GPL-2.0
// Copyright 2022-2023 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

use core::cell::RefCell;
use core::fmt::Write;
use core::ops::Range;
use fdt::node::FdtNode;
use log::{Metadata, Record};
use mmio::{Allow, Deny, VolBox};
use once_cell::unsync::OnceCell;

struct DumbSerialConsoleWriter(VolBox<u32, Deny, Allow>);

impl Write for DumbSerialConsoleWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let out = &mut self.0;

        for b in s.as_bytes().iter() {
            if *b == b'\n' {
                out.write(b'\r' as u32);
            }
            out.write(*b as u32)
        }
        Ok(())
    }
}

pub struct DumbSerialConsole {
    pub base: usize,
    out: RefCell<DumbSerialConsoleWriter>,
}

// SAFETY: DumbSerialConsole is only accessible via shared references, and its interior mutability
// is implemented using a RefCell. EFI boot services are single threaded, and the only way we might
// enter recursively is when a panic is triggered by a write to the log, which is why the log::Log
// implementation uses try_borrow_mut().
unsafe impl Sync for DumbSerialConsole {}

pub fn init(base: &Range<usize>) -> &'static DumbSerialConsole {
    // SAFETY: the code is single threaded and does not recurse, so the first invocation will
    // run to completion before this code is ever executed again.
    unsafe {
        // Statically allocated so we can init the console before the heap
        static mut CON: OnceCell<DumbSerialConsole> = OnceCell::new();

        let v = VolBox::<u32, Deny, Allow>::new(base.start as *mut u32);
        let c = &raw mut CON;
        (*c).get_or_init(|| DumbSerialConsole {
            base: base.start,
            out: RefCell::new(DumbSerialConsoleWriter(v)),
        })
    }
}

pub fn init_from_fdt_node(node: FdtNode) -> Option<&'static DumbSerialConsole> {
    let reg = node.reg()?.nth(0)?;
    let base = reg.starting_address as usize;
    let size = reg.size?;
    Some(init(&(base..base + size)))
}

impl efiloader::SimpleConsole for DumbSerialConsole {
    fn write_string(&self, s: &str) {
        self.out.borrow_mut().write_str(s).ok();
    }

    fn read_byte(&self) -> Option<u8> {
        None
    }
}

impl log::Log for DumbSerialConsole {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            if let Ok(mut out) = self.out.try_borrow_mut() {
                write!(out, "efilite {} - {}", record.level(), record.args()).ok();
            }
        }
    }

    fn flush(&self) {}
}
