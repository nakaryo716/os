use core::fmt::Write;

use crate::x86::{busy_loop_hint, read_io_port_u8, write_io_port_u8};

pub struct SerialPort {
    base: u16,
}

impl SerialPort {
    /// # Safety
    ///  The specified serial port must be unique within the program.
    pub unsafe fn new(base: u16) -> Self {
        Self { base }
    }

    /// # Safety
    ///  This serial port(com1) must be unique within the program.
    pub unsafe fn new_for_com1() -> Self {
        Self { base: 0x3f8 }
    }

    pub fn init(&mut self) {
        // Disable all interrupts
        write_io_port_u8(self.base + 1, 0x00);
        // Enable DLAB (set baud rate divisor)
        write_io_port_u8(self.base + 3, 0x80);
        // Set Divisor
        // baud rate = (115200 / BAUD_DIVISOR)
        const BAUD_DIVISOR: u16 = 0x0001;
        write_io_port_u8(self.base, (BAUD_DIVISOR & 0xff) as u8);
        write_io_port_u8(self.base + 1, (BAUD_DIVISOR >> 8) as u8);
        // 8 bits, no parity, one stop bit
        // Disable DLAB
        write_io_port_u8(self.base + 3, 0x03);
        // Enable FIFO, clear them, with 14-byte threshold
        write_io_port_u8(self.base + 2, 0xC7);
        // IRQs enabled, RTS/DSR set
        write_io_port_u8(self.base + 4, 0x0B);
    }

    pub fn send_char(&mut self, c: char) {
        while (read_io_port_u8(self.base + 5) & 0x20) == 0 {
            busy_loop_hint();
        }

        write_io_port_u8(self.base, c as u8);
    }

    pub fn send_str(&mut self, s: &str) {
        // TODO: this implementation lead bugs related of multi byte character
        for c in s.chars() {
            self.send_char(c);
        }
    }
}

impl Write for SerialPort {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.send_str(s);
        Ok(())
    }
}
