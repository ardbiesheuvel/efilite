// SPDX-License-Identifier: GPL-2.0
// Copyright 2022-2023 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

use crate::current_el;
use core::arch::asm;
use core::cell::RefCell;

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

fn smccc_call(fid: u32, arg: u32, use_smc: bool) -> i32 {
    let mut ret: i32;
    if use_smc {
        unsafe {
            asm!(
                "smc #0",

                in("w0") fid,
                in("w1") arg,

                lateout("w0") ret,
                lateout("w1") _,
                lateout("w2") _,
                lateout("w3") _,

                options(nomem, nostack),
            );
        }
    } else {
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
    }
    ret
}

fn have_smccc(use_smc: bool) -> bool {
    smccc_call(PSCI_1_0_PSCI_VERSION, 0, use_smc) >= PSCI_1_0_PSCI_VERSION_1_0
        && smccc_call(PSCI_1_0_PSCI_FEATURES, ARM_SMCCC_VERSION, use_smc) == 0
        && smccc_call(ARM_SMCCC_VERSION, 0, use_smc) >= ARM_SMCCC_VERSION_1_1
        && smccc_call(ARM_SMCCC_TRNG_VERSION, 0, use_smc) >= ARM_SMCCC_TRNG_VERSION_1_0
        && smccc_call(ARM_SMCCC_TRNG_FEATURES, ARM_SMCCC_TRNG_RND64, use_smc) == 0
}

pub struct Random {
    have_smccc: bool,
    have_rndr: bool,
    use_smc: bool,
    seed: RefCell<u64>,
}

impl Random {
    pub fn new<F>(fallback: F) -> Option<Random>
    where
        F: Fn() -> Option<u64>,
    {
        let use_smc = current_el() == 2;

        let mut l: u64;
        unsafe {
            asm!(
                "mrs  {reg}, id_aa64isar0_el1",
                reg = out(reg) l,
                options(pure, nomem, nostack, preserves_flags)
            );
        }
        let rndr = (l >> ID_AA64ISAR0_RNDR_SHIFT) & 0xf != 0;
        let smccc = have_smccc(use_smc);
        let mut seed = 0u64;

        if !rndr && !smccc {
            seed = fallback()?;
        }

        Some(Random {
            have_smccc: smccc,
            have_rndr: rndr,
            use_smc: use_smc,
            seed: RefCell::new(seed),
        })
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

    fn get_pseudo_random_bytes(&self, bytes: &mut [u8]) -> bool {
        let mut s = self.seed.borrow_mut();
        let mut b: &mut [u8] = bytes;

        if *s == 0 {
            return false;
        }

        while b.len() > 0 {
            let l = unsafe {
                let mut l: u64;
                asm!(
                    "dup   v0.2d, {s}",
                    "dup   v1.4s, {m:w}",
                    "aese  v0.16b, v1.16b",
                    "aesmc v0.16b, v0.16b",
                    "mov   {s}, v0.d[0]",
                    "mov   {l}, v0.d[1]",
                    s = inout(reg) *s,
                    m = in(reg) 0xaa55,
                    l = out(reg) l,
                    out("v0") _,
                    out("v1") _,
                );
                l
            };
            let n = b.len().min(core::mem::size_of_val(&l));
            let v: &mut [u8];
            (v, b) = b.split_at_mut(n);
            v.copy_from_slice(&l.to_le_bytes()[..n]);
        }
        true
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
            if use_raw {
                return false;
            }
            return self.get_pseudo_random_bytes(bytes);
        }

        while b.len() > 0 {
            let bits = MAX_BITS_PER_CALL.min(8 * b.len());
            let (mut k, mut l, mut m): (u64, u64, u64);
            let mut ret: u64;

            if self.use_smc {
                unsafe {
                    asm!(
                        "smc #0",

                        in("w0") ARM_SMCCC_TRNG_RND64,
                        in("w1") bits,

                        lateout("x0") ret,
                        lateout("x1") k,
                        lateout("x2") l,
                        lateout("x3") m,

                        options(nomem, nostack),
                    );
                }
            } else {
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
            }
            if ret != 0 {
                return false;
            }

            for s in [m, l, k] {
                let n = b.len().min(8);
                let v: &mut [u8];
                (v, b) = b.split_at_mut(n);
                v.copy_from_slice(&s.to_le_bytes()[0..n]);

                if b.len() == 0 {
                    break;
                }
            }
        }
        true
    }
}
