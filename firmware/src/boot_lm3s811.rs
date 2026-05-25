//! # Boot Layer (Layer 3)
//!
//! The lowest software layer. Runs before `main()` and before any Rust code
//! can safely execute. Responsibilities:
//!
//!   - Place the Cortex-M vector table at the very start of Flash
//!   - Provide the Reset handler (true hardware entry point after power-on)
//!   - Copy `.data` from Flash into RAM
//!   - Zero-initialise `.bss`
//!   - Hand control to `runtime::start()` — never returns
//!
//! ## Rules enforced for this file
//!   - `unsafe` is permitted here and ONLY here for raw-pointer memory init
//!   - No application logic belongs here whatsoever
//!   - No HAL calls — the HAL is initialised by the runtime, not the bootloader
//!   - Must compile correctly even if every other module changes

use core::ptr;

// ─── Linker-defined section boundary symbols ──────────────────────────────────
//
// These are NOT variables. They are addresses defined in link.x.
// Taking a reference gives the address itself, not a value stored there.

extern "C" {
    static _sbss: u8; // First byte of .bss in RAM
    static _ebss: u8; // One-past-last byte of .bss in RAM
    static _sdata: u8; // First byte of .data destination in RAM
    static _edata: u8; // One-past-last byte of .data destination in RAM
    static _sidata: u8; // First byte of .data source in Flash
}

// ─── Reset handler ────────────────────────────────────────────────────────────

/// The Cortex-M Reset exception handler — the true hardware entry point.
///
/// Called by the CPU immediately after power-on or system reset. Uses the C
/// ABI so the linker can place its address correctly in the vector table.
///
/// Initialisation order is mandatory and must not change:
///   1. Zero `.bss`  — before any static that relies on zero-initialisation
///   2. Copy `.data` — before any initialised static is read
///   3. Enter the runtime — does not return
#[no_mangle]
pub unsafe extern "C" fn Reset() -> ! {
    zero_bss_section();
    copy_data_section_from_flash();
    crate::runtime::start()
}

/// Zero every byte in the `.bss` section.
///
/// All Rust statics without explicit initialisers live here. Both the C
/// standard and Rust guarantee they are zero at program start. On bare metal,
/// with no OS loader, we must fulfil that guarantee ourselves.
unsafe fn zero_bss_section() {
    let bss_start = &_sbss as *const u8 as *mut u8;
    // SAFETY: _sbss/_ebss are valid linker symbols that bracket .bss in RAM.
    let bss_length = (&_ebss as *const u8).offset_from(&_sbss) as usize;
    ptr::write_bytes(bss_start, 0, bss_length);
}

/// Copy the `.data` section from its load address in Flash to its runtime
/// address in RAM.
///
/// Statics with non-zero initialisers (e.g. `static X: u32 = 42`) are stored
/// in Flash at link time but must live in RAM at runtime so they can be
/// mutated. This copy bridges that gap.
unsafe fn copy_data_section_from_flash() {
    let destination = &_sdata as *const u8 as *mut u8;
    let length = (&_edata as *const u8).offset_from(&_sdata) as usize;
    let source = &_sidata as *const u8;
    // SAFETY: Flash source and RAM destination never overlap by definition.
    ptr::copy_nonoverlapping(source, destination, length);
}

// ─── Default exception handler ────────────────────────────────────────────────

/// Catch-all for any exception or interrupt with no dedicated handler.
///
/// Spins forever. In a debugger, halt execution and inspect the link register
/// to identify which exception vector was triggered and why.
#[no_mangle]
pub unsafe extern "C" fn DefaultHandler() -> ! {
    loop {}
}

// ─── Cortex-M3 vector table ───────────────────────────────────────────────────
//
// Must be placed at 0x0000_0000 (start of Flash on LM3S811). The linker
// script enforces this via the named sections below.
//
// Slot 0  — initial stack pointer value (link.x emits this as a raw LONG)
// Slot 1  — Reset vector (below)
// Slots 2-15 — remaining 14 core Cortex-M3 exception vectors (below)
// Slots 16+  — vendor/peripheral interrupt vectors (add here as needed)

/// Slot 1: Reset vector. The address the CPU fetches on power-on.
#[link_section = ".vector_table.reset_vector"]
#[no_mangle]
pub static RESET_VECTOR: unsafe extern "C" fn() -> ! = Reset;

/// Slots 2–15: core Cortex-M3 exception vectors.
///
/// All route to DefaultHandler for now. Replace individual entries with real
/// handlers as features are added:
///   - SysTick (slot 15) when a millisecond tick clock is needed
///   - SVCall  (slot 11) if a privilege-separation model is ever added
///   - Fault handlers (3–6) can be upgraded to emit diagnostic UART output
#[link_section = ".vector_table.exceptions"]
#[no_mangle]
pub static EXCEPTION_VECTORS: [Option<unsafe extern "C" fn() -> !>; 14] = [
    Some(DefaultHandler), // 2:  NMI
    Some(DefaultHandler), // 3:  HardFault
    Some(DefaultHandler), // 4:  MemManage
    Some(DefaultHandler), // 5:  BusFault
    Some(DefaultHandler), // 6:  UsageFault
    None,                 // 7:  Reserved
    None,                 // 8:  Reserved
    None,                 // 9:  Reserved
    None,                 // 10: Reserved
    Some(DefaultHandler), // 11: SVCall
    Some(DefaultHandler), // 12: DebugMonitor
    None,                 // 13: Reserved
    Some(DefaultHandler), // 14: PendSV
    Some(DefaultHandler), // 15: SysTick
];
