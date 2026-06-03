use crate::hal::Display;
use crate::math::fixed_point;
use crate::math::matrix::{Matrix, MatrixKind};
use crate::runtime::state::{DisplayGrid, MAX_ROW_LEN};
use crate::ui::font;

// 16-column display layout:
//   Cols 0-13: viewport into virtual buffer (scrollable)
//   Col 14:    whitespace margin (fixed)
//   Col 15:    scroll-direction arrow (fixed)
//     Page 0 (top row): < if col_off>0, > if content continues right
//     Page 1 (bottom row): ^ if row_off>0, ↓ if more rows below

const VIEWPORT_W: usize = 14;
const ARROW_COL: usize = 15;

// Box-drawing glyphs (5 columns wide, bit 0 = top row of the 7-row cell)
const TOP_LEFT: [u8; 5] = [0b00000000, 0b11111111, 0b00000001, 0b00000001, 0b00000001];
const BOTTOM_LEFT: [u8; 5] = [0b00000000, 0b11111111, 0b10000000, 0b10000000, 0b10000000];
const TOP_RIGHT: [u8; 5] = [0b00000000, 0b00000001, 0b00000001, 0b00000001, 0b11111111];
const BOTTOM_RIGHT: [u8; 5] = [0b00000000, 0b10000000, 0b10000000, 0b10000000, 0b11111111];
const VERT_LINE_LEFT: [u8; 5] = [0b00000000, 0b11111111, 0b00000000, 0b00000000, 0b00000000];
const VERT_LINE_RIGHT: [u8; 5] = [0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b11111111];
const DOWN_ARROW: [u8; 5] = [0b00000010, 0b00000100, 0b00001000, 0b00000100, 0b00000010];

fn put_glyph<D: Display>(fb: &mut D::Buffer, page: usize, col: usize, glyph: &[u8; 5]) {
    if page >= D::HEIGHT / 8 || col + 5 > D::WIDTH { return; }
    let off = page * D::WIDTH + col;
    for (i, &g) in glyph.iter().enumerate() {
        if off + i < fb.as_mut().len() { fb.as_mut()[off + i] = g; }
    }
    if col + 5 < D::WIDTH {
        let gap = off + 5;
        if gap < fb.as_mut().len() { fb.as_mut()[gap] = 0; }
    }
}

fn put_char<D: Display>(fb: &mut D::Buffer, page: usize, col: usize, byte: u8) {
    if page >= D::HEIGHT / 8 { return; }
    if col > D::WIDTH || col + font::GLYPH_WIDTH > D::WIDTH { return; }
    let glyph = font::glyph_columns(byte);
    let off = page * D::WIDTH + col;
    for (i, &g) in glyph.iter().enumerate() {
        if off + i < fb.as_mut().len() { fb.as_mut()[off + i] = g; }
    }
    if col + font::GLYPH_WIDTH < D::WIDTH {
        let gap = off + font::GLYPH_WIDTH;
        if gap < fb.as_mut().len() { fb.as_mut()[gap] = 0; }
    }
}

/// Draw a bracket glyph for the given matrix row position.
fn draw_bracket<D: Display>(fb: &mut D::Buffer, page: usize, col: usize,
                            r: usize, total_rows: usize, right_side: bool)
{
    let glyph = if total_rows == 1 {
        if right_side { &BOTTOM_RIGHT } else { &BOTTOM_LEFT }
    } else if r == 0 {
        if right_side { &TOP_RIGHT } else { &TOP_LEFT }
    } else if r == total_rows - 1 {
        if right_side { &BOTTOM_RIGHT } else { &BOTTOM_LEFT }
    } else {
        if right_side { &VERT_LINE_RIGHT } else { &VERT_LINE_LEFT }
    };
    put_glyph::<D>(fb, page, col, glyph);
}

