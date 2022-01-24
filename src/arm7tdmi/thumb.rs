use super::{
    reg::{NamedGeneralRegister::Pc, Registers},
    Cpu, Exception,
};

#[derive(Copy, Clone, Debug)]
enum InstructionFormat {
    MoveShiftedReg = 1,
    AddSub,
    MoveCmpAddSubImm,
    AluOp,
    HiRegOpBranchExchange,
    LoadPcRel,
    LoadStoreRel,
    LoadStoreSignExtend,
    LoadStoreImm,
    LoadStoreHword,
    LoadStoreSpRel,
    LoadAddr,
    AddSp,
    PushPopReg,
    MultiLoadStore,
    CondBranch,
    SoftwareInterrupt,
    UncondBranch,
    LongBranchWithLink,
    Undefined = 0,
}

#[must_use]
fn decode_format(instr: u16) -> InstructionFormat {
    #[allow(clippy::enum_glob_use)]
    use InstructionFormat::*;

    #[allow(clippy::cast_possible_truncation)]
    let hi8 = (instr >> 8) as u8;
    let hi6 = hi8 >> 2;
    let hi5 = hi8 >> 3;
    let hi4 = hi8 >> 4;
    let hi3 = hi8 >> 5;
    let bit9 = hi8 & 0b10 != 0;

    match (hi3, hi4, hi5, hi6, hi8, bit9) {
        (_, _, _, _, 0b1011_0000, _) => AddSp,
        (_, _, _, _, 0b1011_1111, _) => SoftwareInterrupt,
        (_, _, _, 0b01_0000, _, _) => AluOp,
        (_, _, _, 0b01_0001, _, _) => HiRegOpBranchExchange,
        (_, _, 0b0_0011, _, _, _) => AddSub,
        (_, _, 0b0_1001, _, _, _) => LoadPcRel,
        (_, _, 0b1_1100, _, _, _) => UncondBranch,
        (_, 0b0101, _, _, _, true) => LoadStoreSignExtend,
        (_, 0b0101, _, _, _, false) => LoadStoreRel,
        (_, 0b1000, _, _, _, _) => LoadStoreHword,
        (_, 0b1001, _, _, _, _) => LoadStoreSpRel,
        (_, 0b1010, _, _, _, _) => LoadAddr,
        (_, 0b1011, _, _, _, _) => PushPopReg,
        (_, 0b1100, _, _, _, _) => MultiLoadStore,
        (_, 0b1101, _, _, _, _) => CondBranch,
        (_, 0b1111, _, _, _, _) => LongBranchWithLink,
        (0b000, _, _, _, _, _) => MoveShiftedReg,
        (0b001, _, _, _, _, _) => MoveCmpAddSubImm,
        (0b011, _, _, _, _, _) => LoadStoreImm,
        _ => Undefined,
    }
}

impl Registers {
    #[must_use]
    pub(crate) fn pc_thumb_addr(&self) -> u32 {
        self.r[Pc] & !1
    }
}

