#![warn(clippy::pedantic)]

mod arm7tdmi;
mod bus;
mod gba;

use gba::Gba;

fn main() {
    let mut gba = Gba::new();
    gba.reset();

    // TODO: do something
    gba.step();
}
