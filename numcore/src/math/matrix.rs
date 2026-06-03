use super::complex::Complex;
use super::fixed_point as fp;

pub const MAX_MATRIX_DIM: usize = 4;
pub const MAX_MATRIX_CELLS: usize = MAX_MATRIX_DIM * MAX_MATRIX_DIM;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MatrixKind {
    Scalar,
    Complex,
    Mat,
    Scientific,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Matrix {
    pub data: [i64; MAX_MATRIX_CELLS],
    pub rows: u8,
    pub cols: u8,
    pub kind: MatrixKind,
}

impl Matrix {
    pub fn scalar(value: i64) -> Self {
        let mut data = [0i64; MAX_MATRIX_CELLS];
        data[0] = value;
        Self {
            data,
            rows: 1,
            cols: 1,
            kind: MatrixKind::Scalar,
        }
    }

    pub fn complex(re: i64, im: i64) -> Self {
        let mut data = [0i64; MAX_MATRIX_CELLS];
        data[0] = re;
        data[1] = im;
        Self {
            data,
            rows: 1,
            cols: 2,
            kind: MatrixKind::Complex,
        }
    }

    pub fn scientific(mantissa: i64, exponent: i64) -> Option<Self> {
        if exponent > 99 || exponent < -99 {
            return None;
        }
        let mut data = [0i64; MAX_MATRIX_CELLS];
        data[0] = mantissa;
        data[1] = exponent;
        Some(Self {
            data,
            rows: 1,
            cols: 2,
            kind: MatrixKind::Scientific,
        })
    }

    pub fn to_scientific(&self) -> Option<(i64, i64)> {
        match self.kind {
            MatrixKind::Scientific => Some((self.data[0], self.data[1])),
            _ => None,
        }
    }

    pub fn to_scientific_value(&self) -> Option<(i64, i64)> {
        match self.kind {
            MatrixKind::Scientific => self.to_scientific(),
            MatrixKind::Scalar => {
                let v = self.data[0];
                if v == 0 {
                    return Some((0, 0));
                }
                normalize_scientific(v, 0)
            }
            MatrixKind::Complex if self.data[1] == 0 => {
                Matrix::scalar(self.data[0]).to_scientific_value()
            }
            _ => None,
        }
    }

    pub fn mat_from_slice(slice: &[i64], rows: u8, cols: u8) -> Option<Self> {
        let len = rows as usize * cols as usize;
        if rows == 0 || cols == 0 || rows > MAX_MATRIX_DIM as u8 || cols > MAX_MATRIX_DIM as u8 {
            return None;
        }
        if slice.len() < len {
            return None;
        }
        let mut data = [0i64; MAX_MATRIX_CELLS];
        data[..len].copy_from_slice(&slice[..len]);
        Some(Self {
            data,
            rows,
            cols,
            kind: MatrixKind::Mat,
        })
    }

    pub fn identity(n: u8) -> Option<Self> {
        if n == 0 || n > MAX_MATRIX_DIM as u8 {
            return None;
        }
        let mut data = [0i64; MAX_MATRIX_CELLS];
        for i in 0..n as usize {
            data[i * n as usize + i] = fp::SCALE;
        }
        Some(Self {
            data,
            rows: n,
            cols: n,
            kind: MatrixKind::Mat,
        })
    }

    pub fn from_complex(c: Complex) -> Self {
        if c.im == 0 {
            Self::scalar(c.re)
        } else {
            Self::complex(c.re, c.im)
        }
    }

    pub fn to_complex(&self) -> Option<Complex> {
        match self.kind {
            MatrixKind::Scalar => Some(Complex::from_real(self.data[0])),
            MatrixKind::Complex => Some(Complex::new(self.data[0], self.data[1])),
            MatrixKind::Scientific => None,
            MatrixKind::Mat => None,
        }
    }

    /// Whether this is a square matrix (Mat kind, rows == cols).
    pub fn is_square(&self) -> bool {
        self.kind == MatrixKind::Mat && self.rows == self.cols
    }

    pub fn cell(&self, row: usize, col: usize) -> i64 {
        self.data[row * self.cols as usize + col]
    }

    pub fn cell_mut(&mut self, row: usize, col: usize) -> &mut i64 {
        &mut self.data[row * self.cols as usize + col]
    }

    pub fn elementwise_add(&self, other: &Self) -> Option<Self> {
        if self.kind != MatrixKind::Mat || other.kind != MatrixKind::Mat {
            return None;
        }
        if self.rows != other.rows || self.cols != other.cols {
            return None;
        }
        let len = self.rows as usize * self.cols as usize;
        let mut data = [0i64; MAX_MATRIX_CELLS];
        for i in 0..len {
            data[i] = self.data[i].saturating_add(other.data[i]);
        }
        Some(Self {
            data,
            rows: self.rows,
            cols: self.cols,
            kind: MatrixKind::Mat,
        })
    }

    pub fn elementwise_sub(&self, other: &Self) -> Option<Self> {
        if self.kind != MatrixKind::Mat || other.kind != MatrixKind::Mat {
            return None;
        }
        if self.rows != other.rows || self.cols != other.cols {
            return None;
        }
        let len = self.rows as usize * self.cols as usize;
        let mut data = [0i64; MAX_MATRIX_CELLS];
        for i in 0..len {
            data[i] = self.data[i].saturating_sub(other.data[i]);
        }
        Some(Self {
            data,
            rows: self.rows,
            cols: self.cols,
            kind: MatrixKind::Mat,
        })
    }

    pub fn matmul(&self, other: &Self) -> Option<Self> {
        if self.kind != MatrixKind::Mat || other.kind != MatrixKind::Mat {
            return None;
        }
        let (m, n, p) = (self.rows as usize, self.cols as usize, other.cols as usize);
        if n != other.rows as usize {
            return None;
        }
        if m > MAX_MATRIX_DIM || p > MAX_MATRIX_DIM {
            return None;
        }
        let mut data = [0i64; MAX_MATRIX_CELLS];
        for i in 0..m {
            for j in 0..p {
                let mut sum: i128 = 0;
                for k in 0..n {
                    let a = self.data[i * n + k] as i128;
                    let b = other.data[k * p + j] as i128;
                    sum = sum.saturating_add(a.saturating_mul(b));
                }
                let scaled = (sum >> 32) as i64;
                data[i * p + j] = scaled;
            }
        }
        Some(Self {
            data,
            rows: m as u8,
            cols: p as u8,
            kind: MatrixKind::Mat,
        })
    }

    pub fn scalar_mul(&self, k: i64) -> Option<Self> {
        if self.kind != MatrixKind::Mat {
            return None;
        }
        let len = self.rows as usize * self.cols as usize;
        let mut data = [0i64; MAX_MATRIX_CELLS];
        for i in 0..len {
            data[i] = fp::multiply(self.data[i], k)?;
        }
        Some(Self {
            data,
            rows: self.rows,
            cols: self.cols,
            kind: MatrixKind::Mat,
        })
    }

    pub fn scalar_add(&self, k: i64) -> Option<Self> {
        if self.kind != MatrixKind::Mat {
            return None;
        }
        let len = self.rows as usize * self.cols as usize;
        let mut data = [0i64; MAX_MATRIX_CELLS];
        for i in 0..len {
            data[i] = self.data[i].saturating_add(k);
        }
        Some(Self {
            data,
            rows: self.rows,
            cols: self.cols,
            kind: MatrixKind::Mat,
        })
    }

    pub fn scalar_sub(&self, k: i64) -> Option<Self> {
        if self.kind != MatrixKind::Mat {
            return None;
        }
        let len = self.rows as usize * self.cols as usize;
        let mut data = [0i64; MAX_MATRIX_CELLS];
        for i in 0..len {
            data[i] = self.data[i].saturating_sub(k);
        }
        Some(Self {
            data,
            rows: self.rows,
            cols: self.cols,
            kind: MatrixKind::Mat,
        })
    }

    pub fn transpose(&self) -> Option<Self> {
        if self.kind != MatrixKind::Mat {
            return None;
        }
        let (r, c) = (self.rows as usize, self.cols as usize);
        let mut data = [0i64; MAX_MATRIX_CELLS];
        for i in 0..r {
            for j in 0..c {
                data[j * r + i] = self.data[i * c + j];
            }
        }
        Some(Self {
            data,
            rows: self.cols,
            cols: self.rows,
            kind: MatrixKind::Mat,
        })
    }

    pub fn determinant(&self) -> Option<i64> {
        if self.kind != MatrixKind::Mat || self.rows != self.cols {
            return None;
        }
        let n = self.rows as usize;
        if n == 0 {
            return None;
        }
        if n == 1 {
            return Some(self.data[0]);
        }
        if n == 2 {
            let a = self.data[0];
            let b = self.data[1];
            let c = self.data[2];
            let d = self.data[3];
            let ad = fp::multiply(a, d)?;
            let bc = fp::multiply(b, c)?;
            return Some(ad.saturating_sub(bc));
        }
        if n == 3 {
            let a = self.data[0];
            let b = self.data[1];
            let c = self.data[2];
            let d = self.data[3];
            let e = self.data[4];
            let f = self.data[5];
            let g = self.data[6];
            let h = self.data[7];
            let i = self.data[8];
            let ei = fp::multiply(e, i)?;
            let fh = fp::multiply(f, h)?;
            let term1 = fp::multiply(a, ei.saturating_sub(fh))?;
            let di = fp::multiply(d, i)?;
            let fg = fp::multiply(f, g)?;
            let term2 = fp::multiply(b, di.saturating_sub(fg))?;
            let dh = fp::multiply(d, h)?;
            let eg = fp::multiply(e, g)?;
            let term3 = fp::multiply(c, dh.saturating_sub(eg))?;
            return Some(term1.saturating_sub(term2).saturating_add(term3));
        }
        let mut mat = self.data;
        let mut det = fp::SCALE;
        for col in 0..n {
            let pivot_row = (col..n).find(|&r| mat[r * n + col] != 0)?;
            if pivot_row != col {
                for j in col..n {
                    mat.swap(col * n + j, pivot_row * n + j);
                }
                det = -det;
            }
            let pivot = mat[col * n + col];
            det = fp::multiply(det, pivot)?;
            for j in col + 1..n {
                mat[col * n + j] = fp::divide(mat[col * n + j], pivot)?;
            }
            for row in col + 1..n {
                let factor = mat[row * n + col];
                if factor == 0 {
                    continue;
                }
                for j in col + 1..n {
                    let sub = fp::multiply(factor, mat[col * n + j])?;
                    mat[row * n + j] = mat[row * n + j].saturating_sub(sub);
                }
            }
        }
        Some(det)
    }

    pub fn minor(&self, r: usize, c: usize) -> Option<Self> {
        if self.kind != MatrixKind::Mat || self.rows != self.cols {
            return None;
        }
        let n = self.rows as usize;
        if n <= 1 || r >= n || c >= n {
            return None;
        }
        let mut data = [0i64; MAX_MATRIX_CELLS];
        let mut pos = 0;
        for i in 0..n {
            if i == r {
                continue;
            }
            for j in 0..n {
                if j == c {
                    continue;
                }
                data[pos] = self.data[i * n + j];
                pos += 1;
            }
        }
        Some(Self {
            data,
            rows: (n - 1) as u8,
            cols: (n - 1) as u8,
            kind: MatrixKind::Mat,
        })
    }

    pub fn cofactor(&self) -> Option<Self> {
        if self.kind != MatrixKind::Mat || self.rows != self.cols {
            return None;
        }
        let n = self.rows as usize;
        let mut data = [0i64; MAX_MATRIX_CELLS];
        for i in 0..n {
            for j in 0..n {
                let minor = self.minor(i, j)?;
                let det = minor.determinant()?;
                let sign = if (i + j) % 2 == 0 {
                    fp::SCALE
                } else {
                    -fp::SCALE
                };
                data[i * n + j] = fp::multiply(det, sign)?;
            }
        }
        Some(Self {
            data,
            rows: self.rows,
            cols: self.cols,
            kind: MatrixKind::Mat,
        })
    }

    pub fn adjugate(&self) -> Option<Self> {
        let c = self.cofactor()?;
        c.transpose()
    }

    pub fn inverse(&self) -> Option<Self> {
        let det = self.determinant()?;
        if det == 0 {
            return None;
        }
        let adj = self.adjugate()?;
        let one_over_det = fp::divide(fp::SCALE, det)?;
        adj.scalar_mul(one_over_det)
    }

    pub fn negate(&self) -> Self {
        let len = self.rows as usize * self.cols as usize;
        let mut data = [0i64; MAX_MATRIX_CELLS];
        for i in 0..len {
            data[i] = self.data[i].wrapping_neg();
        }
        Self {
            data,
            rows: self.rows,
            cols: self.cols,
            kind: self.kind,
        }
    }
}

// ─── Scientific notation constants and normalizer ────────────────────────

pub const SCI_MANTISSA_MIN: i64 = 4_294_967_296;
pub const SCI_MANTISSA_MAX: i64 = 42_949_672_959;

pub fn normalize_scientific(mantissa: i64, _exponent: i64) -> Option<(i64, i64)> {
    if mantissa == 0 {
        return Some((0, 0));
    }
    let mut m = mantissa;
    let mut e = _exponent;
    while m < SCI_MANTISSA_MIN {
        m = super::fixed_point::multiply(m, super::fixed_point::from_integer(10))?;
        e -= 1;
    }
    while m > SCI_MANTISSA_MAX {
        m = super::fixed_point::divide(m, super::fixed_point::from_integer(10))?;
        e += 1;
    }
    if e > 99 || e < -99 {
        return None;
    }
    Some((m, e))
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn test_matrix_scalar_construction() {
        let m = Matrix::scalar(42);
        assert_eq!(m.rows, 1);
        assert_eq!(m.cols, 1);
        assert_eq!(m.kind, MatrixKind::Scalar);
        assert_eq!(m.data[0], 42);
    }

    #[test]
    fn test_matrix_complex_construction() {
        let m = Matrix::complex(3, 4);
        assert_eq!(m.rows, 1);
        assert_eq!(m.cols, 2);
        assert_eq!(m.kind, MatrixKind::Complex);
        assert_eq!(m.data[0], 3);
        assert_eq!(m.data[1], 4);
    }

    #[test]
    fn test_identity() {
        let id3 = Matrix::identity(3).unwrap();
        assert_eq!(id3.rows, 3);
        assert_eq!(id3.cols, 3);
        assert_eq!(id3.kind, MatrixKind::Mat);
        for i in 0..3 {
            for j in 0..3 {
                let expected = if i == j { fp::SCALE } else { 0 };
                assert_eq!(id3.data[i * 3 + j], expected, "({},{})", i, j);
            }
        }
    }

    #[test]
    fn test_identity_out_of_range() {
        assert!(Matrix::identity(0).is_none());
        assert!(Matrix::identity(7).is_none());
    }

    #[test]
    fn test_mat_from_slice() {
        let data = [1, 2, 3, 4, 5, 6];
        let m = Matrix::mat_from_slice(&data, 2, 3).unwrap();
        assert_eq!(m.rows, 2);
        assert_eq!(m.cols, 3);
        assert_eq!(m.cell(0, 0), 1);
        assert_eq!(m.cell(0, 2), 3);
        assert_eq!(m.cell(1, 1), 5);
    }

    #[test]
    fn test_elementwise_add() {
        let a = Matrix::mat_from_slice(&[1, 2, 3, 4], 2, 2).unwrap();
        let b = Matrix::mat_from_slice(&[5, 6, 7, 8], 2, 2).unwrap();
        let c = a.elementwise_add(&b).unwrap();
        assert_eq!(c.cell(0, 0), 6);
        assert_eq!(c.cell(1, 1), 12);
    }

    #[test]
    fn test_elementwise_sub() {
        let a = Matrix::mat_from_slice(&[5, 6, 7, 8], 2, 2).unwrap();
        let b = Matrix::mat_from_slice(&[1, 2, 3, 4], 2, 2).unwrap();
        let c = a.elementwise_sub(&b).unwrap();
        assert_eq!(c.cell(0, 0), 4);
        assert_eq!(c.cell(1, 1), 4);
    }

    #[test]
    fn test_matmul_2x2() {
        let a = Matrix::mat_from_slice(&[fp::SCALE, fp::SCALE, 0, fp::SCALE], 2, 2).unwrap();
        let b = Matrix::mat_from_slice(&[fp::SCALE, 0, fp::SCALE, fp::SCALE], 2, 2).unwrap();
        let c = a.matmul(&b).unwrap();
        assert_eq!(c.rows, 2);
        assert_eq!(c.cols, 2);
        let two = 2 * fp::SCALE;
        assert_eq!(c.cell(0, 0), two);
        assert_eq!(c.cell(0, 1), fp::SCALE);
        assert_eq!(c.cell(1, 0), fp::SCALE);
        assert_eq!(c.cell(1, 1), fp::SCALE);
    }

    #[test]
    fn test_matmul_dim_mismatch() {
        let a = Matrix::mat_from_slice(&[1, 2, 3, 4, 5, 6], 2, 3).unwrap();
        let b = Matrix::mat_from_slice(&[1, 2, 3, 4], 2, 2).unwrap();
        assert!(a.matmul(&b).is_none());
    }

    #[test]
    fn test_matmul_2x3_3x2() {
        let a = Matrix::mat_from_slice(&[1, 2, 3, 4, 5, 6], 2, 3).unwrap();
        let b = Matrix::mat_from_slice(&[7, 8, 9, 10, 11, 12], 3, 2).unwrap();
        let c = a.matmul(&b).unwrap();
        assert_eq!(c.rows, 2);
        assert_eq!(c.cols, 2);
    }

    #[test]
    fn test_scalar_mul() {
        let a = Matrix::mat_from_slice(
            &[fp::SCALE, fp::SCALE * 2, fp::SCALE * 3, fp::SCALE * 4],
            2,
            2,
        )
        .unwrap();
        let b = a.scalar_mul(fp::SCALE * 2).unwrap();
        assert_eq!(b.cell(0, 0), fp::SCALE * 2);
        assert_eq!(b.cell(0, 1), fp::SCALE * 4);
        assert_eq!(b.cell(1, 0), fp::SCALE * 6);
        assert_eq!(b.cell(1, 1), fp::SCALE * 8);
    }

    #[test]
    fn test_scalar_add() {
        let a = Matrix::mat_from_slice(&[1, 2, 3, 4], 2, 2).unwrap();
        let b = a.scalar_add(fp::SCALE).unwrap();
        assert_eq!(b.cell(0, 0), 1 + fp::SCALE);
        assert_eq!(b.cell(1, 1), 4 + fp::SCALE);
    }

    #[test]
    fn test_transpose_2x3() {
        let a = Matrix::mat_from_slice(&[1, 2, 3, 4, 5, 6], 2, 3).unwrap();
        let t = a.transpose().unwrap();
        assert_eq!(t.rows, 3);
        assert_eq!(t.cols, 2);
        assert_eq!(t.cell(0, 0), 1);
        assert_eq!(t.cell(1, 0), 2);
        assert_eq!(t.cell(2, 1), 6);
    }

    #[test]
    fn test_determinant_1x1() {
        let m = Matrix::mat_from_slice(&[fp::SCALE * 5], 1, 1).unwrap();
        assert_eq!(m.determinant(), Some(fp::SCALE * 5));
    }

    #[test]
    fn test_determinant_2x2() {
        let m = Matrix::mat_from_slice(&[fp::SCALE, 0, 0, fp::SCALE], 2, 2).unwrap();
        assert_eq!(m.determinant(), Some(fp::SCALE));
    }

    #[test]
    fn test_determinant_3x3() {
        let one = fp::SCALE;
        let m = Matrix::mat_from_slice(&[one, 0, 0, 0, one, 0, 0, 0, one], 3, 3).unwrap();
        assert_eq!(m.determinant(), Some(one));
    }

    #[test]
    fn test_determinant_singular() {
        let one = fp::SCALE;
        let m = Matrix::mat_from_slice(&[one, one, one, one], 2, 2).unwrap();
        assert_eq!(m.determinant(), Some(0));
    }

    #[test]
    fn test_determinant_not_square() {
        let m = Matrix::mat_from_slice(&[1, 2, 3, 4, 5, 6], 2, 3).unwrap();
        assert!(m.determinant().is_none());
    }

    #[test]
    fn test_negate() {
        let m = Matrix::mat_from_slice(&[1, -2, 3, -4], 2, 2).unwrap();
        let n = m.negate();
        assert_eq!(n.cell(0, 0), -1);
        assert_eq!(n.cell(0, 1), 2);
        assert_eq!(n.cell(1, 0), -3);
        assert_eq!(n.cell(1, 1), 4);
    }

    #[test]
    fn test_to_complex() {
        let s = Matrix::scalar(5);
        assert_eq!(s.to_complex(), Some(Complex::from_real(5)));
        let c = Matrix::complex(3, 4);
        assert_eq!(c.to_complex(), Some(Complex::new(3, 4)));
        let m = Matrix::mat_from_slice(&[1, 2], 1, 2).unwrap();
        assert!(m.to_complex().is_none());
    }

    #[test]
    fn test_determinant_4x4_identity() {
        let id4 = Matrix::identity(4).unwrap();
        assert_eq!(id4.determinant(), Some(fp::SCALE));
    }
}
