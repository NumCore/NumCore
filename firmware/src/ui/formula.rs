//! OLED formula renderer.
//!
//! Keeps the math input syntax ASCII-friendly while presenting common
//! calculator constructs in a compact 96x16 visual form.

use crate::ui::font;
use hal::oled::{Framebuffer, DISPLAY_PAGE_COUNT, DISPLAY_WIDTH_PIXELS};

const GLYPH_GAP: usize = 1;
const AGGREGATE_SYMBOL_COLUMNS: usize = 6;
const AGGREGATE_BOUND_CHARS: usize = 5;
const AGGREGATE_BOUND_COLUMNS: usize = AGGREGATE_BOUND_CHARS * font::CHAR_ADVANCE;
const AGGREGATE_BODY_START: usize = AGGREGATE_SYMBOL_COLUMNS + AGGREGATE_BOUND_COLUMNS;

#[derive(Clone, Copy)]
struct AggregateView<'a> {
    op: AggregateOp,
    body: &'a [u8],
    variable: u8,
    lower: &'a [u8],
    upper: &'a [u8],
}

#[derive(Clone, Copy)]
enum AggregateOp {
    Integral,
    Sum,
}

/// Render expression and result as a calculator-style OLED screen.
pub fn render_screen(fb: &mut Framebuffer, expression: &[u8], result: Option<&[u8]>) {
    clear_framebuffer(fb);

    if let Some(aggregate) = parse_aggregate(expression) {
        render_aggregate(fb, aggregate, result);
        return;
    }

    render_ascii_pretty(fb, 0, 0, tail_for_line(expression, font::CHARS_PER_LINE));
    if let Some(result) = result {
        render_ascii_pretty(fb, 1, 0, b"=");
        render_ascii_pretty(
            fb,
            1,
            font::CHAR_ADVANCE,
            tail_for_line(result, font::CHARS_PER_LINE - 1),
        );
    }
}

fn render_aggregate(fb: &mut Framebuffer, aggregate: AggregateView, result: Option<&[u8]>) {
    match aggregate.op {
        AggregateOp::Integral => draw_tall_glyph(fb, 0, &INTEGRAL_TOP, &INTEGRAL_BOTTOM),
        AggregateOp::Sum => draw_tall_glyph(fb, 0, &SIGMA_TOP, &SIGMA_BOTTOM),
    }

    render_ascii_pretty(
        fb,
        0,
        AGGREGATE_SYMBOL_COLUMNS,
        tail_for_line(aggregate.upper, AGGREGATE_BOUND_CHARS),
    );
    render_ascii_pretty(
        fb,
        1,
        AGGREGATE_SYMBOL_COLUMNS,
        tail_for_line(aggregate.lower, AGGREGATE_BOUND_CHARS),
    );

    let body_start = AGGREGATE_BODY_START;
    let body_cols = DISPLAY_WIDTH_PIXELS.saturating_sub(body_start);
    let body_chars = body_cols / font::CHAR_ADVANCE;
    render_ascii_pretty(fb, 0, body_start, tail_for_line(aggregate.body, body_chars));

    if let Some(result) = result {
        render_ascii_pretty(fb, 1, body_start, b"=");
        render_ascii_pretty(
            fb,
            1,
            body_start + font::CHAR_ADVANCE,
            tail_for_columns(result, body_cols - font::CHAR_ADVANCE),
        );
    } else if matches!(aggregate.op, AggregateOp::Integral) {
        let mut dx = [b'd', aggregate.variable];
        dx[1] = dx[1].to_ascii_lowercase();
        let dx_col = DISPLAY_WIDTH_PIXELS.saturating_sub(font::CHAR_ADVANCE * 2);
        render_ascii_pretty(fb, 1, dx_col, &dx);
    }
}

fn parse_aggregate(input: &[u8]) -> Option<AggregateView<'_>> {
    let trimmed = trim_ascii(input);
    let (op, prefix_len) = if starts_with_ignore_ascii_case(trimmed, b"int(") {
        (AggregateOp::Integral, 4)
    } else if starts_with_ignore_ascii_case(trimmed, b"sum(") {
        (AggregateOp::Sum, 4)
    } else {
        return None;
    };

    if trimmed.last().copied()? != b')' {
        return None;
    }

    let inner = &trimmed[prefix_len..trimmed.len() - 1];
    let mut parts = [&[][..]; 4];
    split_top_level_commas(inner, &mut parts)?;

    let var = trim_ascii(parts[1]);
    if var.len() != 1 || !var[0].is_ascii_alphabetic() {
        return None;
    }

    Some(AggregateView {
        op,
        body: trim_ascii(parts[0]),
        variable: var[0].to_ascii_uppercase(),
        lower: trim_ascii(parts[2]),
        upper: trim_ascii(parts[3]),
    })
}

