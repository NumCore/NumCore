#![no_std]
#![no_main]

mod boot;

use core::panic::PanicInfo;

fn start() -> ! {
    numcore::runtime::start::<hal_lm3s811::Lm3s811Uart, hal_lm3s811::Ssd0303>()
}

#[panic_handler]
fn handle_panic(_panic_info: &PanicInfo) -> ! {
    hal_lm3s811::uart::transmit_bytes(b"\r\n*** PANIC - system halted ***\r\n");
    loop {}
}
