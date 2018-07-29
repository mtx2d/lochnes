use std::fmt;
use rom::Rom;

pub struct Nes {
    pub rom: Rom,
    pub ram: [u8; 0x0800],
    pub cpu: Cpu,
    pub ppu: Ppu,
}

impl Nes {
    pub fn new_from_rom(rom: Rom) -> Self {
        let ram = [0; 0x0800];
        let cpu = Cpu::new();
        let ppu = Ppu::new();

        let mut nes = Nes {
            rom,
            ram,
            cpu,
            ppu,
        };

        let reset_addr = nes.read_u16(0xFFFC);

        nes.cpu.pc = reset_addr;

        nes
    }

    pub fn read_u8(&self, addr: u16) -> u8 {
        let mapper = self.rom.header.mapper;
        if mapper != 0 {
            unimplemented!("Unhandled mapper: {}", mapper);
        }

        match addr {
            0x0000...0x07FF => {
                self.ram[addr as usize]
            }
            0x2002 => {
                self.ppu.ppustatus()
            }
            0x8000...0xFFFF => {
                let rom_offset = addr - 0x8000;
                let mapped_addr = rom_offset as usize % self.rom.prg_rom.len();
                self.rom.prg_rom[mapped_addr]
            }
            _ => {
                unimplemented!("Unhandled read from address: 0x{:X}", addr);
            }
        }
    }

    pub fn read_i8(&self, addr: u16) -> i8 {
        self.read_u8(addr) as i8
    }

    pub fn read_u16(&self, addr: u16) -> u16 {
        let lo = self.read_u8(addr);
        let hi = self.read_u8(addr + 1);

        lo as u16 | ((hi as u16) << 8)
    }

    pub fn write_u8(&mut self, addr: u16, value: u8) {
        let mapper = self.rom.header.mapper;
        if mapper != 0 {
            unimplemented!("Unhandled mapper: {}", mapper);
        }

        match addr {
            0x0000...0x07FF => {
                self.ram[addr as usize] = value;
            }
            0x2000 => {
                self.ppu.set_ppuctrl(value);
            }
            0x2001 => {
                self.ppu.set_ppumask(value);
            }
            0x2005 => {
                self.ppu.write_ppuscroll(value);
            }
            0x8000...0xFFFF => {
                let rom_offset = addr - 0x8000;
                let mapped_addr = rom_offset as usize % self.rom.prg_rom.len();
                self.rom.prg_rom[mapped_addr] = value;
            }
            _ => {
                unimplemented!("Unhandled write to address: 0x{:X}", addr);
            }
        }
    }