fn split_top_level_commas<'a>(input: &'a [u8], parts: &mut [&'a [u8]; 4]) -> Option<()> {
    let mut depth = 0usize;
    let mut part_start = 0usize;
    let mut part_count = 0usize;

    for (index, &byte) in input.iter().enumerate() {
        match byte {
            b'(' => depth = depth.checked_add(1)?,
            b')' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
            }
            b',' if depth == 0 => {
                if part_count >= parts.len() {
                    return None;
                }
                parts[part_count] = &input[part_start..index];
                part_count += 1;
                part_start = index + 1;
            }
            _ => {}
        }
    }

    if part_count != 3 {
        return None;
    }
    parts[3] = &input[part_start..];
    Some(())
}

fn render_ascii_pretty(fb: &mut Framebuffer, page: usize, start_col: usize, text: &[u8]) {
    if page >= DISPLAY_PAGE_COUNT {
        return;
    }

    let mut col = start_col;
    let mut index = 0usize;
    while index < text.len() && col + font::GLYPH_WIDTH <= DISPLAY_WIDTH_PIXELS {
        let consumed = if starts_with_ignore_ascii_case(&text[index..], b"pi") {
            draw_page_glyph(fb, page, col, &PI_GLYPH);
            2
        } else {
            match text[index] {
                b'*' => draw_page_glyph(fb, page, col, &MULTIPLY_GLYPH),
                b'/' => draw_page_glyph(fb, page, col, &DIVIDE_GLYPH),
                b'-' => draw_page_glyph(fb, page, col, &MINUS_GLYPH),
                byte => draw_page_glyph(fb, page, col, font::glyph_columns(byte)),
            }
            1
        };

        col += font::GLYPH_WIDTH + GLYPH_GAP;
        index += consumed;
    }
}

fn draw_page_glyph(
    fb: &mut Framebuffer,
    page: usize,
    col: usize,
    columns: &[u8; font::GLYPH_WIDTH],
) {
    if page >= DISPLAY_PAGE_COUNT || col + font::GLYPH_WIDTH > DISPLAY_WIDTH_PIXELS {
        return;
    }

    let offset = page * DISPLAY_WIDTH_PIXELS + col;
    for (i, &column) in columns.iter().enumerate() {
        fb[offset + i] = column;
    }
    if col + font::GLYPH_WIDTH < DISPLAY_WIDTH_PIXELS {
        fb[offset + font::GLYPH_WIDTH] = 0;
    }
}

fn draw_tall_glyph(fb: &mut Framebuffer, col: usize, top: &[u8; 5], bottom: &[u8; 5]) {
    draw_page_glyph(fb, 0, col, top);
    draw_page_glyph(fb, 1, col, bottom);
}

fn clear_framebuffer(fb: &mut Framebuffer) {
    for byte in fb {
        *byte = 0;
    }
}

fn trim_ascii(bytes: &[u8]) -> &[u8] {
    let mut start = 0usize;
    let mut end = bytes.len();
    while start < end && bytes[start] == b' ' {
        start += 1;
    }
    while end > start && bytes[end - 1] == b' ' {
        end -= 1;
    }
    &bytes[start..end]
}

fn starts_with_ignore_ascii_case(bytes: &[u8], prefix: &[u8]) -> bool {
    if bytes.len() < prefix.len() {
        return false;
    }

    for i in 0..prefix.len() {
        if bytes[i].to_ascii_lowercase() != prefix[i].to_ascii_lowercase() {
            return false;
        }
    }
    true
}

fn tail_for_line(bytes: &[u8], chars: usize) -> &[u8] {
    if bytes.len() <= chars {
        bytes
    } else {
        &bytes[bytes.len() - chars..]
    }
}

fn tail_for_columns(bytes: &[u8], columns: usize) -> &[u8] {
    tail_for_line(bytes, columns / font::CHAR_ADVANCE)
}

#[rustfmt::skip]
const INTEGRAL_TOP: [u8; 5] = [
    0b00000000,
    0b00000000,
    0b11111110,
    0b00000001,
    0b00000001,
];

#[rustfmt::skip]
const INTEGRAL_BOTTOM: [u8; 5] = [
    0b01000000,
    0b01000000,
    0b00111111,
    0b00000000,
    0b00000000,
];

#[rustfmt::skip]
const SIGMA_TOP: [u8; 5] = [
    0b00001100,
    0b00010100,
    0b00100100,
    0b01000100,
    0b10000100,
];

#[rustfmt::skip]
const SIGMA_BOTTOM: [u8; 5] = [
    0b00011000,
    0b00010100,
    0b00010010,
    0b00010001,
    0b00010000,
];

#[rustfmt::skip]
const PI_GLYPH: [u8; 5] = [
    0b00000100,
    0b01111100,
    0b00000100,
    0b01111100,
    0b00000100,
];

#[rustfmt::skip]
const MULTIPLY_GLYPH: [u8; 5] = [
    0b00100010,
    0b00010100,
    0b00001000,
    0b00010100,
    0b00100010,
];

#[rustfmt::skip]
const DIVIDE_GLYPH: [u8; 5] = [
    0b00001000,
    0b00001000,
    0b00101010,
    0b00001000,
    0b00001000,
];

#[rustfmt::skip]
const MINUS_GLYPH: [u8; 5] = [
    0b00001000,
    0b00001000,
    0b00001000,
    0b00001000,
    0b00001000,
];
