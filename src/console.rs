// SPDX-License-Identifier: GPL-2.0
// Copyright 2022-2023 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

use core::fmt::Write;
use core::mem::MaybeUninit;
use core::ops::Range;
use fdt::node::FdtNode;
use log::{Metadata, Record};
use mmio::{Allow, Deny, VolBox};
use spinning_top::Spinlock;

pub struct DumbSerialConsole {
    pub base: usize,
    out: Spinlock<VolBox<u32, Deny, Allow>>,
}

struct DumbSerialConsoleWriter<'a> {
    console: &'a DumbSerialConsole,
}

pub fn init(base: &Range<usize>) -> &'static DumbSerialConsole {
    // Statically allocated so we can init the console before the heap
    static mut CON: MaybeUninit<DumbSerialConsole> = MaybeUninit::uninit();

    unsafe {
        let v = VolBox::<u32, Deny, Allow>::new(base.start as *mut u32);
        CON.write(DumbSerialConsole {
            base: base.start,
            out: Spinlock::new(v),
        })
    }
}

pub fn init_from_fdt_node(node: FdtNode) -> Option<&'static DumbSerialConsole> {
    let reg = node.reg()?.nth(0)?;
    let base = reg.starting_address as usize;
    let size = reg.size?;
    Some(init(&(base..base + size)))
}

impl DumbSerialConsole {
    fn puts(&self, s: &str) {
        let mut out = self.out.lock();

        for b in s.as_bytes().iter() {
            if *b == b'\n' {
                out.write(b'\r' as u32);
            }
            out.write(*b as u32)
        }
    }
}

impl efiloader::SimpleConsole for DumbSerialConsole {
    fn write_string(&self, s: &str) {
        self.puts(s)
    }

    fn read_byte(&self) -> Option<u8> {
        None
    }
}

impl Write for DumbSerialConsoleWriter<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        Ok(self.console.puts(s))
    }
}

impl log::Log for DumbSerialConsole {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let mut out = DumbSerialConsoleWriter { console: &self };
            write!(&mut out, "efilite {} - {}", record.level(), record.args()).ok();
        }
    }

    fn flush(&self) {}
}