    fn step_cpu(&mut self) -> CpuStep {
        let pc = self.cpu.pc;
        let next_pc;

        let opcode = self.read_u8(pc);
        let opcode = Opcode::from_u8(opcode);

        let op;
        match opcode {
            Opcode::AndImm => {
                let a = self.cpu.a;
                let value = self.read_u8(pc + 1);
                let a = a & value;
                self.cpu.a = a;

                self.cpu.set_flags(CpuFlags::Z, a == 0);
                self.cpu.set_flags(CpuFlags::N, (a & 0b_1000_0000) != 0);

                next_pc = pc + 2;
                op = Op::AndImm { value };
            }
            Opcode::Beq => {
                let addr_offset = self.read_i8(pc + 1);

                let pc_after = pc + 2;
                if self.cpu.p.contains(CpuFlags::Z) {
                    // TODO: Handle offset past page! With that, `i8` shouldn't
                    // be necessary
                    next_pc = (pc_after as i16 + addr_offset as i16) as u16;
                }
                else {
                    next_pc = pc_after;
                }
                op = Op::Beq { addr_offset };
            }
            Opcode::Bne => {
                let addr_offset = self.read_i8(pc + 1);

                let pc_after = pc + 2;
                if self.cpu.p.contains(CpuFlags::Z) {
                    next_pc = pc_after;
                }
                else {
                    // TODO: Handle offset past page! With that, `i8` shouldn't
                    // be necessary
                    next_pc = (pc_after as i16 + addr_offset as i16) as u16;
                }
                op = Op::Bne { addr_offset };
            }
            Opcode::Bpl => {
                let addr_offset = self.read_i8(pc + 1);

                let pc_after = pc + 2;
                if self.cpu.p.contains(CpuFlags::N) {
                    next_pc = pc_after;
                }
                else {
                    // TODO: Handle offset past page! With that, `i8` shouldn't
                    // be necessary
                    next_pc = (pc_after as i16 + addr_offset as i16) as u16;
                }
                op = Op::Bpl { addr_offset };
            }
            Opcode::Cld => {
                self.cpu.set_flags(CpuFlags::D, false);
                next_pc = pc + 1;
                op = Op::Cld;
            }
            Opcode::DecZero => {
                let zero_page = self.read_u8(pc + 1);
                let addr = zero_page as u16;
                let value = self.read_u8(addr);
                let value = value.wrapping_sub(1);
                self.write_u8(addr, value);

                self.cpu.set_flags(CpuFlags::Z, value == 0);
                self.cpu.set_flags(CpuFlags::N, (value & 0b_1000_0000) != 0);

                next_pc = pc + 2;
                op = Op::DecZero { zero_page };
            }
            Opcode::Dey => {
                let y = self.cpu.y.wrapping_sub(1);
                self.cpu.set_flags(CpuFlags::Z, y == 0);
                self.cpu.set_flags(CpuFlags::N, (y & 0b_1000_0000) != 0);
                self.cpu.y = y;

                next_pc = pc + 1;
                op = Op::Dey;
            }
            Opcode::JmpAbs => {
                let addr = self.read_u16(pc + 1);

                next_pc = addr;
                op = Op::JmpAbs { addr };
            }
            Opcode::Jsr => {
                let addr = self.read_u16(pc + 1);
                let ret_pc = pc.wrapping_add(3);
                let push_pc = ret_pc.wrapping_sub(1);
                let push_pc_hi = ((0xFF00 & push_pc) >> 8) as u8;
                let push_pc_lo = (0x00FF & push_pc) as u8;
                let stack_offset_hi = self.cpu.s;
                let stack_offset_lo = self.cpu.s.wrapping_sub(1);
                let stack_addr_hi = 0x0100 & stack_offset_hi as u16;
                let stack_addr_lo = 0x0100 & stack_offset_lo as u16;

                self.write_u8(stack_addr_hi, push_pc_hi);
                self.write_u8(stack_addr_lo, push_pc_lo);

                self.cpu.s = self.cpu.s.wrapping_sub(2);

                next_pc = addr;
                op = Op::Jsr { addr };

            }
            Opcode::LdaAbs => {
                let addr = self.read_u16(pc + 1);
                let value = self.read_u8(addr);
                self.cpu.a = value;
                next_pc = pc + 3;
                op = Op::LdaAbs { addr };
            }
            Opcode::LdaImm => {
                let value = self.read_u8(pc + 1);
                self.cpu.a = value;
                next_pc = pc + 2;
                op = Op::LdaImm { value };
            }
            Opcode::LdxImm => {
                let value = self.read_u8(pc + 1);
                self.cpu.x = value;
                next_pc = pc + 2;
                op = Op::LdxImm { value };
            }
            Opcode::LdyImm => {
                let value = self.read_u8(pc + 1);
                self.cpu.y = value;
                next_pc = pc + 2;
                op = Op::LdyImm { value };
            }
            Opcode::Sei => {
                self.cpu.set_flags(CpuFlags::I, true);
                next_pc = pc + 1;
                op = Op::Sei;
            }
            Opcode::StaAbs => {
                let a = self.cpu.a;
                let addr = self.read_u16(pc + 1);
                self.write_u8(addr, a);
                next_pc = pc + 3;
                op = Op::StaAbs { addr };
            }
            Opcode::StaZero => {
                let a = self.cpu.a;
                let zero_page = self.read_u8(pc + 1);
                let addr = zero_page as u16;
                self.write_u8(addr, a);
                next_pc = pc + 2;
                op = Op::StaZero { zero_page };
            }
            Opcode::StaIndY => {
                let a = self.cpu.a;
                let y = self.cpu.y;
                let target_addr_base = self.read_u8(pc + 1);

                // TODO: Is this right? Does the target address
                // wrap around the zero page?
                let addr_base = self.read_u16(target_addr_base as u16);
                let addr = addr_base.wrapping_add(y as u16);
                self.write_u8(addr, a);

                next_pc = pc + 2;
                op = Op::StaIndY { target_addr_base };
            }
            Opcode::StyZero => {
                let y = self.cpu.y;
                let zero_page = self.read_u8(pc + 1);
                let addr = zero_page as u16;
                self.write_u8(addr, y);
                next_pc = pc + 2;
                op = Op::StyZero { zero_page };
            }
            Opcode::Txs => {
                let x = self.cpu.x;
                self.cpu.s = x;
                next_pc = pc + 1;
                op = Op::Txs;
            }
        }

        self.cpu.pc = next_pc;

        debug_assert_eq!(Opcode::from(&op), opcode);

        CpuStep { pc, op }
    }

