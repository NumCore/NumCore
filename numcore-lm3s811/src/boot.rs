use core::ptr;

extern "C" {
    static _sbss: u8;
    static _ebss: u8;
    static _sdata: u8;
    static _edata: u8;
    static _sidata: u8;
}

#[no_mangle]
pub unsafe extern "C" fn Reset() -> ! {
    zero_bss_section();
    copy_data_section_from_flash();
    crate::start()
}

unsafe fn zero_bss_section() {
    let bss_start = &_sbss as *const u8 as *mut u8;
    let bss_length = (&_ebss as *const u8).offset_from(&_sbss) as usize;
    ptr::write_bytes(bss_start, 0, bss_length);
}

unsafe fn copy_data_section_from_flash() {
    let destination = &_sdata as *const u8 as *mut u8;
    let length = (&_edata as *const u8).offset_from(&_sdata) as usize;
    let source = &_sidata as *const u8;
    ptr::copy_nonoverlapping(source, destination, length);
}

#[no_mangle]
pub unsafe extern "C" fn DefaultHandler() -> ! {
    loop {}
}

#[link_section = ".vector_table.reset_vector"]
#[no_mangle]
pub static RESET_VECTOR: unsafe extern "C" fn() -> ! = Reset;

#[link_section = ".vector_table.exceptions"]
#[no_mangle]
pub static EXCEPTION_VECTORS: [Option<unsafe extern "C" fn() -> !>; 14] = [
    Some(DefaultHandler),
    Some(DefaultHandler),
    Some(DefaultHandler),
    Some(DefaultHandler),
    Some(DefaultHandler),
    None,
    None,
    None,
    None,
    Some(DefaultHandler),
    Some(DefaultHandler),
    None,
    Some(DefaultHandler),
    Some(DefaultHandler),
];
