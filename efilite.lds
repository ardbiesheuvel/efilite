/* SPDX-License-Identifier: GPL-2.0 */
/*
 * Copyright 2022 Google LLC
 * Author: Ard Biesheuvel <ardb@google.com>
 */

MEMORY
{
	flash	: ORIGIN = 0,          LENGTH = 2M
	ram	: ORIGIN = 0x40000000, LENGTH = 4M
}

PROVIDE(_init_base = 0x40000000);
PROVIDE(_init_size = 0x400000);

ENTRY(_entry)

SECTIONS
{
	.text : {
		_entry = .;
		KEEP(*(.text.entry))
		*(.rodata.idmap)

		/*
		 * Omit the first 64k flash region from the runtime mapping
		 * so that the NOR flash control registers are not mapped
		 * by the OS
		 */
		. = ALIGN(0x10000);
		RTSCODE = .;
		*(SORT_BY_ALIGNMENT(.text*))
		*(SORT_BY_ALIGNMENT(.rodata*))
		*(.got .got.plt)
		*(.igot .iplt .igot.plt)
	} >flash

	/*
	 * QEMU passes the DT blob by storing it at the base of DRAM
	 * before starting the guest
	 */
	.dtb (NOLOAD) : {
		BSDATA = .;
		DTB = .;
		_dtb = .;
		. += 0x200000;
		_dtb_end = .;
	} >ram

	/*
	 * put the stack first so we will notice if we overrun and
	 * hit the R/O mapping of the DT blob
	 */
	.stack (NOLOAD) : {
		. += 0x4000;
		_stack_end = .;
	} >ram

	.data : ALIGN(32) {
		_data = .;
		*(SORT_BY_ALIGNMENT(.data*))
		. = ALIGN(32);
		_edata = .;
	} >ram AT >flash

	data_lma = LOADADDR(.data);

	.bss : ALIGN (32) {
		_bss_start = .;
		*(SORT_BY_ALIGNMENT(.bss*))
		. = ALIGN(32);
		_bss_end = .;
		_end = .;
	} >ram

	/*
	 * Reserve a single 64k block for all allocations that need
	 * to persist at runtime
	 */
	. = _init_base + _init_size - 0x10000;

	.rtsdata ALIGN(0x10000) : {
		RTSDATA = .;
		_avail = ABSOLUTE(. - _end);
		_rtsdata = .;
		*(.rtsdata .rtsdata*)
		_rtsedata = ALIGN(32);
		RTSPOOL = ALIGN(0x1000);
		. += 0xf000;
	} > ram AT >flash

	rtsdata_lma = LOADADDR(.rtsdata);

	.debug_abbrev 	0 : { *(.debug_abbrev) }
	.debug_info 	0 : { *(.debug_info) }
	.debug_aranges 	0 : { *(.debug_aranges) }
	.debug_ranges 	0 : { *(.debug_ranges) }
	.debug_str 	0 : { *(.debug_str) }
	.debug_pubnames	0 : { *(.debug_pubnames) }
	.debug_pubtypes	0 : { *(.debug_pubtypes) }
	.debug_line 	0 : { *(.debug_line) }
	.debug_frame 	0 : { *(.debug_frame) }
	.debug_loc 	0 : { *(.debug_loc) }

	/DISCARD/ : {
		*(.note* .comment* .rela.* .eh_frame_hdr .eh_frame)
	}
}
ASSERT(SIZEOF(.rtsdata) != 0x10000, ".rtsdata section too big")
