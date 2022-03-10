// SPDX-License-Identifier: GPL-2.0
// Copyright 2022 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

#[no_mangle]
pub extern "C" fn bcmp(s1: *const (), s2: *const (), len: usize) -> i32 {
    memcmp(s1, s2, len)
}

#[no_mangle]
pub extern "C" fn memcmp(s1: *const (), s2: *const (), len: usize) -> i32 {
    let (mut s1, mut s2) = unsafe {(
        core::slice::from_raw_parts(s1 as *mut u8, len),
        core::slice::from_raw_parts(s2 as *const u8, len)
    )};

    while let Some((c1, rem)) = s1.split_first() {
        if let Some((c2, rem)) = s2.split_first() {
            if *c1 != *c2 {
                return *c1 as i32 - *c2 as i32;
            }
            s2 = rem;
        }
        s1 = rem;
    }
    0
}

#[no_mangle]
pub extern "C" fn memset(s: *mut (), c: i32, n: usize) -> *mut () {
    let mut dst = unsafe {
        core::slice::from_raw_parts_mut(s as *mut u8, n)
    };

    while let Some((d, rem)) = dst.split_first_mut() {
        *d = c as u8;
        dst = rem;
    }
    s
}

#[no_mangle]
pub extern "C" fn memcpy(dest: *mut (), src: *const (), n: usize) -> *mut () {
    let (mut dst, mut src) = unsafe {(
        core::slice::from_raw_parts_mut(dest as *mut u8, n),
        core::slice::from_raw_parts(src as *const u8, n)
    )};

    while let Some((d, rem)) = dst.split_first_mut() {
        if let Some((s, rem)) = src.split_first() {
            *d = *s;
            src = rem;
        }
        dst = rem;
    }
    dest
}

#[no_mangle]
pub extern "C" fn memmove(dest: *mut (), src: *const (), n: usize) -> *mut () {
    if (dest as usize) < (src as usize) ||
        (dest as usize) >= (src as usize) + n {
        return memcpy(dest, src, n);
    }

    let (mut dst, mut src) = unsafe {(
        core::slice::from_raw_parts_mut(dest as *mut u8, n),
        core::slice::from_raw_parts(src as *const u8, n)
    )};

    while let Some((d, rem)) = dst.split_last_mut() {
        if let Some((s, rem)) = src.split_last() {
            *d = *s;
            src = rem;
        }
        dst = rem;
    }
    dest
}
