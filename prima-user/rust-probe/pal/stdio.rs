// Prima single-thread PAL. The native entry shim supplies the linked hooks.
use crate::io::{self, Write};

unsafe extern "C" {
    fn __prima_std_write_byte(byte: u8) -> bool;
}

pub struct Stdin;
pub struct Stdout;
pub type Stderr = Stdout;

impl Stdin {
    pub const fn new() -> Self { Self }
}
impl io::Read for Stdin {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() { Ok(0) } else { Err(io::Error::UNSUPPORTED_PLATFORM) }
    }
}
impl Stdout {
    pub const fn new() -> Self { Self }
}
impl Write for Stdout {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        for (index, byte) in buf.iter().enumerate() {
            // SAFETY: hook takes a value only; no borrowed buffer crosses FFI.
            if !unsafe { __prima_std_write_byte(*byte) } {
                return if index == 0 {
                    Err(io::Error::from(io::ErrorKind::BrokenPipe))
                } else { Ok(index) };
            }
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> { Ok(()) }
}
pub const STDIN_BUF_SIZE: usize = 0;
pub fn is_ebadf(_: &io::Error) -> bool { false }
pub fn panic_output() -> Option<Stderr> { Some(Stdout::new()) }
