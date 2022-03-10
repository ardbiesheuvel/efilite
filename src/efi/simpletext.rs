// SPDX-License-Identifier: GPL-2.0
// Copyright 2022 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

use crate::efi::guid;
use crate::efi::install_protocol;
use crate::efi::{Bool, Char16, Event, Guid, Handle, status::*};
use crate::console::OUT;

use core::ptr;

const EFI_SIMPLE_TEXT_INPUT_PROTOCOL_GUID: Guid = guid!(
    0x387477c1, 0x69c7, 0x11d2, [0x8e, 0x39, 0x0, 0xa0, 0xc9, 0x69, 0x72, 0x3b]
);

const EFI_SIMPLE_TEXT_OUTPUT_PROTOCOL_GUID: Guid = guid!(
    0x387477c2, 0x69c7, 0x11d2, [0x8e, 0x39, 0x0, 0xa0, 0xc9, 0x69, 0x72, 0x3b]
);

#[repr(C)]
struct KeyStroke {
    scan_code: u16,
    unicode_char: Char16,
}

#[repr(C)]
pub struct SimpleTextInput {
    reset: Reset<Self>,
    read_key_stroke: ReadKeyStroke,
    wait_for_key: Event,
}

type Reset<T> =
    extern "C" fn(
        _this: *mut T,
        _extended_verification: Bool,
    ) -> Status;

type ReadKeyStroke =
    extern "C" fn(
        _this: *mut SimpleTextInput,
        _key: *mut KeyStroke,
    ) -> Status;

#[repr(C)]
pub struct SimpleTextOutput {
    reset: Reset<Self>,
    output_string: OutputString,
    test_string: OutputString,
}

type OutputString =
    extern "C" fn(
        _this: *mut SimpleTextOutput,
        _string: *const Char16,
    ) -> Status;

extern "C" fn reset<T>(
    _this: *mut T,
    _extended_verification: Bool,
) -> Status {
    Status::EFI_SUCCESS
}

extern "C" fn read_key_stroke(
    _this: *mut SimpleTextInput,
    _key: *mut KeyStroke
) -> Status {
    Status::EFI_NOT_READY
}

static mut SIMPLE_TEXT_IN: SimpleTextInput = SimpleTextInput {
     reset: reset::<SimpleTextInput>,
     read_key_stroke: read_key_stroke,
     wait_for_key: ptr::null_mut(),
};

impl SimpleTextInput {
    pub fn get(handle: Handle) -> &'static Self {
        let inp = unsafe { &SIMPLE_TEXT_IN };
        install_protocol(handle, &EFI_SIMPLE_TEXT_INPUT_PROTOCOL_GUID, inp);
        inp
    }
}

extern "C" fn output_string(
    _this: *mut SimpleTextOutput,
    _string: *const Char16,
) -> Status {
    OUT.write_wchar_array(_string);
    Status::EFI_SUCCESS
}

extern "C" fn test_string(
    _this: *mut SimpleTextOutput,
    _string: *const Char16,
) -> Status {
    Status::EFI_SUCCESS
}

static mut SIMPLE_TEXT_OUT: SimpleTextOutput = SimpleTextOutput {
     reset: reset::<SimpleTextOutput>,
     output_string: output_string,
     test_string: test_string,
     // TODO add missing fields
};

impl SimpleTextOutput {
    pub fn get(handle: Handle) -> &'static Self {
        let out = unsafe { &SIMPLE_TEXT_OUT };
        install_protocol(handle, &EFI_SIMPLE_TEXT_OUTPUT_PROTOCOL_GUID, out);
        out
    }
}

