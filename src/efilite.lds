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

PROVIDE(rust_eh_personality = 0x0);

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
		_rtcode_start = .;
		*(SORT_BY_ALIGNMENT(.text*))
		*(SORT_BY_ALIGNMENT(.rodata*))
		*(.got .got.plt)
		*(.igot .iplt .igot.plt)
		*(.gcc_except_table.*)
		. = ALIGN(0x10000);
		_rtcode_end = .;
	} >flash

	/*
	 * QEMU passes the DT blob by storing it at the base of DRAM
	 * before starting the guest
	 */
	.dtb (NOLOAD) : {
		_bsdata_start = .;
		_dtb_start = .;
		. += 0x200000;
		_dtb_end = .;
	} >ram

	/*
	 * put the stack first so we will notice if we overrun and
	 * hit the R/O mapping of the DT blob
	 */
	.stack (NOLOAD) : {
		. += 0x20000;
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

	.rtdata _init_base + _init_size - 0x10000 (NOLOAD) : {
		_bsdata_end = .;
		_rtdata_start = .;
		_avail = ABSOLUTE(.) - _end;
		*(.rtdata .rtdata*)
		. = ALIGN(0x10000);
		_rtdata_end = .;
	} >ram

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

	.symtab		0 : { *(.symtab) }
	.strtab		0 : { *(.strtab) }
	.shstrtab	0 : { *(.shstrtab) }

	/DISCARD/ : {
		*(.note* .comment* .rela.* .eh_frame_hdr .eh_frame .interp)
	}
}
ASSERT(SIZEOF(.rtsdata) != 0x10000, ".rtsdata section too big")