impl Cpu {
    #[allow(clippy::too_many_lines)]
    pub(crate) fn execute_thumb(&mut self, instr: u16) {
        #[allow(clippy::enum_glob_use)]
        use InstructionFormat::*;

        // TODO: add to CPU cycle counts when implemented
        #[allow(clippy::match_same_arms)]
        match decode_format(instr) {
            // TODO: 1S cycle
            #[allow(clippy::cast_possible_truncation)]
            MoveShiftedReg => {
                let r_dst = usize::from(instr) & 0b111;
                let offset = (instr >> 6) as u8 & 0b1_1111;
                let value = self.reg.r[(usize::from(instr) >> 3) & 0b111];
                let op = (instr >> 11) & 0b11;

                match op {
                    // LSL{S}, ASL{S}
                    0 => self.reg.r[r_dst] = self.execute_lsl_asl(value, offset),

                    // LSR{S}, ASR{S}
                    1 | 2 => self.reg.r[r_dst] = self.execute_lsr_asr(value, offset, op == 1),

                    _ => unreachable!("format should be AddSub"),
                }
            }

            // TODO: 1S cycle
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            AddSub => {
                let r_dst = usize::from(instr) & 0b111;
                let a = self.reg.r[(usize::from(instr) >> 3) & 0b111];
                let r_or_imm = (instr >> 6) & 0b111;
                let op = (instr >> 9) & 0b11;

                let b = if op & 0b10 == 0 {
                    self.reg.r[usize::from(r_or_imm)] // register
                } else {
                    r_or_imm.into() // immediate
                };

                self.reg.r[r_dst] = if op & 1 == 0 {
                    self.execute_add(a, b)
                } else {
                    self.execute_sub(a, b)
                };
            }

            // TODO: 1S cycle
            MoveCmpAddSubImm => {
                let imm = u32::from(instr & 0b1111_1111);
                let r_dst = (usize::from(instr) >> 8) & 0b111;
                let op = (instr >> 11) & 0b11;

                match op {
                    0 => self.reg.r[r_dst] = self.execute_mov(imm),
                    1 => {
                        self.execute_sub(self.reg.r[r_dst], imm); // CMP
                    }
                    2 => self.reg.r[r_dst] = self.execute_add(self.reg.r[r_dst], imm),
                    3 => self.reg.r[r_dst] = self.execute_sub(self.reg.r[r_dst], imm),
                    _ => unreachable!(),
                }
            }

            // TODO: 1S: AND, EOR, ADC, SBC, TST, NEG, CMP, CMN, ORR, BIC, MVN
            //       1S+1I: LSL, LSR, ASR, ROR
            //       1S+mI: MUL (m=1..4; depending on MSBs of incoming Rd value)
            AluOp => {
                let r_dst = usize::from(instr) & 0b111;
                let value = self.reg.r[(usize::from(instr) >> 3) & 0b111];
                let op = (instr >> 6) & 0b1111;

                match op {
                    // AND{S}
                    0 => self.reg.r[r_dst] = self.execute_and(self.reg.r[r_dst], value),

                    // EOR{S} (logical XOR) TODO: factor
                    1 => {
                        let result = self.reg.r[r_dst] ^ value;
                        self.reg.r[r_dst] = result;
                        self.reg.cpsr.set_nz_from(result);
                    }

                    // LSL{S}
                    #[allow(clippy::cast_possible_truncation)]
                    2 => self.reg.r[r_dst] = self.execute_lsl_asl(self.reg.r[r_dst], value as _),

                    // LSR{S}, ASR{S}
                    #[allow(clippy::cast_possible_truncation)]
                    3 | 4 => {
                        self.reg.r[r_dst] =
                            self.execute_lsr_asr(self.reg.r[r_dst], value as _, op == 3);
                    }

                    // ADC{S}
                    5 => self.reg.r[r_dst] = self.execute_adc(self.reg.r[r_dst], value),

                    // SBC{S}
                    6 => self.reg.r[r_dst] = self.execute_sbc(self.reg.r[r_dst], value),

                    // ROR{S}
                    #[allow(clippy::cast_possible_truncation)]
                    7 => self.reg.r[r_dst] = self.execute_ror(self.reg.r[r_dst], value as _),

                    // TST
                    8 => {
                        self.execute_and(self.reg.r[r_dst], value);
                    }

                    // NEG{S}
                    9 => self.reg.r[r_dst] = self.execute_sub(0, value),

                    // CMP
                    10 => {
                        self.execute_sub(self.reg.r[r_dst], value);
                    }

                    // CMN
                    11 => {
                        self.execute_add(self.reg.r[r_dst], value);
                    }

                    // ORR{S} logical TODO: factor
                    12 => {
                        let result = self.reg.r[r_dst] | value;
                        self.reg.r[r_dst] = result;
                        self.reg.cpsr.set_nz_from(result);
                    }

                    // MUL{S}
                    13 => self.reg.r[r_dst] = self.execute_mul(self.reg.r[r_dst], value),

                    // BIC{S}
                    14 => self.reg.r[r_dst] = self.execute_and(self.reg.r[r_dst], !value),

                    // MVN{S} TODO: factor?
                    15 => {
                        let result = !value;
                        self.reg.r[r_dst] = result;
                        self.reg.cpsr.set_nz_from(result);
                    }

                    _ => unreachable!(),
                }
            }

            // TODO: 1S cycle for ADD, MOV, CMP
            //       2S + 1N cycles for ADD, MOV with Rd=R15 and for BX
            HiRegOpBranchExchange => {
                let r_dst_no_msb = usize::from(instr) & 0b111;
                let r_src = (usize::from(instr) >> 3) & 0b1111;
                let r_dst_msb_or_bl = instr & (1 << 7) != 0;
                let op = todo!();
            }

            LoadPcRel => todo!(),
            LoadStoreRel => todo!(),
            LoadStoreSignExtend => todo!(),
            LoadStoreImm => todo!(),
            LoadStoreHword => todo!(),
            LoadStoreSpRel => todo!(),
            LoadAddr => todo!(),
            AddSp => todo!(),
            PushPopReg => todo!(),
            MultiLoadStore => todo!(),
            CondBranch => todo!(),
            SoftwareInterrupt => self.enter_exception(Exception::SoftwareInterrupt),
            UncondBranch => todo!(),
            LongBranchWithLink => todo!(),
            Undefined => self.enter_exception(Exception::UndefinedInstr),
        }
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    fn execute_add(&mut self, a: u32, b: u32) -> u32 {
        let result = u64::from(a).wrapping_add(b.into());
        let (a_signed, b_signed) = (a as i32, b as i32);
        let (a_neg, b_neg) = (a_signed.is_negative(), b_signed.is_negative());
        let same_sign = a_neg == b_neg;

        self.reg.cpsr.overflow = same_sign && (result as i32).is_negative() != a_neg;
        self.reg.cpsr.carry = result > u32::MAX.into();
        self.reg.cpsr.set_nz_from(result as _);

        result as _
    }

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
    fn execute_sub(&mut self, a: u32, b: u32) -> u32 {
        self.execute_add(a, -(b as i32) as _)
    }

    fn execute_adc(&mut self, a: u32, b: u32) -> u32 {
        self.execute_add(a, b.wrapping_add(self.reg.cpsr.carry.into()))
    }

    fn execute_sbc(&mut self, a: u32, b: u32) -> u32 {
        self.execute_sub(a, b.wrapping_add((!self.reg.cpsr.carry).into()))
    }

    fn execute_mul(&mut self, a: u32, b: u32) -> u32 {
        let result = a.wrapping_mul(b);
        self.reg.cpsr.set_nz_from(result);
        // TODO: MUL also corrupts the carry flag (lol), but in what way?

        result
    }

    fn execute_mov(&mut self, value: u32) -> u32 {
        self.reg.cpsr.set_nz_from(value);

        value
    }

    fn execute_and(&mut self, a: u32, b: u32) -> u32 {
        let result = a & b;
        self.reg.cpsr.set_nz_from(result);

        result
    }

    fn execute_lsl_asl(&mut self, value: u32, offset: u8) -> u32 {
        let mut result = value;
        if offset != 0 {
            result <<= offset;
            self.reg.cpsr.carry = (value << (offset - 1)) & (1 << 31) != 0;
        }
        self.reg.cpsr.set_nz_from(result);

        result
    }

    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    fn execute_lsr_asr(&mut self, value: u32, offset: u8, logical: bool) -> u32 {
        let mut result = value;
        if offset != 0 {
            result = if logical {
                value >> offset
            } else {
                ((value as i32) >> offset) as _
            };
            self.reg.cpsr.carry = (value >> (offset - 1)) & 1 != 0;
        }
        self.reg.cpsr.set_nz_from(result);

        result
    }

    fn execute_ror(&mut self, value: u32, offset: u8) -> u32 {
        let result = value.rotate_right(offset.into());
        self.reg.cpsr.carry = (value >> (offset - 1)) & 1 != 0;
        self.reg.cpsr.set_nz_from(result);

        result
    }
}

#[allow(clippy::unusual_byte_groupings, clippy::cast_sign_loss)]
#[allow(clippy::unnecessary_cast)] // this lint doesn't detect negative literals properly
#[cfg(test)]
mod tests {
    use super::*;
    use crate::arm7tdmi::reg::{GeneralRegisters, StatusRegister};

