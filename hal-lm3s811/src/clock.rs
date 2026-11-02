//! # System Clock (SysCtl)
//!
//! The LM3S811 System Control block manages run-mode clock gating for all
//! peripherals. A peripheral that has not been clocked will not respond to
//! register writes - always enable its clock before configuring it.
//!
//! The LM3S811 is configured for its maximum 50 MHz system clock using the
//! on-chip PLL. All baud-rate and timing calculations in other HAL modules
//! must match this frequency. If a real board uses a different crystal or PLL
//! configuration, update `SYSTEM_CLOCK_HZ` here and all derived constants will
//! follow automatically.

use crate::mmio;

// --- SysCtl register map ------------------------------------------------------

/// Base address of the System Control block.
pub const SYSCTL_BASE: u32 = 0x400F_E000;

/// Raw Interrupt Status register. Bit 6 is the PLL lock status.
pub const SYSCTL_RIS_OFFSET: u32 = 0x050;

/// Run-Mode Clock Configuration register.
pub const SYSCTL_RCC_OFFSET: u32 = 0x060;

/// Run-mode Clock Gating Control register 1.
/// Bit 0 = UART0 clock enable.
pub const SYSCTL_RCGC1_OFFSET: u32 = 0x104;

/// Run-mode Clock Gating Control register 2.
/// Bit 0 = GPIOA clock enable.
/// Bit 3 = GPIOD clock enable (I2C0 pins live here on LM3S811).
pub const SYSCTL_RCGC2_OFFSET: u32 = 0x108;

// --- RCC bit fields -----------------------------------------------------------

/// PLL power-down control. Must be cleared to run the PLL.
const RCC_PWRDN: u32 = 1 << 13;
/// PLL output enable. Must be cleared to run the PLL.
const RCC_OEN: u32 = 1 << 12;
/// PLL bypass. Set while the PLL is configured, cleared once it is locked.
const RCC_BYPASS: u32 = 1 << 11;
/// Enable the system clock divider.
const RCC_USESYSDIV: u32 = 1 << 22;
/// System clock divider field.
const RCC_SYSDIV_MASK: u32 = 0xF << 23;
/// SYSDIV value 3 selects a divider of 4, giving 50 MHz from the 200 MHz VCO.
const RCC_SYSDIV_50MHZ: u32 = 3 << 23;
/// PLL lock bit in the Raw Interrupt Status register.
const RIS_PLL_LOCK: u32 = 1 << 6;

// --- Clock constants ----------------------------------------------------------

/// System clock frequency in Hz.
///
/// Change this constant when targeting a board with a different clock source.
/// All baud-rate and timing calculations derive from this value.
pub const SYSTEM_CLOCK_HZ: u32 = 50_000_000;

// --- Clock gate bit masks -----------------------------------------------------

/// RCGC1 bit to enable the UART0 peripheral clock.
pub const CLOCK_GATE_UART0: u32 = 1 << 0;

/// RCGC1 bit to enable the I2C0 peripheral clock.
pub const CLOCK_GATE_I2C0: u32 = 1 << 12;

/// RCGC2 bit to enable the GPIOA peripheral clock (UART0 pins: PA0/PA1).
pub const CLOCK_GATE_GPIOA: u32 = 1 << 0;

/// RCGC2 bit to enable the GPIOB peripheral clock (I2C0 pins: PB2/PB3).
pub const CLOCK_GATE_GPIOB: u32 = 1 << 1;

// --- Public API ---------------------------------------------------------------

/// Configure the PLL and system divider for a 50 MHz system clock.
///
/// The sequence follows the LM3S811 datasheet:
/// 1. bypass the PLL and disable the system divider;
/// 2. power and enable the PLL, select SYSDIV = 3;
/// 3. wait for the PLL lock bit;
/// 4. remove the bypass.
///
/// This must run before any peripheral timing is calculated.
pub fn configure_system_clock() {
    let mut rcc = mmio::read_register(SYSCTL_BASE, SYSCTL_RCC_OFFSET);

    // Step 1: run from the raw oscillator while the PLL is configured.
    rcc |= RCC_BYPASS;
    rcc &= !RCC_USESYSDIV;
    mmio::write_register(SYSCTL_BASE, SYSCTL_RCC_OFFSET, rcc);

    // Step 2: power and enable the PLL, then select the 50 MHz divider.
    rcc &= !(RCC_PWRDN | RCC_OEN);
    rcc &= !RCC_SYSDIV_MASK;
    rcc |= RCC_SYSDIV_50MHZ | RCC_USESYSDIV;
    mmio::write_register(SYSCTL_BASE, SYSCTL_RCC_OFFSET, rcc);

    // Step 3: wait until the PLL reports lock.
    while mmio::read_register(SYSCTL_BASE, SYSCTL_RIS_OFFSET) & RIS_PLL_LOCK == 0 {}

    // Step 4: switch the system clock to the locked PLL output.
    rcc &= !RCC_BYPASS;
    mmio::write_register(SYSCTL_BASE, SYSCTL_RCC_OFFSET, rcc);
    spin_for_cycles(100);
}

/// Enable the clock for one or more peripherals in RCGC1.
///
/// Pass a bitmask of `CLOCK_GATE_*` constants for the peripherals to enable.
/// Multiple peripherals can be enabled in a single call by OR-ing masks:
///
/// ```rust
/// enable_rcgc1_peripherals(CLOCK_GATE_UART0 | CLOCK_GATE_I2C0);
/// ```
pub fn enable_rcgc1_peripherals(peripheral_mask: u32) {
    // Set-bits only - never clear a clock gate that another driver enabled.
    mmio::set_register_bits(SYSCTL_BASE, SYSCTL_RCGC1_OFFSET, peripheral_mask);
}

/// Enable the clock for one or more GPIO ports in RCGC2.
///
/// Pass a bitmask of `CLOCK_GATE_GPIO*` constants.
pub fn enable_rcgc2_peripherals(peripheral_mask: u32) {
    mmio::set_register_bits(SYSCTL_BASE, SYSCTL_RCGC2_OFFSET, peripheral_mask);
}

/// Busy-wait for approximately `cycle_count` CPU cycles.
///
/// Used after enabling a peripheral clock to allow the clock to stabilise
/// before the first register access. The LM3S811 datasheet recommends at
/// least 3 clock cycles; we default to a generous 10.
///
/// This is not a precise timer - use a SysTick-based delay once the tick
/// clock is implemented.
#[inline(always)]
pub fn spin_for_cycles(cycle_count: u32) {
    for _ in 0..cycle_count {
        // nop prevents the compiler from optimising the loop away.
        unsafe { core::arch::asm!("nop") }
    }
}
