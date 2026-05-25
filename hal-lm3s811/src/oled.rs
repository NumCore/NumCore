#![allow(dead_code)]
//! # OLED Display Driver — SSD0303 / OSRAM Pictiva 96×16 (Layer 4 — HAL)
//!
//! Drives the 96×16 monochrome OLED display fitted to the LM3S811EVB
//! evaluation board via the I2C0 peripheral (PB2 = SCL, PB3 = SDA).
//!
//! ## Display geometry
//!   96 columns × 16 rows = 1 536 pixels
//!   The SSD0303 organises RAM as 2 pages × 96 columns (1 byte = 8 vertical pixels).
//!   Page 0 = rows 0-7 (top), Page 1 = rows 8-15 (bottom).
//!
//! ## I2C protocol (SSD0303)
//!   Every I2C write starts with a control byte that tells the controller
//!   whether the following bytes are commands or data (GDDRAM pixels):
//!
//!     Control byte 0x80  → one command byte follows  (Co=1, D/C#=0)
//!     Control byte 0x40  → data stream     (Co=0, D/C#=1)
//!
//! ## QEMU behaviour
//!   The `lm3s811evb` QEMU machine models the SSD0303 faithfully enough for
//!   init + page-write sequences. The display framebuffer is visible in the
//!   QEMU SDL/GTK window that appears when you run without `-nographic`.
//!
//! ## Coordinate system
//!   (col, page) where col ∈ [0, 95] and page ∈ [0, 1].
//!   Within each page byte, bit 0 = topmost pixel of that 8-pixel stripe.
//!
//! ## Usage
//!   ```rust
//!   oled::initialise_oled();
//!   oled::clear_display();
//!   oled::render_screen(&framebuffer);   // framebuffer: [u8; 192]
//!   ```

use crate::i2c;

// ─── I2C address ──────────────────────────────────────────────────────────────

/// SSD0303 I2C address on the LM3S811EVB.
const OLED_I2C_ADDRESS: u8 = 0x3D;

// ─── SSD0303 command constants ────────────────────────────────────────────────

/// Control byte: the next byte in this transaction is a command.
///
/// QEMU's SSD0303 model accepts 0x80 for command bytes and 0x40 for data.
/// It rejects 0x00 command-stream framing even though some SSD130x-style
/// controllers document that mode.
const CTRL_COMMAND_BYTE: u8 = 0x80;

/// Control byte: everything that follows in this transaction is GDDRAM data.
const CTRL_DATA_STREAM: u8 = 0x40;

/// Turn the display panel on.
const CMD_DISPLAY_ON: u8 = 0xAF;

/// Turn the display panel off (sleep, retains RAM).
const CMD_DISPLAY_OFF: u8 = 0xAE;

/// Set display start line to row 0 (bits [5:0] = 0).
const CMD_START_LINE_0: u8 = 0x40;

/// Set memory addressing mode (SSD0303 uses page mode by default; this
/// command is included for clarity — value 0x20 selects horizontal mode on
/// SSD1306, but SSD0303 uses a different scheme; we set the page address
/// explicitly instead).

/// Set page address (OR with page number 0-1 to produce 0xB0 / 0xB1).
const CMD_PAGE_ADDRESS_BASE: u8 = 0xB0;

/// Set column address high nibble (OR with high nibble of column).
const CMD_COLUMN_HIGH_BASE: u8 = 0x10;

/// Set column address low nibble (OR with low nibble of column).
const CMD_COLUMN_LOW_BASE: u8 = 0x00;

/// Set contrast (double-byte command — next byte is the contrast value 0-255).
const CMD_SET_CONTRAST: u8 = 0x81;

/// Default contrast level. Increase if the display appears dim in QEMU.
const DEFAULT_CONTRAST: u8 = 0xCF;

/// Normal display (pixel on = GDDRAM bit 1). 0xA7 inverts.
const CMD_NORMAL_DISPLAY: u8 = 0xA7;

/// Segment re-map: column address 0 → SEG0 (left-to-right scan).
const CMD_SEG_REMAP_NORMAL: u8 = 0xA0;

/// COM output scan direction: from COM0 downward (top-to-bottom).
const CMD_COM_SCAN_NORMAL: u8 = 0xC0;

/// Multiplex ratio command (double-byte: next byte = MUX ratio − 1).
const CMD_SET_MUX_RATIO: u8 = 0xA8;

/// MUX ratio for 16 rows (16 − 1 = 15).
const MUX_RATIO_16_ROWS: u8 = 0x0F;

/// Display offset command (double-byte: next byte = vertical shift, 0 = none).
const CMD_SET_DISPLAY_OFFSET: u8 = 0xD3;

/// No vertical offset.
const DISPLAY_OFFSET_NONE: u8 = 0x00;

/// Charge pump enable command (SSD0303 uses internal charge pump).
/// First byte is the command, second selects internal VCC.
const CMD_CHARGE_PUMP: u8 = 0x8D;
const CHARGE_PUMP_ENABLE: u8 = 0x14;

// ─── Display dimensions ───────────────────────────────────────────────────────

/// Number of pixel columns.
pub const DISPLAY_WIDTH_PIXELS: usize = 96;

/// Controller column where the 96 visible pixels begin in QEMU's SSD0303 model.
const VISIBLE_COLUMN_OFFSET: u8 = 36;

