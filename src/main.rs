#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[no_mangle]
fn efi_main() {
    // println!("Hello, world!");
    loop {}
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
