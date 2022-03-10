// SPDX-License-Identifier: GPL-2.0
// Copyright 2022 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

use core::arch::asm;

const PSCI_1_0_PSCI_VERSION: u64 = 0x84000000;
const PSCI_1_0_PSCI_FEATURES: u64 = 0x8400000a;

const ARM_SMCCC_VERSION: u64 = 0x80000000;
const ARM_SMCCC_TRNG_RND64: u64 = 0xc4000053;
const MAX_BITS_PER_CALL: usize = 192;

fn hvc_call(fid: u64, arg: u64) -> u64 {
    let mut ret: u64;
    unsafe {
        asm!("hvc   #0",
             in("x0") fid,
             in("x1") arg,

             lateout("x0") ret,
             lateout("x1") _,
             lateout("x2") _,
             lateout("x3") _,
             options(nomem, nostack, preserves_flags),
        );
    }
    ret
}

fn have_smccc() -> bool {
    hvc_call(PSCI_1_0_PSCI_VERSION, 0) >= 0x10000 &&
    hvc_call(PSCI_1_0_PSCI_FEATURES, ARM_SMCCC_VERSION) == 0
}

pub fn get_random_u64() -> Option<u64> {
    let mut ret: u64;
    let mut l: u64;

    if !have_smccc() {
        return None
    }

    unsafe {
        asm!("hvc   #0",
             in("x0") ARM_SMCCC_TRNG_RND64,
             in("x1") 64,

             lateout("x0") ret,
             lateout("x1") _,
             lateout("x2") _,
             lateout("x3") l,
             options(nomem, nostack, preserves_flags),
        );
    }
    if ret == 0 {
        Some(l)
    } else {
        None
    }
}

pub fn get_random_bytes(bytes: &mut[u8]) -> bool {
    let mut b: &mut[u8] = bytes;

    if !have_smccc() {
        return false
    }

    while b.len() > 0 {
        let bits = MAX_BITS_PER_CALL.min(8 * b.len());
        let (mut k, mut l, mut m): (u64, u64, u64);
        let mut ret: u64;

        unsafe {
            asm!("hvc   #0",
                 in("x0") ARM_SMCCC_TRNG_RND64,
                 in("x1") bits,

                 lateout("x0") ret,
                 lateout("x1") k,
                 lateout("x2") l,
                 lateout("x3") m,
                 options(nomem, nostack, preserves_flags),
            );
        }
        if ret != 0 {
            return false;
        }

        for s in [m, l, k] {
            let n = b.len().min(8);
            let v: &mut[u8];
            (v, b) = b.split_at_mut(n);
            v.copy_from_slice(&s.to_le_bytes().split_at(n).0);
        }
    }
    true
}
