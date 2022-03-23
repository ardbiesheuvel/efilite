// SPDX-License-Identifier: GPL-2.0
// Copyright 2022 Google LLC
// Authors: Ilias Apalodimas <ilias.apalodimas@linaro.org>
//          Ard Biesheuvel <ardb@google.com>

use crate::efi::guid;
use crate::efi::*;
use crate::efi::{Guid, Handle, new_handle, status::*};
use crate::rng;

use core::slice;

pub const EFI_RNG_PROTOCOL_GUID: Guid = guid!(
    0x3152bca5, 0xeade, 0x433d, [0x86, 0x2e, 0xc0, 0x1c, 0xdc, 0x29, 0x1f, 0x44]
);

type EfiRngAlgo = Guid;

// Don't describe the raw algorithm as the default, so that we can serve
// calls to the default RNG from RNDR as well, without knowing or having
// to specify what RNDR is backed by
const RNG_ALGORITHM_DEFAULT: EfiRngAlgo = guid!(
    0xb65fc704, 0x93b4, 0x4301, [0x90, 0xea, 0xa7, 0x5c, 0x33, 0x93, 0xb5, 0xe9]
);

const EFI_RNG_ALGORITHM_RAW: EfiRngAlgo = guid!(
    0xe43176d7, 0xb6e8, 0x4827, [0xb7, 0x84, 0x7f, 0xfd, 0xc4, 0xb6, 0x85, 0x61]
);

#[repr(C)]
pub struct EfiRng {
    get_info: GetInfo<Self>,
    get_rng: GetRNG<Self>,
    handle: Handle,
}

type GetInfo<T> =
    extern "C" fn( *mut T, *mut usize, *mut EfiRngAlgo) -> Status;

type GetRNG<T> =
    extern "C" fn(*mut T, *const EfiRngAlgo, usize, *mut u8) -> Status;

extern "C" fn get_info<T>(
    _this: *mut T,
    rng_algorithm_list_size: *mut usize,
    rng_algorithm_list: *mut EfiRngAlgo,
) -> Status {
    let len = unsafe { &mut *rng_algorithm_list_size };
    if *len < 2 {
        *len = 2;
        return Status::EFI_BUFFER_TOO_SMALL
    }
    let guids = unsafe {
        slice::from_raw_parts_mut(rng_algorithm_list, 2)
    };
    guids[0] = RNG_ALGORITHM_DEFAULT;
    guids[1] = EFI_RNG_ALGORITHM_RAW;
    *len = 2;
    Status::EFI_SUCCESS
}

extern "C" fn get_rng<T>(
    _this: *mut T,
    rng_algorithm: *const EfiRngAlgo,
    rng_value_length: usize,
    rng_value: *mut u8,
) -> Status {
    let output = unsafe {
        slice::from_raw_parts_mut(rng_value, rng_value_length)
    };
    let use_raw =
        !rng_algorithm.is_null() &&
        match unsafe { *rng_algorithm } {
            RNG_ALGORITHM_DEFAULT => false,
            EFI_RNG_ALGORITHM_RAW => true,
            _ => { return Status::EFI_UNSUPPORTED; }
        };

    // Use get_random_u64() if we can
    if !use_raw && output.len() <= core::mem::size_of::<u64>() {
        if let Some(l) = rng::get_random_u64() {
            output.copy_from_slice(&l.to_le_bytes().split_at(output.len()).0);
            return Status::EFI_SUCCESS
        }
    }

    if rng::get_entropy(output) {
        Status::EFI_SUCCESS
    } else {
        Status::EFI_NOT_READY
    }
}

pub fn new() -> EfiRng {
    let rng = EfiRng {
        get_info: get_info::<EfiRng>,
        get_rng: get_rng::<EfiRng>,
        handle: new_handle(),
    };
    install_protocol(rng.handle, &EFI_RNG_PROTOCOL_GUID, &rng);
    rng
}

impl Drop for EfiRng {
    fn drop(&mut self) {
        uninstall_protocol(self.handle, &EFI_RNG_PROTOCOL_GUID, &self);
    }
}
