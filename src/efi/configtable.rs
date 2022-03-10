// SPDX-License-Identifier: GPL-2.0
// Copyright 2022 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

use crate::efi::Guid;

#[derive(Clone, Copy)]
#[repr(C)]
pub struct ConfigurationTable {
    pub vendor_guid: Guid,
    pub vendor_table: *const (),
}
