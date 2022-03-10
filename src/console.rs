// SPDX-License-Identifier: GPL-2.0
// Copyright 2022 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

use core::fmt::Write;
use log::{Level, Metadata, Record};
use mmio::{Allow, Deny, VolBox};

pub struct QemuSerialConsole {
    base: u64,
}

struct QemuSerialConsoleWriter<'a> {
    console: &'a QemuSerialConsole,
}

impl QemuSerialConsole {
    fn puts(&self, s: &str) {
        //
        // This is technically racy, as nothing is preventing concurrent accesses to the UART if we
        // model it this way. However, this is a debug tool only, and we never read back the
        // register value so any races cannot have any observeable side effects to the program
        // itself.
        //
        let mut out = unsafe { VolBox::<u32, Deny, Allow>::new(self.base as *mut u32) };

        for b in s.as_bytes().iter() {
            if *b == b'\n' {
                out.write(b'\r' as u32);
            }
            out.write(*b as u32)
        }
    }

    pub fn write_wchar_array(&self, s: *const u16) {
        let mut out = unsafe { VolBox::<u32, Deny, Allow>::new(self.base as *mut u32) };

        let mut offset: isize = 0;
        loop {
            match unsafe { *s.offset(offset) } {
                0 => break,
                0x80.. => (),
                w => {
                    if w == b'\n' as u16 {
                        out.write(b'\r' as u32);
                    }
                    out.write(w as u32);
                }
            }
            offset += 1;
        }
    }
}

impl Write for QemuSerialConsoleWriter<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.console.puts(s);
        Ok(())
    }
}

impl log::Log for QemuSerialConsole {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let mut out = QemuSerialConsoleWriter { console: &self };
            write!(&mut out, "efilite {} - {}", record.level(), record.args()).unwrap();
        }
    }

    fn flush(&self) {}
}

// The primary UART of QEMU's mach-virt
pub static OUT: QemuSerialConsole = QemuSerialConsole { base: 0x900_0000 };
