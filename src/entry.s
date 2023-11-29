// SPDX-License-Identifier: GPL-2.0
// Copyright 2022 Google LLC
// Author: Ard Biesheuvel <ardb@google.com>

	.macro		adr_l, reg:req, sym:req
	adrp		\reg, \sym
	add		\reg, \reg, :lo12:\sym
	.endm

	.macro		mov_i, reg:req, imm:req
	movz		\reg, :abs_g3:\imm
	movk		\reg, :abs_g2_nc:\imm
	movk		\reg, :abs_g1_nc:\imm
	movk		\reg, :abs_g0_nc:\imm
	.endm

	.section	".text.entry", "ax", %progbits
	mov_i		x0, .Lmairval
	mov_i		x1, .Ltcrval
	adrp		x2, idmap
	mov_i		x3, .Lsctlrval
	mov_i		x4, .Lcpacrval
	adr_l		x5, vector_table

	mrs		x6, id_aa64mmfr0_el1	// check the supported PA range
	and		x6, x6, 0xf
	cmp		x6, #2			// 36-bit PA only?
	movz		x6, :abs_g2:.L_TCR_IPS_64GB
	movz		x7, :abs_g2:.L_TCR_IPS_1TB
	csel		x6, x6, x7, lt
	orr		x1, x1, x6

	mrs		x6, CurrentEL		// enable VHE if running at EL2
	tbz		x6, #3, 0f
	mrs		x6, hcr_el2
	orr		x6, x6, #1 << 34	// set E2H
	orr		x6, x6, #1 << 27	// set TGE
	msr		hcr_el2, x6
	isb

0:	msr		mair_el1, x0		// set up the 1:1 mapping
	msr		tcr_el1, x1
	msr		ttbr0_el1, x2
	isb

	tlbi		vmalle1			// invalidate any cached translations
	ic		iallu			// invalidate the I-cache
	dsb		nsh
	isb
	b		2f

	.section	".text", "ax", %progbits
2:	msr		sctlr_el1, x3		// enable MMU and caches
	msr		cpacr_el1, x4		// enable FP/SIMD
	msr		vbar_el1, x5		// enable exception handling
	isb

	adr_l		x0, _data		// initialize the .data section
	adr_l		x1, _edata
	adr_l		x2, data_lma
3:	cmp		x0, x1
	b.ge		4f
	ldp		q0, q1, [x2], #32
	stp		q0, q1, [x0], #32
	b		3b

4:	adr_l		x0, _bss_start		// wipe the .bss section
	adr_l		x1, _bss_end
	movi		v0.16b, #0
5:	cmp		x0, x1
	b.ge		6f
	stp		q0, q0, [x0], #32
	b		5b

6:	mov		x29, xzr		// initialize the frame pointer
	adrp		x0, _stack_end
	mov		sp, x0
	adrp		x0, _init_base		// initial DRAM base address
	adr_l		x1, _end		// statically allocated by program
	sub		x1, x1, x0
	mov_i		x2, _avail
	bl		efilite_main

	mrs		x1, CurrentEL
	tbnz		x1, #3, 8f
7:	mov_i		x0, 0x84000008		// PSCI SYSTEM OFF
	hvc		#0
	wfi
	b		7b

8:	mov_i		x0, 0x84000008		// PSCI SYSTEM OFF
	smc		#0
	wfi
	b		8b

	.macro		vector_entry
	.align		7
	adrp		x0, idmap
	adrp		x1, _stack_end
	msr		ttbr0_el1, x0		// switch back to the initial ID map
	isb
	mov		sp, x1			// reset the stack pointer
	mov		x29, xzr
	mrs		x0, esr_el1
	mrs		x1, elr_el1
	mrs		x2, far_el1
	bl		handle_exception
	.endm

	.section	".text.vector", "ax", %progbits
	.align		11
vector_table:
	.rept		16
	vector_entry
	.endr

	.set		.L_MAIR_DEV_nGnRE,	0x04
	.set		.L_MAIR_MEM_WBWA,	0xff
	.set		.Lmairval, .L_MAIR_DEV_nGnRE | (.L_MAIR_MEM_WBWA << 8)

	.set		.L_TCR_TG0_4KB,		0x0 << 14
	.set		.L_TCR_TG1_4KB,		0x2 << 30
	.set		.L_TCR_IPS_64GB,	0x1 << 32
	.set		.L_TCR_IPS_1TB,		0x2 << 32
	.set		.L_TCR_EPD1,		0x1 << 23
	.set		.L_TCR_SH_INNER,	0x3 << 12
	.set		.L_TCR_RGN_OWB,		0x1 << 10
	.set		.L_TCR_RGN_IWB,		0x1 << 8
	.set		.Ltcrval,	.L_TCR_TG0_4KB | .L_TCR_TG1_4KB | .L_TCR_EPD1 | .L_TCR_RGN_OWB
	.set		.Ltcrval, .Ltcrval | .L_TCR_RGN_IWB | .L_TCR_SH_INNER | (64 - 39) // TCR_T0SZ

	.set		.L_SCTLR_ELx_I,		0x1 << 12
	.set		.L_SCTLR_ELx_SA,	0x1 << 3
	.set		.L_SCTLR_ELx_C,		0x1 << 2
	.set		.L_SCTLR_ELx_M,		0x1 << 0
	.set		.L_SCTLR_EL1_SPAN,	0x1 << 23
	.set		.L_SCTLR_EL1_WXN,	0x1 << 19
	.set		.L_SCTLR_EL1_SED,	0x1 << 8
	.set		.L_SCTLR_EL1_ITD,	0x1 << 7
	.set		.L_SCTLR_EL1_RES1,	(0x1 << 11) | (0x1 << 20) | (0x1 << 22) | (0x1 << 28) | (0x1 << 29)
	.set		.Lsctlrval, .L_SCTLR_ELx_M | .L_SCTLR_ELx_C | .L_SCTLR_ELx_SA | .L_SCTLR_EL1_ITD | .L_SCTLR_EL1_SED
	.set		.Lsctlrval, .Lsctlrval | .L_SCTLR_ELx_I | .L_SCTLR_EL1_WXN | .L_SCTLR_EL1_SPAN | .L_SCTLR_EL1_RES1

	.set		.L_CPACR_EL1_FPEN,	0x3 << 20
	.set		.Lcpacrval, .L_CPACR_EL1_FPEN
