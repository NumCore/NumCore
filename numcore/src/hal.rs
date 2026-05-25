pub trait Uart {
    fn init();
    fn transmit_bytes(bytes: &[u8]);
    fn transmit_byte(byte: u8);
    fn poll_byte() -> Option<u8>;
}

pub trait Display {
    type Buffer: AsMut<[u8]> + AsRef<[u8]>;
    const WIDTH: usize;
    const HEIGHT: usize;
    fn init();
    fn new_buffer() -> Self::Buffer;
    fn render(fb: &Self::Buffer);
    fn set_pixel(fb: &mut Self::Buffer, col: usize, row: usize, on: bool);
}
