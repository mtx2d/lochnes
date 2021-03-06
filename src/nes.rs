use crate::input::{Input, InputState};
use crate::rom::Rom;
use crate::video::Video;
use cpu::{Cpu, CpuStep};
use mapper::Mapper;
use ppu::{Ppu, PpuStep};
use std::cell::Cell;
use std::ops::{Generator, GeneratorState};
use std::pin::Pin;
use std::u8;

pub mod cpu;
pub mod mapper;
pub mod ppu;

#[derive(Clone)]
pub struct Nes<'a, I>
where
    I: NesIo,
{
    pub io: &'a I,
    input_reader: InputReader<&'a I::Input>,
    pub mapper: Mapper,
    pub ram: Cell<[u8; 0x0800]>,
    pub cpu: Cpu,
    pub ppu: Ppu,
}

impl<'a, I> Nes<'a, I>
where
    I: NesIo,
{
    pub fn new(io: &'a I, rom: Rom) -> Self {
        let ram = Cell::new([0; 0x0800]);
        let cpu = Cpu::new();
        let ppu = Ppu::new();
        let mapper = Mapper::from_rom(rom);
        let input_reader = InputReader::new(io.input());

        let nes = Nes {
            io,
            input_reader,
            mapper,
            ram,
            cpu,
            ppu,
        };

        let reset_addr = nes.read_u16(0xFFFC);

        nes.cpu.pc.set(reset_addr);

        nes
    }

    fn ram(&self) -> &[Cell<u8>] {
        let ram: &Cell<[u8]> = &self.ram;
        ram.as_slice_of_cells()
    }

    pub fn read_u8(&self, addr: u16) -> u8 {
        let ram = self.ram();

        match addr {
            0x0000..=0x07FF => ram[addr as usize].get(),
            0x2002 => self.ppu.ppustatus(),
            0x2007 => self.ppu.read_ppudata(self),
            0x4000..=0x4007 => {
                // TODO: Return APU pulse
                0x00
            }
            0x4008..=0x400B => {
                // TODO: Return APU triangle
                0x00
            }
            0x400C..=0x400F => {
                // TODO: Return APU noise
                0x00
            }
            0x4010..=0x4013 => {
                // TODO: Return APU DMC
                0x00
            }
            0x4015 => {
                // TODO: Return APU status
                0x00
            }
            0x4016 => {
                // TODO: Handle open bus behavior!
                match self.input_reader.read_port_1_data() {
                    true => 0b_0000_0001,
                    false => 0b_0000_0000,
                }
            }
            0x4017 => {
                // TODO: Return joystick state
                0x40
            }
            0x6000..=0xFFFF => self.mapper.read_u8(addr),
            _ => {
                unimplemented!("Unhandled read from address: 0x{:X}", addr);
            }
        }
    }

    pub fn read_u16(&self, addr: u16) -> u16 {
        let lo = self.read_u8(addr);
        let hi = self.read_u8(addr.wrapping_add(1));

        lo as u16 | ((hi as u16) << 8)
    }

    pub fn write_u8(&self, addr: u16, value: u8) {
        let ram = self.ram();

        match addr {
            0x0000..=0x07FF => {
                ram[addr as usize].set(value);
            }
            0x2000 => {
                self.ppu.set_ppuctrl(value);
            }
            0x2001 => {
                self.ppu.set_ppumask(value);
            }
            0x2003 => {
                self.ppu.write_oamaddr(value);
            }
            0x2004 => {
                self.ppu.write_oamdata(value);
            }
            0x2005 => {
                self.ppu.write_ppuscroll(value);
            }
            0x2006 => {
                self.ppu.write_ppuaddr(value);
            }
            0x2007 => {
                self.ppu.write_ppudata(self, value);
            }
            0x4000..=0x4007 => {
                // TODO: APU pulse
            }
            0x4008..=0x400B => {
                // TODO: APU triangle
            }
            0x400C..=0x400F => {
                // TODO: APU noise
            }
            0x4010..=0x4013 => {
                // TODO: APU DMC
            }
            0x4014 => {
                self.copy_oam_dma(value);
            }
            0x4015 => {
                // TODO: APU sound channel control
            }
            0x4016 => {
                let strobe = (value & 0b_0000_0001) != 0;
                if strobe {
                    self.input_reader.start_strobe();
                } else {
                    self.input_reader.stop_strobe();
                }
            }
            0x4017 => {
                // TODO: Implement APU frame counter
            }
            0x6000..=0xFFFF => {
                self.mapper.write_u8(addr, value);
            }
            _ => {
                unimplemented!("Unhandled write to address: 0x{:X}", addr);
            }
        }
    }

    fn push_u8(&self, value: u8) {
        let s = self.cpu.s.get();
        let stack_addr = 0x0100 | s as u16;

        self.write_u8(stack_addr, value);

        self.cpu.s.set(s.wrapping_sub(1));
    }

    fn push_u16(&self, value: u16) {
        let value_hi = ((0xFF00 & value) >> 8) as u8;
        let value_lo = (0x00FF & value) as u8;

        self.push_u8(value_hi);
        self.push_u8(value_lo);
    }

    pub fn read_ppu_u8(&self, addr: u16) -> u8 {
        let palette_ram = self.ppu.palette_ram();

        match addr {
            0x0000..=0x3EFF => self.mapper.read_ppu_u8(self, addr),
            0x3F10 => self.read_ppu_u8(0x3F00),
            0x3F14 => self.read_ppu_u8(0x3F04),
            0x3F18 => self.read_ppu_u8(0x3F08),
            0x3F1C => self.read_ppu_u8(0x3F0C),
            0x3F00..=0x3FFF => {
                let offset = (addr - 0x3F00) as usize % palette_ram.len();
                palette_ram[offset].get()
            }
            0x4000..=0xFFFF => {
                unimplemented!("Tried to read from PPU address ${:04X}", addr);
            }
        }
    }

    pub fn write_ppu_u8(&self, addr: u16, value: u8) {
        let palette_ram = self.ppu.palette_ram();

        match addr {
            0x0000..=0x3EFF => {
                self.mapper.write_ppu_u8(self, addr, value);
            }
            0x3F10 => {
                self.write_ppu_u8(0x3F00, value);
            }
            0x3F14 => {
                self.write_ppu_u8(0x3F04, value);
            }
            0x3F18 => {
                self.write_ppu_u8(0x3F08, value);
            }
            0x3F1C => {
                self.write_ppu_u8(0x3F0C, value);
            }
            0x3F00..=0x3FFF => {
                let offset = (addr - 0x3F00) as usize % palette_ram.len();
                palette_ram[offset].set(value);
            }
            0x4000..=0xFFFF => {
                unimplemented!("Tried to write to PPU address ${:04X}", addr);
            }
        }
    }

    fn copy_oam_dma(&self, page: u8) {
        let target_addr_start = self.ppu.oam_addr.get() as u16;
        let mut oam = self.ppu.oam.get();
        for index in 0x00..=0xFF {
            let source_addr = ((page as u16) << 8) | index;
            let byte = self.read_u8(source_addr);

            let target_addr = (target_addr_start + index) as usize % oam.len();
            oam[target_addr] = byte;
        }

        self.ppu.oam.set(oam);
    }

    pub fn run(&'a self) -> impl Generator<Yield = NesStep, Return = !> + 'a {
        let mut run_cpu = Cpu::run(&self);

        let mut run_ppu = Ppu::run(&self);

        move || loop {
            // TODO: Clean this up
            loop {
                match Pin::new(&mut run_cpu).resume(()) {
                    GeneratorState::Yielded(cpu_step @ CpuStep::Cycle) => {
                        yield NesStep::Cpu(cpu_step);
                        break;
                    }
                    GeneratorState::Yielded(cpu_step) => {
                        yield NesStep::Cpu(cpu_step);
                    }
                }
            }

            for _ in 0u8..3 {
                loop {
                    match Pin::new(&mut run_ppu).resume(()) {
                        GeneratorState::Yielded(ppu_step @ PpuStep::Cycle) => {
                            yield NesStep::Ppu(ppu_step);
                            break;
                        }
                        GeneratorState::Yielded(ppu_step) => {
                            yield NesStep::Ppu(ppu_step);
                        }
                    }
                }
            }
        }
    }
}

pub enum NesStep {
    Cpu(CpuStep),
    Ppu(PpuStep),
}

// A trait that encapsulates NES I/O traits (`Video` and `Input`), allowing
// code that uses `Nes` to only take or return a single generic parameter.
pub trait NesIo {
    type Video: Video;
    type Input: Input;

    fn video(&self) -> &Self::Video;
    fn input(&self) -> &Self::Input;
}

pub struct NesIoWith<V, I>
where
    V: Video,
    I: Input,
{
    pub video: V,
    pub input: I,
}

impl<V, I> NesIo for NesIoWith<V, I>
where
    V: Video,
    I: Input,
{
    type Video = V;
    type Input = I;

    fn video(&self) -> &Self::Video {
        &self.video
    }

    fn input(&self) -> &Self::Input {
        &self.input
    }
}

impl<'a, I> NesIo for &'a I
where
    I: NesIo,
{
    type Video = I::Video;
    type Input = I::Input;

    fn video(&self) -> &Self::Video {
        (*self).video()
    }

    fn input(&self) -> &Self::Input {
        (*self).input()
    }
}

#[derive(Clone)]
struct InputReader<I>
where
    I: Input,
{
    input: I,
    strobe: Cell<InputStrobe>,
}

impl<I> InputReader<I>
where
    I: Input,
{
    fn new(input: I) -> Self {
        let strobe = Cell::new(InputStrobe::Live);
        InputReader { input, strobe }
    }

    fn start_strobe(&self) {
        self.strobe.set(InputStrobe::Live);
    }

    fn stop_strobe(&self) {
        let state = self.input.input_state();
        self.strobe.set(InputStrobe::Strobed {
            state,
            read_port_1: 0,
            read_port_2: 0,
        });
    }

    fn read_port_1_data(&self) -> bool {
        match self.strobe.get() {
            InputStrobe::Live => {
                let current_state = self.input.input_state();
                current_state.joypad_1.a
            }
            InputStrobe::Strobed {
                state,
                read_port_1,
                read_port_2,
            } => {
                let data = match read_port_1 {
                    0 => state.joypad_1.a,
                    1 => state.joypad_1.b,
                    2 => state.joypad_1.select,
                    3 => state.joypad_1.start,
                    4 => state.joypad_1.up,
                    5 => state.joypad_1.down,
                    6 => state.joypad_1.left,
                    7 => state.joypad_1.right,
                    _ => true,
                };

                self.strobe.set(InputStrobe::Strobed {
                    state,
                    read_port_1: read_port_1.saturating_add(1),
                    read_port_2,
                });

                data
            }
        }
    }
}

#[derive(Clone, Copy)]
enum InputStrobe {
    Live,
    Strobed {
        state: InputState,
        read_port_1: u8,
        read_port_2: u8,
    },
}
