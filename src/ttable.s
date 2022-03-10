// SPDX-License-Identifier: GPL-2.0
// Copyright 2022 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

	.set		.L_TT_TYPE_BLOCK, 0x1
	.set		.L_TT_TYPE_PAGE,  0x3
	.set		.L_TT_TYPE_TABLE, 0x3

	.set		.L_TT_AF, 0x1 << 10
	.set		.L_TT_NG, 0x1 << 11
	.set		.L_TT_RO, 0x2 << 6
	.set		.L_TT_XN, 0x3 << 53

	.set		.L_TT_MT_DEV, 0x0 << 2			// MAIR #0
	.set		.L_TT_MT_MEM, (0x1 << 2) | (0x3 << 8)	// MAIR #1

	.set		.L_PAGE_XIP,  .L_TT_TYPE_PAGE  | .L_TT_MT_MEM | .L_TT_AF | .L_TT_RO | .L_TT_NG
	.set		.L_BLOCK_DEV, .L_TT_TYPE_BLOCK | .L_TT_MT_DEV | .L_TT_AF | .L_TT_XN | .L_TT_NG
	.set		.L_BLOCK_MEM, .L_TT_TYPE_BLOCK | .L_TT_MT_MEM | .L_TT_AF | .L_TT_XN | .L_TT_NG
	.set		.L_BLOCK_RO,  .L_BLOCK_MEM | .L_TT_RO

	.globl		idmap
	.section	".rodata.idmap", "a", %progbits
	.align		12

			/* level 1 */
idmap:	.quad		0f + .L_TT_TYPE_TABLE		// 1 GB of flash and device mappings
	.quad		1f + .L_TT_TYPE_TABLE		// up to 1 GB of DRAM
	.fill		510, 8, 0x0			// 510 GB of remaining VA space

			/* level 2 */
0:	.quad		2f + .L_TT_TYPE_TABLE		// up to 2 MB of flash
	.fill		63, 8, 0x0			// 126 MB of unused flash
	.set		.Lidx, 64
	.rept		448
	.quad		.L_BLOCK_DEV | (.Lidx << 21)	// 896 MB of RW- device mappings
	.set		.Lidx, .Lidx + 1
	.endr

			/* level 2 */
1:	.quad		.L_BLOCK_RO  | 0x40000000	// DT provided by VMM
	.quad		.L_BLOCK_MEM | 0x40200000	// 2 MB of DRAM
	.fill		510, 8, 0x0

			/* level 3 */
2:	.fill		16, 8, 0x0			// omit first 64k
	.set		.Lidx, 16
	.rept		496
	.quad		.L_PAGE_XIP | (.Lidx << 12)	// 2044 KiB of R-X flash mappings
	.set		.Lidx, .Lidx + 1
	.endr
