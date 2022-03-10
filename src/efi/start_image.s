// SPDX-License-Identifier: GPL-2.0
// Copyright 2022 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

	.section ".text", "ax", %progbits
	.globl	exit_image
exit_image:
	mov	sp, x1
	b	0f

	.globl	start_image
start_image:
	stp	x29, x30, [sp, #-96]!
	mov	x29, sp
	stp	x19, x20, [sp, #16]
	stp	x21, x22, [sp, #32]
	stp	x23, x24, [sp, #48]
	stp	x25, x26, [sp, #64]
	stp	x27, x28, [sp, #80]

	mov	x19, x3
	str	x29, [x19]	// store current SP in loadedimage protocol
	blr	x2
	str	xzr, [x19]	// wipe recorded SP value

0:	ldp	x19, x20, [sp, #16]
	ldp	x21, x22, [sp, #32]
	ldp	x23, x24, [sp, #48]
	ldp	x25, x26, [sp, #64]
	ldp	x27, x28, [sp, #80]
	ldp	x29, x30, [sp], #96
	ret
