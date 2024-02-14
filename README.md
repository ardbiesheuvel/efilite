Introduction
============

This repository contains the Rust sources to build an implementation of UEFI that can boot Linux kernels on QEMU's arm64 'virt' machine. This is useful for self-decompressing EFI zboot kernels, and systemd UKI images that carry their own loading/unpacking logic. It may also be useful for container workloads if the kernel and initrd are not part of the guest filesystem image.

All assets loaded into the guest (kernel, initrd, command line, ACPI tables, SMBIOS tables) must be provided by the host/VMM. The firmware will load them into guest memory using fw_cfg's DMA interface.

There is [currently] no support for UEFI block I/O inside the guest. This means that booting distro ISOs is not supported, only kernels (or other EFI apps, to a limited extent) and initrds provided on the QEMU command line are accessible by the guest firmware.

This firmware implementation relies on preliminary page tables in NOR flash, and builds its own page tables in RAM based on memory availability. This allows the MMU to be enabled before any memory accesses are made, increasing performance, and completely removing the need for managing coherency explicitly. To avoid elaborate TLB maintenance and the need to reason about break-before-make (BBM) rules, the two sets of page tables are tagged using different ASIDs, and all mappings of memory are non-global.

The firmware can execute at EL1, or at EL2 if VHE is implemented.

For additional robustness, the firmware enables the WXN control in SCTLR, making all writable mappings of memory implicitly non-executable. An implementation of the UEFI memory attribute protocol is provided to allow loaders such as EFI zboot or UKI to manage executable permissions on memory ranges that they populate with executable code.

The initrd is exposed to the OS via the Linux-specific VendorMedia GUID device path for initrds. This is supported by all Linux architectures that implement EFI boot.

An implementation of the EFI RNG protocol is provided as well, based on the host's TRNG SMCCC implementation, or the RNDR system register, whichever is available.

Some minimal EFI runtime services are implemented: ResetSystem() and GetTime(), which are needed by Linux/arm64, are fully functional. GetVariable()/GetNextVariable() are implemented as stubs which are callable but never return anything. SetVariable() returns EFI_UNSUPPORTED.

Building
========

`cargo build` will produce an ELF executable called `efilite` somewhere in the `target/` directory, which can be converted into binary format using

`objcopy -O binary efilite efilite.bin`.

Running the firmware
====================

The binary firmware image can be passed to QEMU using the `-bios` command line option. E.g., the following command

`qemu-system-aarch64 -M virt -cpu host -accel kvm -bios efilite.bin -m 1g -kernel vmlinuz.efi -nographic`

should produce output similar to

```
efilite INFO - Using pl011@9000000 for console output
efilite INFO - Heap allocator with 831 KB of memory
efilite INFO - [0x0000000009000000..0x0000000009001000] Attributes(VALID | NON_GLOBAL | EXECUTE_NEVER)
efilite INFO - QEMU fwcfg node found: fw-cfg@9020000
efilite INFO - [0x0000000009020000..0x0000000009021000] Attributes(VALID | NON_GLOBAL | EXECUTE_NEVER)
efilite INFO - PL031 RTC node found: pl031@9010000
efilite INFO - [0x0000000009010000..0x0000000009011000] Attributes(VALID | NON_GLOBAL | EXECUTE_NEVER)
efilite INFO - Mapping all DRAM regions found in the DT:
efilite INFO - [0x0000000040000000..0x0000000080000000] Attributes(VALID | NORMAL | NON_GLOBAL | EXECUTE_NEVER)
efilite INFO - Remapping statically allocated regions:
efilite INFO - [0x0000000000010000..0x00000000000c0000] Attributes(VALID | NORMAL | READ_ONLY | NON_GLOBAL)
efilite INFO - [0x0000000040200000..0x0000000040400000] Attributes(VALID | NORMAL | NON_GLOBAL | EXECUTE_NEVER)
efilite INFO - [0x0000000040000000..0x0000000040200000] Attributes(VALID | NORMAL | READ_ONLY | NON_GLOBAL | EXECUTE_NEVER)
efilite INFO - Booting in ACPI mode
efilite INFO - Installing SMBIOS tables
efilite INFO - Starting loaded EFI program
EFI stub: Decompressing Linux Kernel...
EFI stub: Generating empty DTB
EFI stub: Exiting boot services...
[    0.000000] Booting Linux on physical CPU 0x0000000000 [0x000f0510]
[    0.000000] Linux version 6.8.0-rc3+ (ardb@palermo.c.googlers.com) (Debian clang version 16.0.6 (19), Debian LLD 16.0.6) #284 SMP PREEMPT Tue Feb 13 17:24:37 CET 2024
```
