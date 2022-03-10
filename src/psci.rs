// SPDX-License-Identifier: GPL-2.0
// Copyright 2022 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

use core::arch::asm;

const PSCI_SYSTEM_OFF: u32 = 0x84000008;
const PSCI_SYSTEM_RESET: u32 = 0x84000009;

fn psci_call(fid: u32) {
    unsafe { asm!("hvc #0", in("x0") fid); }
}

pub fn poweroff() -> ! {
    psci_call(PSCI_SYSTEM_OFF);
    loop {}
}

pub fn reboot() -> ! {
    psci_call(PSCI_SYSTEM_RESET);
    loop {}
}