    fn test_instr(
        before: impl Fn(&mut Cpu),
        instr: u16,
        expected_rs: &GeneralRegisters,
        expected_cspr: StatusRegister,
    ) {
        let mut cpu = Cpu::new();
        cpu.reset();
        cpu.reg.cpsr.irq_disabled = false;
        cpu.reg.cpsr.fiq_disabled = false;
        before(&mut cpu);
        cpu.execute_thumb(instr);

        assert_eq!(cpu.reg.r, *expected_rs);
        assert_eq!(cpu.reg.cpsr, expected_cspr);
    }

    macro_rules! test_instr {
        ($before:expr, $instr:expr, $expected_rs:expr, $($expected_cspr_flags:ident)|*) => {
            #[allow(unused_mut)]
            let mut expected_cspr = StatusRegister::default();
            $(
                test_instr!(@expand &mut expected_cspr, $expected_cspr_flags);
            )*

            test_instr($before, $instr, &GeneralRegisters($expected_rs), expected_cspr);
        };

        ($instr:expr, $expected_rs:expr, $($expected_cspr_flags:ident)|*) => {
            test_instr!(|_| {}, $instr, $expected_rs, $($expected_cspr_flags)|*);
        };

        (@expand $expected_cspr:expr, $flag:ident) => (
            $expected_cspr.$flag = true;
        );
    }

