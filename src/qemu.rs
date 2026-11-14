use crate::x86::{halt, write_io_port_u8};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[repr(u32)]
pub enum QemuExitCode {
    Success = 0x01, // exit status 3
    Fail = 0x02,    // exit status 5
}

pub fn exit_qemu(exit_code: QemuExitCode) -> ! {
    write_io_port_u8(0xf4, exit_code as u8);
    loop {
        halt();
    }
}
