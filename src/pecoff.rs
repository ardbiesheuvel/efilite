// SPDX-License-Identifier: GPL-2.0
// Copyright 2022 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

use byte_slice_cast::*;

pub struct Parser<'a> {
    image: &'a[u8],
    code: &'a[u8],
    entrypoint: &'a[u8],
}

impl<'a> Parser<'a> {
    fn get_pe_header(slice: &'a[u8]) -> Option<&'a[u8]> {
        // Check that the image starts with the magic bytes 'MZ'
        if slice[0] != b'M' || slice[1] != b'Z' || slice.len() < 64 {
            return None
        }
        // Get the PE header offset from the MSDOS header
        let offset: usize =
            slice.get(60..=63).unwrap().as_slice_of::<u32>().unwrap()[0] as _;
        let pehdr = &slice.get(offset..).unwrap();
        if pehdr[0] != b'P' || pehdr[1] != b'E' || pehdr.len() < 84 {
            return None
        }
        Some(pehdr)
    }

    pub fn get_image_size(slice: &'a[u8]) -> Option<usize> {

        if let Some(pehdr) = Self::get_pe_header(slice) {
            let size_of_image: usize =
                pehdr.get(80..=83).unwrap().as_slice_of::<u32>().unwrap()[0] as _;
            Some(size_of_image)
        } else {
            None
        }
    }

    pub fn from_slice(slice: &'a[u8]) -> Option<Parser<'a>> {
        let pehdr = Self::get_pe_header(slice).unwrap();

        let base_of_code: usize =
            pehdr.get(44..=47).unwrap().as_slice_of::<u32>().unwrap()[0] as _;
        let size_of_code: usize =
            pehdr.get(28..=31).unwrap().as_slice_of::<u32>().unwrap()[0] as _;
        let entrypoint: usize =
            pehdr.get(40..=43).unwrap().as_slice_of::<u32>().unwrap()[0] as _;
        let size_of_image: usize =
            pehdr.get(80..=83).unwrap().as_slice_of::<u32>().unwrap()[0] as _;

        // Check that the various bounds are within the slice
        if base_of_code >= slice.len() || (base_of_code + size_of_code) > slice.len() ||
            entrypoint >= slice.len() || size_of_code == 0 ||
            size_of_image == 0 || size_of_image > slice.len() {
            return None;
        }

        Some(Parser::<'a> {
            image:
                &slice.get(0..=(size_of_image - 1)).unwrap(),
            entrypoint:
                &slice.get(entrypoint..entrypoint).unwrap(),
            code:
                &slice.get(base_of_code..=(base_of_code + size_of_code - 1)).unwrap(),
        })
    }

    pub fn get_image(&self) -> &'a[u8] {
        self.image
    }

    pub fn get_code_region(&self) -> &'a[u8] {
        self.code
    }

    pub fn get_entrypoint(&self) -> *const u8 {
        self.entrypoint as *const _ as *const u8
    }
}