/// Number of pages (each page = 8 pixel rows).
pub const DISPLAY_PAGE_COUNT: usize = 2;

/// Total bytes in one full framebuffer (width × pages).
pub const FRAMEBUFFER_SIZE: usize = DISPLAY_WIDTH_PIXELS * DISPLAY_PAGE_COUNT;

// ─── Framebuffer type alias ───────────────────────────────────────────────────

/// A complete display framebuffer.
///
/// Layout: `fb[page * DISPLAY_WIDTH_PIXELS + col]`
/// Bit 0 of each byte is the topmost pixel of that 8-row stripe.
pub type Framebuffer = [u8; FRAMEBUFFER_SIZE];

// ─── Initialisation ───────────────────────────────────────────────────────────

/// Initialise the SSD0303 controller and turn the display on.
///
/// Must be called after `i2c::initialise_i2c()`. Sends the full recommended
/// startup sequence from the SSD0303 application note.
pub fn initialise_oled() {
    let init_sequence: [u8; 5] = [
        CMD_DISPLAY_OFF,
        CMD_START_LINE_0,
        CMD_SEG_REMAP_NORMAL,
        CMD_NORMAL_DISPLAY,
        CMD_DISPLAY_ON,
    ];

    send_command_bytes(&init_sequence);
}

// ─── Display operations ───────────────────────────────────────────────────────

/// Fill every pixel to off (black). Equivalent to `render_screen(&[0u8; 192])`.
pub fn clear_display() {
    let blank: Framebuffer = [0u8; FRAMEBUFFER_SIZE];
    render_screen(&blank);
}

/// Write a complete framebuffer to the display.
///
/// Iterates over each of the two pages, seeks to column 0 of that page, then
/// streams all 96 pixel bytes in a single I2C data transaction.
///
/// # Arguments
/// * `framebuffer` — 192-byte array, layout: `fb[page * 96 + col]`
pub fn render_screen(framebuffer: &Framebuffer) {
    for page in 0..DISPLAY_PAGE_COUNT {
        seek_to_page_column(page as u8, VISIBLE_COLUMN_OFFSET);

        // Build a data packet: [CTRL_DATA_STREAM, byte0, byte1, … byte95]
        // We need a fixed-size buffer because no_std has no Vec.
        let mut packet = [0u8; 1 + DISPLAY_WIDTH_PIXELS]; // 97 bytes
        packet[0] = CTRL_DATA_STREAM;
        let page_start = page * DISPLAY_WIDTH_PIXELS;
        packet[1..].copy_from_slice(&framebuffer[page_start..page_start + DISPLAY_WIDTH_PIXELS]);

        i2c::send_bytes(OLED_I2C_ADDRESS, &packet);
    }
}

/// Set the SSD0303's internal address pointer to a specific page and column.
///
/// Must be called before streaming pixel data for that page. The SSD0303 auto-
/// increments the column pointer as data bytes are written, so one seek per
/// page is sufficient when writing the full 96-column row.
fn seek_to_page_column(page: u8, column: u8) {
    let cmd_bytes: [u8; 3] = [
        CMD_PAGE_ADDRESS_BASE | (page & 0x01),         // 0xB0 or 0xB1
        CMD_COLUMN_HIGH_BASE | ((column >> 4) & 0x0F), // high nibble
        CMD_COLUMN_LOW_BASE | (column & 0x0F),         // low nibble
    ];
    send_command_bytes(&cmd_bytes);
}

/// Send commands using QEMU-compatible one-command-byte framing.
///
/// The packet layout is [0x80, cmd0, 0x80, cmd1, ...].
fn send_command_bytes(commands: &[u8]) {
    let mut packet = [0u8; 32];
    let mut packet_len = 0;

    for &command in commands {
        if packet_len + 2 > packet.len() {
            i2c::send_bytes(OLED_I2C_ADDRESS, &packet[..packet_len]);
            packet_len = 0;
        }

        packet[packet_len] = CTRL_COMMAND_BYTE;
        packet[packet_len + 1] = command;
        packet_len += 2;
    }

    if packet_len != 0 {
        i2c::send_bytes(OLED_I2C_ADDRESS, &packet[..packet_len]);
    }
}

// ─── Pixel helpers ────────────────────────────────────────────────────────────

/// Set or clear an individual pixel in a framebuffer (does not push to display).
///
/// Call `render_screen` after composing your frame. This avoids one I2C
/// transaction per pixel.
///
/// # Arguments
/// * `fb`  — mutable reference to the framebuffer to modify
/// * `col` — pixel column, 0-95 (clamped — out-of-range silently ignored)
/// * `row` — pixel row,    0-15 (clamped)
/// * `on`  — `true` lights the pixel, `false` clears it
#[inline]
pub fn set_pixel(fb: &mut Framebuffer, col: usize, row: usize, on: bool) {
    if col >= DISPLAY_WIDTH_PIXELS || row >= (DISPLAY_PAGE_COUNT * 8) {
        return;
    }
    let page = row / 8;
    let bit = row % 8;
    let byte_index = page * DISPLAY_WIDTH_PIXELS + col;
    if on {
        fb[byte_index] |= 1 << bit;
    } else {
        fb[byte_index] &= !(1 << bit);
    }
}
