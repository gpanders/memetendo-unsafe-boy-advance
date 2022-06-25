#![allow(clippy::module_name_repetitions)]

use intbits::Bits;

#[allow(clippy::cast_possible_truncation)]
pub fn write_hword_as_bytes<T: Bus + ?Sized>(bus: &mut T, addr: u32, value: u16) {
    bus.write_byte(addr, value as u8);
    bus.write_byte(addr.wrapping_add(1), value.bits(8..) as _);
}

pub trait Bus {
    fn read_byte(&mut self, addr: u32) -> u8;

    fn read_hword(&mut self, addr: u32) -> u16 {
        let lo = self.read_byte(addr);
        let hi = self.read_byte(addr.wrapping_add(1));

        u16::from_le_bytes([lo, hi])
    }

    fn read_word(&mut self, addr: u32) -> u32 {
        let lo = self.read_hword(addr);
        let hi = self.read_hword(addr.wrapping_add(2));

        u32::from(lo).with_bits(16.., hi.into())
    }

    fn write_byte(&mut self, _addr: u32, _value: u8) {}

    fn write_hword(&mut self, addr: u32, value: u16) {
        write_hword_as_bytes(self, addr, value);
    }

    #[allow(clippy::cast_possible_truncation)]
    fn write_word(&mut self, addr: u32, value: u32) {
        self.write_hword(addr, value as u16);
        self.write_hword(addr.wrapping_add(2), value.bits(16..) as _);
    }

    fn prefetch_instr(&mut self, _addr: u32) {}
}

impl Bus for &[u8] {
    fn read_byte(&mut self, addr: u32) -> u8 {
        self[addr as usize]
    }
}

impl Bus for &mut [u8] {
    fn read_byte(&mut self, addr: u32) -> u8 {
        self[addr as usize]
    }

    fn write_byte(&mut self, addr: u32, value: u8) {
        self[addr as usize] = value;
    }
}

pub trait BusAlignedExt {
    fn read_hword_aligned(&mut self, addr: u32) -> u16;
    fn read_word_aligned(&mut self, addr: u32) -> u32;

    fn write_hword_aligned(&mut self, addr: u32, value: u16);
    fn write_word_aligned(&mut self, addr: u32, value: u32);
}

impl<T: Bus> BusAlignedExt for T {
    fn read_hword_aligned(&mut self, addr: u32) -> u16 {
        self.read_hword(addr & !1)
    }

    fn read_word_aligned(&mut self, addr: u32) -> u32 {
        self.read_word(addr & !0b11)
    }

    fn write_hword_aligned(&mut self, addr: u32, value: u16) {
        self.write_hword(addr & !1, value);
    }

    fn write_word_aligned(&mut self, addr: u32, value: u32) {
        self.write_word(addr & !0b11, value);
    }
}

#[cfg(test)]
pub(super) mod tests {
    use std::cell::Cell;

    use super::*;

    #[derive(Debug)]
    pub struct NullBus;

    impl Bus for NullBus {
        fn read_byte(&mut self, _addr: u32) -> u8 {
            0
        }
    }

    #[derive(Debug)]
    pub struct VecBus {
        buf: Vec<u8>,
        allow_oob: bool,
        did_oob: Cell<bool>,
    }

    impl VecBus {
        pub fn new(len: usize) -> Self {
            Self {
                buf: vec![0; len],
                allow_oob: false,
                did_oob: Cell::new(false),
            }
        }

        pub fn assert_oob(&mut self, f: &impl Fn(&mut Self)) {
            assert!(!self.allow_oob, "cannot call assert_oob recursively");
            self.allow_oob = true;
            self.did_oob.set(false);
            f(self);

            assert!(
                self.did_oob.get(),
                "expected oob VecBus access, but there was none"
            );
            self.allow_oob = false;
        }
    }

    impl Bus for VecBus {
        fn read_byte(&mut self, addr: u32) -> u8 {
            self.buf.get(addr as usize).copied().unwrap_or_else(|| {
                self.did_oob.set(true);
                assert!(
                    self.allow_oob,
                    "oob VecBus read at address {addr:#010x} (len {})",
                    self.buf.len()
                );

                0xaa
            })
        }

        fn write_byte(&mut self, addr: u32, value: u8) {
            if let Some(v) = self.buf.get_mut(addr as usize) {
                *v = value;
            } else {
                self.did_oob.set(true);
                assert!(
                    self.allow_oob,
                    "oob VecBus write at address {addr:#010x} (value {value}, len {})",
                    self.buf.len()
                );
            }
        }
    }
}
