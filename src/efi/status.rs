// SPDX-License-Identifier: GPL-2.0
// Copyright 2022 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

const EFI_ERROR_BASE: usize = isize::MAX as usize + 1;

#[allow(non_camel_case_types)]
#[allow(dead_code)]
#[derive(Debug)]
#[repr(usize)]
pub enum Status {
  EFI_SUCCESS            =  0,
  EFI_LOAD_ERROR         =  1 + EFI_ERROR_BASE,
  EFI_INVALID_PARAMETER  =  2 + EFI_ERROR_BASE,
  EFI_UNSUPPORTED        =  3 + EFI_ERROR_BASE,
  EFI_BAD_BUFFER_SIZE    =  4 + EFI_ERROR_BASE,
  EFI_BUFFER_TOO_SMALL   =  5 + EFI_ERROR_BASE,
  EFI_NOT_READY          =  6 + EFI_ERROR_BASE,
  EFI_DEVICE_ERROR       =  7 + EFI_ERROR_BASE,
  EFI_WRITE_PROTECTED    =  8 + EFI_ERROR_BASE,
  EFI_OUT_OF_RESOURCES   =  9 + EFI_ERROR_BASE,
  EFI_NOT_FOUND          = 14 + EFI_ERROR_BASE,
  EFI_TIMEOUT            = 18 + EFI_ERROR_BASE,
  EFI_ABORTED            = 21 + EFI_ERROR_BASE,
  EFI_SECURITY_VIOLATION = 26 + EFI_ERROR_BASE,
}
