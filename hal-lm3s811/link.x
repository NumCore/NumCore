/* LM3S811 memory map
 *   Flash : 0x0000_0000  64 KB
 *   SRAM  : 0x2000_0000   8 KB
 */

MEMORY
{
    FLASH : ORIGIN = 0x00000000, LENGTH = 64K
    RAM   : ORIGIN = 0x20000000, LENGTH = 8K
}

_stack_size = 1536;
_stack_start = ORIGIN(RAM) + LENGTH(RAM);
_stack_end = _stack_start - _stack_size;

/* Cortex-M vector table must start at the very beginning of Flash. */
ENTRY(Reset)

SECTIONS
{
    /* ── Vector table ─────────────────────────────────────── */
    .vector_table ORIGIN(FLASH) :
    {
        /* Initial stack pointer (end of RAM) */
        LONG(ORIGIN(RAM) + LENGTH(RAM));
        /* Reset handler */
        KEEP(*(.vector_table.reset_vector));
        /* All other exception / interrupt vectors */
        KEEP(*(.vector_table.exceptions));
    } > FLASH

    /* ── Code & read-only data ────────────────────────────── */
    .text :
    {
        *(.text .text.*);
        *(.rodata .rodata.*);
    } > FLASH

    /* ── Initialised data (LMA in Flash, VMA in RAM) ─────── */
    .data : AT(ADDR(.text) + SIZEOF(.text))
    {
        _sdata = .;
        *(.data .data.*);
        _edata = .;
    } > RAM

    /* ── Zero-initialised data ────────────────────────────── */
    .bss (NOLOAD) :
    {
        _sbss = .;
        *(.bss .bss.*);
        *(COMMON);
        _ebss = .;
    } > RAM

    .stack _stack_end (NOLOAD) :
    {
        . = . + _stack_size;
    } > RAM

    /* ── Discarded sections ──────────────────────────────── */
    /DISCARD/ :
    {
        *(.ARM.exidx*);
    }

    /* LMA base used by the startup copy loop */
    _sidata = LOADADDR(.data);
}
