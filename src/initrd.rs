// SPDX-License-Identifier: GPL-2.0
// Copyright 2022-2023 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

use core::mem::MaybeUninit;

pub trait InitrdLoader {
    fn get_size(&mut self) -> usize;

    fn load_initrd_image<'a>(
        &mut self,
        loadbuffer: &'a mut [MaybeUninit<u8>],
    ) -> Result<&'a [u8], &str>;
}
