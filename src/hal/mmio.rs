//! # MMIO Primitives
//!
//! Memory-mapped I/O read and write helpers. Every peripheral register access
//! in the entire firmware flows through these two functions. Keeping them here
//! means `unsafe` for hardware access is confined to a single, auditable file.
//!
//! `read_volatile` and `write_volatile` prevent the compiler from reordering,
//! caching, or eliminating peripheral register accesses — critical for correct
//! hardware behaviour.

/// Read a 32-bit value from a memory-mapped peripheral register.
///
/// # Arguments
/// * `base_address`   — peripheral base address (e.g. `UART0_BASE`)
/// * `register_offset`— byte offset of the register within the peripheral
///
/// # Safety (contained here, not caller's concern)
/// The caller must ensure `base_address + register_offset` is a valid,
/// readable MMIO address for this MCU. All callers in this codebase use
/// named constants from the same HAL module, so this is always satisfied.
#[inline(always)]
pub fn read_register(base_address: u32, register_offset: u32) -> u32 {
    // SAFETY: see above — all call sites use validated HAL constants.
    unsafe { core::ptr::read_volatile((base_address + register_offset) as *const u32) }
}

/// Write a 32-bit value to a memory-mapped peripheral register.
///
/// # Arguments
/// * `base_address`    — peripheral base address
/// * `register_offset` — byte offset of the register within the peripheral
/// * `value`           — the value to write
#[inline(always)]
pub fn write_register(base_address: u32, register_offset: u32, value: u32) {
    // SAFETY: see read_register above.
    unsafe { core::ptr::write_volatile((base_address + register_offset) as *mut u32, value) }
}

/// Read-modify-write: set specific bits in a register without disturbing others.
///
/// Equivalent to: `reg |= bits_to_set`
#[inline(always)]
pub fn set_register_bits(base_address: u32, register_offset: u32, bits_to_set: u32) {
    let current_value = read_register(base_address, register_offset);
    write_register(base_address, register_offset, current_value | bits_to_set);
}
