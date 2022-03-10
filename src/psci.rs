// SPDX-License-Identifier: GPL-2.0
// Copyright 2022-2023 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

use efiloader::runtimeservices::ResetType;
use efiloader::status::Status;

use core::arch::asm;

const PSCI_SYSTEM_OFF: u32 = 0x84000008;
const PSCI_SYSTEM_RESET: u32 = 0x84000009;

fn psci_call(fid: u32) {
    unsafe {
        asm!("hvc #0", in("x0") fid);
    }
}

fn poweroff() -> ! {
    psci_call(PSCI_SYSTEM_OFF);
    loop {}
}

fn reboot() -> ! {
    psci_call(PSCI_SYSTEM_RESET);
    loop {}
}

pub extern "efiapi" fn reset_system(
    _reset_type: ResetType,
    _reset_status: Status,
    _data_size: usize,
    _reset_data: *const (),
) -> Status {
    match _reset_type {
        ResetType::EfiResetShutdown => poweroff(),
        _ => reboot(),
    }
}