/// Build a virtual character buffer from a matrix.
/// Each row stores the FULL rendered row including brackets:
///   left_bracket + gap + padded_value + gap + ... + gap + right_bracket
/// Brackets are stored as `[`, `]`, `|` and intercepted by the renderer
/// to draw box-drawing glyphs instead of ASCII characters.
pub fn build_grid(mat: &Matrix, grid: &mut DisplayGrid) {
    if mat.kind != MatrixKind::Mat { return; }
    let rows = mat.rows as usize;
    let cols = mat.cols as usize;
    if rows == 0 || cols > 5 { return; }
    grid.num_rows = mat.rows;

    let mut col_w = [0u8; 5];
    for r in 0..rows {
        for c in 0..cols {
            let mut tmp = [0u8; 24];
            let s = fixed_point::format_fixed_point(mat.data[r * cols + c], &mut tmp);
            let len = s.len().min(17) as u8;
            if len > col_w[c] { col_w[c] = len; }
        }
    }

    // Row length: left_br + gap + (col_w[c] + gap) * cols + right_br
    let mut row_len = 2usize;
    for c in 0..cols {
        row_len += 1 + col_w[c] as usize; // gap + value
    }
    grid.row_len = row_len.min(MAX_ROW_LEN) as u8;

    // Build each row including brackets
    for r in 0..rows {
        let mut buf = [0u8; MAX_ROW_LEN];
        let mut pos = 0usize;

        // Left bracket
        let left = if rows == 1 || r == 0 || r == rows - 1 { b'[' } else { b'|' };
        buf[pos] = left; pos += 1;

        for c in 0..cols {
            if pos >= MAX_ROW_LEN { break; }
            // Gap before value
            buf[pos] = b' '; pos += 1;
            if pos >= MAX_ROW_LEN { break; }

            let mut tmp = [0u8; 24];
            let s = fixed_point::format_fixed_point(mat.data[r * cols + c], &mut tmp);
            let val_len = s.len().min(17);
            let cell_w = col_w[c] as usize;

            for i in 0..val_len.min(MAX_ROW_LEN.saturating_sub(pos)) {
                buf[pos] = s[i]; pos += 1;
            }
            for _ in val_len..cell_w {
                if pos >= MAX_ROW_LEN { break; }
                buf[pos] = b' '; pos += 1;
            }
        }

        // Right bracket
        if pos < MAX_ROW_LEN {
            let right = if rows == 1 || r == 0 || r == rows - 1 { b']' } else { b'}' };
            buf[pos] = right;
        }

        grid.rows[r] = buf;
    }
}

pub fn render_matrix<D: Display>(
    fb: &mut D::Buffer,
    grid: &DisplayGrid,
    row_off: usize,
    col_off: usize,
) {
    let total_rows = grid.num_rows as usize;
    if total_rows == 0 { return; }
    for b in fb.as_mut().iter_mut() { *b = 0; }

    let row_len = grid.row_len as usize;
    let top = row_off.min(total_rows.saturating_sub(2));
    let bot = if total_rows == 1 { top } else { (top + 1).min(total_rows - 1) };

    let max_co = if row_len > VIEWPORT_W { row_len - VIEWPORT_W } else { 0 };
    let co = col_off.min(max_co);

    // ─ Blit viewport from virtual buffer ─
    // The buffer contains the full row: [ bracket gap values gap bracket ]
    // Brackets are stored as `[`, `]`, `|` and rendered with box-drawing glyphs.
    for i in 0..VIEWPORT_W {
        let src = co + i;
        if src >= row_len { break; }
        let px = i * font::CHAR_ADVANCE;
        if px + font::GLYPH_WIDTH > D::WIDTH { break; }

        let ch_top = grid.rows[top][src];
        let ch_bot = grid.rows[bot][src];

        // Brackets: `[` = top/bottom-left, `]` = top/bottom-right,
        // `|` = middle-left, `}` = middle-right.
        match ch_top {
            b'[' | b'|' => draw_bracket::<D>(fb, 0, px, top, total_rows, false),
            b']' | b'}' => draw_bracket::<D>(fb, 0, px, top, total_rows, true),
            0 => {},
            _ => { put_char::<D>(fb, 0, px, ch_top); },
        }
        match ch_bot {
            b'[' | b'|' => draw_bracket::<D>(fb, 1, px, bot, total_rows, false),
            b']' | b'}' => draw_bracket::<D>(fb, 1, px, bot, total_rows, true),
            0 => {},
            _ => { put_char::<D>(fb, 1, px, ch_bot); },
        }
    }

    // ─ Fixed arrow overlays at col 15 ─
    let arrow_px = ARROW_COL * font::CHAR_ADVANCE;
    if arrow_px + font::GLYPH_WIDTH > D::WIDTH { return; }

    // Page 0 (top row): horizontal scroll indicator
    // Show `>` when content exists to the right; `<` when at the rightmost scroll position.
    if co == max_co && co > 0 {
        put_char::<D>(fb, 0, arrow_px, b'<');
    } else if row_len > VIEWPORT_W && co < max_co {
        put_char::<D>(fb, 0, arrow_px, b'>');
    }

    // Page 1 (bottom row): vertical scroll indicator
    // Show `↓` when rows below; `^` when at the bottommost scroll position.
    let max_ro = total_rows.saturating_sub(2);
    if row_off == max_ro && row_off > 0 {
        put_char::<D>(fb, 1, arrow_px, b'^');
    } else if top + 2 < total_rows && row_off < max_ro {
        put_glyph::<D>(fb, 1, arrow_px, &DOWN_ARROW);
    }
}