    fn step_ppu(&mut self) {
        let cycle = self.ppu.cycle;
        // let frame = cycle / 89_342;
        let frame_cycle = cycle % 89_342;
        let scanline = frame_cycle / 341;
        let scanline_cycle = frame_cycle % 341;

        if scanline == 240 && scanline_cycle == 1 {
            self.ppu.status.set(PpuStatusFlags::VBLANK_STARTED, true);
        }
        self.ppu.cycle += 1;
    }

    pub fn step(&mut self) -> CpuStep {
        let cpu_step = self.step_cpu();
        self.step_ppu();
        self.step_ppu();
        self.step_ppu();

        cpu_step
    }
}

#[derive(Debug)]
pub struct Cpu {
    pub pc: u16,
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub s: u8,
    pub p: CpuFlags,
}

impl Cpu {
    fn new() -> Self {
        Cpu {
            pc: 0,
            a: 0,
            x: 0,
            y: 0,
            s: 0xFD,
            p: CpuFlags::from_bits_truncate(0x34),
        }
    }

    fn set_flags(&mut self, flags: CpuFlags, value: bool) {
        // TODO: Prevent the break (`B`) and unused (`U`) flags
        // from being changed!
        self.p.set(flags, value);
    }
}

bitflags! {
    pub struct CpuFlags: u8 {
        /// Carry flag: set when an arithmetic operation resulted in a carry.
        const C = 1 << 0;

        /// Zero flag: set when an operation results in 0.
        const Z = 1 << 1;

        /// Interrupt disable flag: set to disable CPU interrupts.
        const I = 1 << 2;

        /// Decimal mode flag: exists for compatibility with the 6502 (which
        /// used it for decimal arithmetic), but ignored by the
        /// Rioch 2A03/2A07 processors used in the NES/Famicom.
        const D = 1 << 3;

        /// Break flag: set when BRK or PHP are called, cleared when
        /// the /IRQ or /NMI interrupts are called. When PLP or RTI are called,
        /// this flag is unaffected.
        ///
        /// In other words, this flag isn't exactly "set" or "cleared"-- when an
        /// interrupt happens, the status register is pushed to the stack. If
        /// the interrupt was an /IRQ or /NMI interrupt, the value pushed to the
        /// stack will have this bit cleared; if the interrupt was caused by
        /// BRK (or if the PHP instruction is used), then the value pushed to
        /// the stack will have this bit set.
        const B = 1 << 4;

        /// Unused: this flag is always set to 1
        const U = 1 << 5;

        /// Overflow flag: set when an operation resulted in a signed overflow.
        const V = 1 << 6;

        /// Negative flag: set when an operation resulted in a negative value,
        /// i.e. when the most significant bit (bit 7) is set.
        const N = 1 << 7;
    }
}

