// SPDX-License-Identifier: GPL-2.0
// Copyright 2022 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

pub trait InitrdLoader {
    fn get_size(&mut self) -> usize;

    fn load_initrd_image(&mut self, loadbuffer: &mut[u8]) -> Result<(), &str>;
}


