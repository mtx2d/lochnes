#![feature(generator_trait, exhaustive_patterns)]

use std::ops::{Generator, GeneratorState};
use std::pin::Pin;

use lochnes::{nes, rom, video};
use lochnes::nes::NesStep;
use lochnes::nes::ppu::PpuStep;

fn run_blargg_instr_test(test_name: &str, rom_bytes: &[u8]) {
    let rom = rom::Rom::from_bytes(rom_bytes.into_iter().cloned())
        .expect(&format!("Failed to parse test ROM {:?}", test_name));

    let nes = nes::Nes::new_from_rom(rom.clone());
    let mut video = video::NullVideo;
    let mut run_nes = nes.run(&mut video);

    // Run for a max of 120 frames, just in case the test ROM never completes
    for frame in 0..120 {
        loop {
            match Pin::new(&mut run_nes).resume() {
                GeneratorState::Yielded(NesStep::Ppu(PpuStep::Vblank)) => { break; }
                GeneratorState::Yielded(_) => { }
            }
        }

        const STATUS_TEST_IS_RUNNING: u8 = 0x80;
        const STATUS_TEST_NEEDS_RESET: u8 = 0x81;

        let status = nes.read_u8(0x6000);
        match (status, frame) {
            (_, 0) => { } // Ignore status on first frame
            (STATUS_TEST_IS_RUNNING, _) => { }
            (STATUS_TEST_NEEDS_RESET, _) => {
                unimplemented!("Verification ROM requested a reset!");
            }
            _ => { break; }
        }
    }

    let status = nes.read_u8(0x6000);

    let mut test_output = vec![];
    for i in 0x6004_u16.. {
        let byte = nes.read_u8(i);
        match byte {
            0 => { break; }
            byte => { test_output.push(byte); }
        }
    }

    let test_output = String::from_utf8_lossy(&test_output);
    println!("{}", test_output);
    assert_eq!(status, 0);

    let expected_output = format!("\n{}\n\nPassed\n", test_name);
    assert_eq!(test_output, expected_output);
}

#[test]
fn rom_blargg_instr_test_implied() {
    run_blargg_instr_test("01-implied", include_bytes!("./fixtures/nes-test-roms/nes_instr_test/rom_singles/01-implied.nes"));
}

#[test]
fn rom_blargg_instr_test_immediate() {
    run_blargg_instr_test("02-immediate", include_bytes!("./fixtures/nes-test-roms/nes_instr_test/rom_singles/02-immediate.nes"));
}

#[test]
fn rom_blargg_instr_test_zero_page() {
    run_blargg_instr_test("03-zero_page", include_bytes!("./fixtures/nes-test-roms/nes_instr_test/rom_singles/03-zero_page.nes"));
}