#[derive(Debug, EnumKind)]
#[enum_kind(Opcode)]
pub enum Op {
    AndImm { value: u8 },
    Beq { addr_offset: i8 },
    Bne { addr_offset: i8 },
    Bpl { addr_offset: i8 },
    Cld,
    DecZero { zero_page: u8 },
    Dey,
    JmpAbs { addr: u16 },
    Jsr { addr: u16 },
    LdaAbs { addr: u16 },
    LdaImm { value: u8 },
    LdxImm { value: u8 },
    LdyImm { value: u8 },
    Sei,
    StaAbs { addr: u16 },
    StaZero { zero_page: u8 },
    StaIndY { target_addr_base: u8 },
    StyZero { zero_page: u8 },
    Txs,
}

impl Opcode {
    fn from_u8(opcode: u8) -> Self {
        match opcode {
            0x10 => Opcode::Bpl,
            0x20 => Opcode::Jsr,
            0x29 => Opcode::AndImm,
            0x4C => Opcode::JmpAbs,
            0x78 => Opcode::Sei,
            0x84 => Opcode::StyZero,
            0x85 => Opcode::StaZero,
            0x88 => Opcode::Dey,
            0x8D => Opcode::StaAbs,
            0x91 => Opcode::StaIndY,
            0x9A => Opcode::Txs,
            0xA0 => Opcode::LdyImm,
            0xA2 => Opcode::LdxImm,
            0xA9 => Opcode::LdaImm,
            0xAD => Opcode::LdaAbs,
            0xC6 => Opcode::DecZero,
            0xD0 => Opcode::Bne,
            0xD8 => Opcode::Cld,
            0xF0 => Opcode::Beq,
            opcode => {
                unimplemented!("Unhandled opcode: 0x{:X}", opcode);
            }
        }
    }
}

impl fmt::Display for Opcode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mnemonic = match self {
            Opcode::AndImm => "AND",
            Opcode::Beq => "BEQ",
            Opcode::Bne => "BNE",
            Opcode::Bpl => "BPL",
            Opcode::Cld => "CLD",
            Opcode::DecZero => "DEC",
            Opcode::Dey => "DEY",
            Opcode::JmpAbs => "JMP",
            Opcode::Jsr => "JSR",
            Opcode::LdaAbs | Opcode::LdaImm => "LDA",
            Opcode::LdxImm => "LDX",
            Opcode::LdyImm => "LDY",
            Opcode::Sei => "SEI",
            Opcode::StaAbs | Opcode::StaZero | Opcode::StaIndY => "STA",
            Opcode::StyZero => "STY",
            Opcode::Txs => "TXS",
        };
        write!(f, "{}", mnemonic)?;
        Ok(())
    }
}

impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let opcode = Opcode::from(self);
        match self {
            Op::Cld | Op::Sei | Op::Txs | Op::Dey => {
                write!(f, "{}", opcode)?;
            }
            Op::LdaAbs { addr }
            | Op::StaAbs { addr }
            | Op::JmpAbs { addr }
            | Op::Jsr { addr } => {
                write!(f, "{} ${:04X}", opcode, addr)?;
            }
            Op::DecZero { zero_page }
            | Op::StaZero { zero_page }
            | Op::StyZero { zero_page }=> {
                write!(f, "{} ${:04X}", opcode, *zero_page as u16)?;
            }
            Op::StaIndY { target_addr_base } => {
                write!(f, "{} (${:02X}),Y", opcode, target_addr_base)?;
            }
            Op::AndImm { value }
            | Op::LdaImm { value }
            | Op::LdxImm { value }
            | Op::LdyImm { value } => {
                write!(f, "{} #${:02X}", opcode, value)?;
            }
            Op::Beq { addr_offset }
            | Op::Bne { addr_offset }
            | Op::Bpl { addr_offset } => {
                if *addr_offset >= 0 {
                    write!(f, "{} _ + #${:02X}", opcode, addr_offset)?;
                }
                else {
                    let abs_offset = -(*addr_offset as i16);
                    write!(f, "{} _ - #${:02X}", opcode, abs_offset)?;
                }
            }
        }
        Ok(())
    }
}