    #[test]
    fn execute_thumb_move_shifted_reg() {
        // LSL{S} Rd,Rs,#Offset
        test_instr!(
            |cpu: &mut Cpu| cpu.reg.r[1] = 0b10,
            0b000_00_00011_001_100, // LSL R4,R1,#3
            [0, 0b10, 0, 0, 0b10_000, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        );
        test_instr!(
            |cpu: &mut Cpu| cpu.reg.r[7] = 1,
            0b000_00_01111_111_000, // LSL R0,R7,#15
            [1 << 15, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
        );
        test_instr!(
            |cpu: &mut Cpu| cpu.reg.r[7] = 1 << 31,
            0b000_00_00001_111_000, // LSL R0,R7,#1
            [0, 0, 0, 0, 0, 0, 0, 1 << 31, 0, 0, 0, 0, 0, 0, 0, 0],
            carry | zero
        );
        test_instr!(
            0b000_00_01010_111_000, // LSL R0,R7,#10
            [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            zero
        );
        test_instr!(
            |cpu: &mut Cpu| cpu.reg.r[0] = u32::MAX,
            0b000_00_00000_000_000, // LSL R0,R0,#0
            [u32::MAX, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            negative
        );

        // LSR{S} Rd,Rs,#Offset
        test_instr!(
            |cpu: &mut Cpu| cpu.reg.r[1] = 0b100,
            0b000_01_00011_001_100, // LSR R4,R1,#2
            [0, 0b100, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            zero | carry
        );
        test_instr!(
            |cpu: &mut Cpu| cpu.reg.r[1] = 0b10,
            0b000_01_00011_001_100, // LSR R4,R1,#2
            [0, 0b10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            zero
        );
        test_instr!(
            |cpu: &mut Cpu| cpu.reg.r[7] = 1 << 31,
            0b000_01_11111_111_111, // LSR R7,R7,#31
            [0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
        );
        test_instr!(
            |cpu: &mut Cpu| cpu.reg.r[7] = 1 << 31,
            0b000_01_00000_111_111, // LSR R7,R7,#0
            [0, 0, 0, 0, 0, 0, 0, 1 << 31, 0, 0, 0, 0, 0, 0, 0, 0],
            negative
        );

        // ASR{S} Rd,Rs,#Offset
        test_instr!(
            |cpu: &mut Cpu| cpu.reg.r[7] = 1 << 31,
            0b000_10_11111_111_111, // ASR R7,R7,#31
            [0, 0, 0, 0, 0, 0, 0, u32::MAX, 0, 0, 0, 0, 0, 0, 0, 0],
            negative
        );
        #[rustfmt::skip]
        test_instr!(
            |cpu: &mut Cpu| cpu.reg.r[5] = !(1 << 31),
            0b000_10_00001_101_000, // ASR R0,R5,#1
            [!(0b11 << 30), 0, 0, 0, 0, !(1 << 31), 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            carry
        );
    }

    #[test]
    fn execute_thumb_add_sub() {
        // ADD{S} Rd,Rs,Rn
        test_instr!(
            |cpu: &mut Cpu| {
                cpu.reg.r[1] = 13;
                cpu.reg.r[7] = 7;
            },
            0b00011_00_111_001_100, // ADD R4,R1,R7
            [0, 13, 0, 0, 20, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0, 0],
        );
        test_instr!(
            |cpu: &mut Cpu| {
                cpu.reg.r[7] = 1;
            },
            0b00011_00_111_111_111, // ADD R7,R7,R7
            [0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0],
        );
        test_instr!(
            |cpu: &mut Cpu| {
                cpu.reg.r[6] = u32::MAX;
                cpu.reg.r[7] = 1;
            },
            0b00011_00_111_110_000, // ADD R0,R6,R7
            [0, 0, 0, 0, 0, 0, u32::MAX, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            carry | zero
        );
        test_instr!(
            |cpu: &mut Cpu| {
                cpu.reg.r[0] = -5 as _;
                cpu.reg.r[1] = -10 as _;
            },
            0b00011_00_000_001_010, // ADD R2,R1,R0
            [-5 as _, -10 as _, -15 as _, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            negative | carry
        );
        #[rustfmt::skip]
        test_instr!(
            |cpu: &mut Cpu| {
                cpu.reg.r[0] = i32::MIN as _;
                cpu.reg.r[1] = -1 as _;
            },
            0b00011_00_000_001_010, // ADD R2,R1,R0
            [i32::MIN as _, -1 as _, i32::MIN.wrapping_sub(1) as _, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            carry | overflow
        );

        // SUB{S} Rd,Rs,Rn
        #[rustfmt::skip]
        test_instr!(
            |cpu: &mut Cpu| {
                cpu.reg.r[3] = i32::MIN as _;
                cpu.reg.r[6] = i32::MAX as _;
            },
            0b00011_01_110_011_000, // SUB R0,R3,R6
            [1, 0, 0, i32::MIN as _, 0, 0, i32::MAX as _, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            carry | overflow
        );
        test_instr!(
            |cpu: &mut Cpu| cpu.reg.r[0] = -5 as _,
            0b00011_01_000_000_010, // SUB R2,R0,R0
            [-5 as _, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            carry | zero
        );
        test_instr!(
            |cpu: &mut Cpu| {
                cpu.reg.r[0] = 5;
                cpu.reg.r[1] = -10 as _;
            },
            0b00011_01_000_001_010, // SUB R2,R1,R0
            [5, -10 as _, -15 as _, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            negative | carry
        );

        // ADD{S} Rd,Rs,#nn
        test_instr!(
            |cpu: &mut Cpu| cpu.reg.r[0] = 10,
            0b00011_10_101_000_000, // ADD R0,R0,#5
            [15, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        );

        // SUB{S} Rd,Rs,#nn
        test_instr!(
            |cpu: &mut Cpu| cpu.reg.r[0] = 10,
            0b00011_11_010_000_000, // SUB R0,R0,#2
            [8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            carry
        );
    }

    #[test]
    fn execute_instr_mov_cmp_add_sub_imm() {
        // MOV{S} Rd,#nn
        test_instr!(
            |cpu: &mut Cpu| cpu.reg.cpsr.negative = true,
            0b001_00_101_11111111, // MOV R5,#255
            [0, 0, 0, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        );
        test_instr!(
            |cpu: &mut Cpu| cpu.reg.r[1] = 1337,
            0b001_00_001_00000000, // MOV R1,#0
            [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            zero
        );

        // CMP{S} Rd,#nn
        test_instr!(
            |cpu: &mut Cpu| cpu.reg.r[6] = 255,
            0b001_01_110_11111111, // CMP R6,#255
            [0, 0, 0, 0, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            zero | carry
        );
        test_instr!(
            |cpu: &mut Cpu| cpu.reg.r[2] = 13,
            0b001_01_010_00000000, // CMP R2,#0
            [0, 0, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        );

        // ADD{S} Rd,#nn
        test_instr!(
            |cpu: &mut Cpu| cpu.reg.r[7] = 3,
            0b001_10_111_10101010, // ADD R7,#170
            [0, 0, 0, 0, 0, 0, 0, 173, 0, 0, 0, 0, 0, 0, 0, 0],
        );

        // SUB{S} Rd,#nn
        test_instr!(
            |cpu: &mut Cpu| cpu.reg.r[3] = 10,
            0b001_11_011_00001111, // SUB R3,#15
            [0, 0, 0, -5 as _, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            negative
        );
    }
}
