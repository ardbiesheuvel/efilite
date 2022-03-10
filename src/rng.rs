// SPDX-License-Identifier: GPL-2.0
// Copyright 2022-2023 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

use core::arch::asm;

const ID_AA64ISAR0_RNDR_SHIFT: usize = 60;

const PSCI_1_0_PSCI_VERSION: u32 = 0x84000000;
const PSCI_1_0_PSCI_FEATURES: u32 = 0x8400000a;
const PSCI_1_0_PSCI_VERSION_1_0: i32 = 0x10000;

const ARM_SMCCC_VERSION: u32 = 0x80000000;
const ARM_SMCCC_VERSION_1_1: i32 = 0x10001;

const ARM_SMCCC_TRNG_VERSION: u32 = 0x84000050;
const ARM_SMCCC_TRNG_VERSION_1_0: i32 = 0x10000;

const ARM_SMCCC_TRNG_FEATURES: u32 = 0x84000051;
const ARM_SMCCC_TRNG_RND64: u32 = 0xc4000053;

const MAX_BITS_PER_CALL: usize = 192;

fn hvc32_call(fid: u32, arg: u32) -> i32 {
    let mut ret: i32;
    unsafe {
        asm!(
            "hvc #0",

            in("w0") fid,
            in("w1") arg,

            lateout("w0") ret,
            lateout("w1") _,
            lateout("w2") _,
            lateout("w3") _,

            options(nomem, nostack),
        );
    }
    ret
}

fn have_smccc() -> bool {
    hvc32_call(PSCI_1_0_PSCI_VERSION, 0) >= PSCI_1_0_PSCI_VERSION_1_0
        && hvc32_call(PSCI_1_0_PSCI_FEATURES, ARM_SMCCC_VERSION) == 0
        && hvc32_call(ARM_SMCCC_VERSION, 0) >= ARM_SMCCC_VERSION_1_1
        && hvc32_call(ARM_SMCCC_TRNG_VERSION, 0) >= ARM_SMCCC_TRNG_VERSION_1_0
        && hvc32_call(ARM_SMCCC_TRNG_FEATURES, ARM_SMCCC_TRNG_RND64) == 0
}

pub struct Random {
    have_smccc: bool,
    have_rndr: bool,
}

impl Random {
    pub fn new() -> Random {
        let mut l: u64;
        unsafe {
            asm!(
                "mrs  {reg}, id_aa64isar0_el1",
                reg = out(reg) l,
                options(pure, nomem, nostack, preserves_flags)
            );
        }
        let rndr = (l >> ID_AA64ISAR0_RNDR_SHIFT) & 0xf != 0;

        Random {
            have_smccc: have_smccc(),
            have_rndr: rndr,
        }
    }

    fn read_rndr() -> Option<u64> {
        let mut l: u64;
        let mut ret: u64;
        unsafe {
            asm!(
                "mrs  {reg}, rndr",
                "cset {ret}, ne",

                reg = out(reg) l,
                ret = out(reg) ret,

                options(nomem, nostack)
            );
        }
        if ret != 0 {
            Some(l)
        } else {
            None
        }
    }
}

impl efiloader::Random for Random {
    fn get_entropy(&self, bytes: &mut [u8], use_raw: bool) -> bool {
        let mut b: &mut [u8] = bytes;

        if !use_raw && self.have_rndr {
            while let Some(l) = Self::read_rndr() {
                let n = b.len().min(core::mem::size_of_val(&l));
                let v: &mut [u8];
                (v, b) = b.split_at_mut(n);
                v.copy_from_slice(&l.to_le_bytes()[..n]);
                if b.len() == 0 {
                    return true;
                }
            }
        }

        if !self.have_smccc {
            return false;
        }

        while b.len() > 0 {
            let bits = MAX_BITS_PER_CALL.min(8 * b.len());
            let (mut k, mut l, mut m): (u64, u64, u64);
            let mut ret: u64;

            unsafe {
                asm!(
                    "hvc #0",

                    in("w0") ARM_SMCCC_TRNG_RND64,
                    in("w1") bits,

                    lateout("x0") ret,
                    lateout("x1") k,
                    lateout("x2") l,
                    lateout("x3") m,

                    options(nomem, nostack),
                );
            }
            if ret != 0 {
                return false;
            }

            for s in [m, l, k] {
                // SMCCC TRNG populates registers from the MSB end
                let n = 8 - b.len().min(8);
                let v: &mut [u8];
                (v, b) = b.split_at_mut(8 - n);
                v.copy_from_slice(&s.to_le_bytes()[n..8]);

                if b.len() == 0 {
                    break;
                }
            }
        }
        true
    }
}