pub struct CpuStep {
    pub pc: u16,
    pub op: Op,
}

pub struct Ppu {
    cycle: u64,

    ctrl: PpuCtrlFlags,
    mask: PpuMaskFlags,
    status: PpuStatusFlags,
    oam_addr: u8,
    scroll: u16,
    addr: u16,

    // Latch used for writing to PPUSCROLL and PPUADDR (toggles after a write
    // to each, used to determine if the high bit or low bit is being written).
    scroll_addr_latch: bool,

    pattern_tables: [[u8; 0x1000]; 2],
    nametables: [[u8; 0x0400]; 4],
    oam: [u8; 0x0100],
}

impl Ppu {
    fn new() -> Self {
        Ppu {
            cycle: 0,
            ctrl: PpuCtrlFlags::from_bits_truncate(0x00),
            mask: PpuMaskFlags::from_bits_truncate(0x00),
            status: PpuStatusFlags::from_bits_truncate(0x00),
            oam_addr: 0x00,
            scroll: 0x0000,
            addr: 0x0000,
            scroll_addr_latch: false,
            pattern_tables: [[0; 0x1000]; 2],
            nametables: [[0; 0x0400]; 4],
            oam: [0; 0x0100],
        }
    }

    fn set_ppuctrl(&mut self, value: u8) {
        self.ctrl = PpuCtrlFlags::from_bits_truncate(value);
    }

    fn set_ppumask(&mut self, value: u8) {
        self.mask = PpuMaskFlags::from_bits_truncate(value);
    }

    fn write_ppuscroll(&mut self, value: u8) {
        let latch = self.scroll_addr_latch;

        if latch {
            let scroll_lo = self.scroll & 0x00FF;
            let scroll_hi = (value as u16) << 8;
            self.scroll = scroll_lo | scroll_hi;
        }
        else {
            let scroll_lo = self.scroll as u16;
            let scroll_hi = self.scroll & 0xFF00;
            self.scroll = scroll_lo | scroll_hi;
        }

        self.scroll_addr_latch = !latch;
    }

    fn ppustatus(&self) -> u8 {
        self.status.bits()
    }
}

bitflags! {
    pub struct PpuCtrlFlags: u8 {
        const NAMETABLE_LO = 1 << 0;
        const NAMETABLE_HI = 1 << 1;
        const VRAM_ADDR_INCREMENT = 1 << 2;
        const SPRITE_PATTERN_TABLE_ADDR = 1 << 3;
        const BACKGROUND_PATTERN_TABLE_ADDR = 1 << 4;
        const SPRITE_SIZE = 1 << 5;
        const PPU_MASTER_SLAVE_SELECT = 1 << 6;
        const VBLANK_INTERRUPT = 1 << 7;
    }
}

bitflags! {
    pub struct PpuMaskFlags: u8 {
        const GREYSCALE = 1 << 0;
        const SHOW_BACKGROUND_IN_LEFT_MARGIN = 1 << 1;
        const SHOW_SPRITES_IN_LEFT_MARGIN = 1 << 2;
        const SHOW_BACKGROUND = 1 << 3;
        const SHOW_SPRITES = 1 << 4;
        const EMPHASIZE_RED = 1 << 5;
        const EMPHASIZE_GREEN = 1 << 6;
        const EMPHASIZE_BLUE = 1 << 7;
    }
}

bitflags! {
    pub struct PpuStatusFlags: u8 {
        // NOTE: Bits 0-4 are unused (but result in bits read from
        // the PPU's latch)
        const SPRITE_OVERFLOW = 1 << 5;
        const SPRITE_ZERO_HIT = 1 << 6;
        const VBLANK_STARTED = 1 << 7;
    }
}
