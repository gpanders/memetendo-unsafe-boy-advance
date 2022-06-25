use std::{fs, io, path::Path};

use crate::bus::Bus;

#[derive(Debug)]
pub struct Rom(Box<[u8]>);

impl Rom {
    pub fn from_file(path: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self(fs::read(path)?.into_boxed_slice()))
    }

    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Debug)]
pub struct Cartridge<'a> {
    rom: &'a Rom,
    pub sram: Box<[u8]>,
}

impl<'a> Cartridge<'a> {
    pub fn new(rom: &'a Rom) -> Result<Self, ()> {
        if rom.bytes().len() > 0x200_0000 {
            return Err(());
        }

        Ok(Self {
            rom,
            sram: vec![0; 0x1_0000].into_boxed_slice(),
        })
    }
}

impl Bus for Cartridge<'_> {
    fn read_byte(&mut self, addr: u32) -> u8 {
        self.rom.bytes().get(addr as usize).copied().unwrap_or(0)
    }
}

#[derive(Debug)]
pub struct Bios<'a> {
    rom: &'a Rom,
    readable: bool,
    prefetch_addr: u32,
}

impl<'a> Bios<'a> {
    pub fn new(rom: &'a Rom) -> Result<Self, ()> {
        if rom.bytes().len() != 0x4000 {
            return Err(());
        }

        Ok(Self {
            rom,
            readable: false,
            prefetch_addr: 0,
        })
    }

    pub fn reset(&mut self) {
        self.readable = false;
        self.prefetch_addr = 0;
    }

    pub fn update_protection(&mut self, prefetch_addr: Option<u32>) {
        self.readable = prefetch_addr.is_some();
        if let Some(addr) = prefetch_addr {
            self.prefetch_addr = addr & !0b11;
            println!("{:x} + 8", addr.wrapping_sub(8));
        }
    }
}

impl Bus for Bios<'_> {
    fn read_byte(&mut self, addr: u32) -> u8 {
        self.rom.bytes().read_byte(if self.readable {
            addr
        } else {
            self.prefetch_addr | (addr & 0b11)
        })
    }
}
