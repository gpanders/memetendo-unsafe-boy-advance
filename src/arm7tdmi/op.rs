use crate::bus::DataBus;

use super::{reg::NamedGeneralRegister::Pc, Cpu, OperationState};

#[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
fn execute_add_impl(cpu: &mut Cpu, update_cond: bool, a: u32, b: u32, c: u32) -> u32 {
    let (a_b, a_b_overflow) = (a as i32).overflowing_add(b as _);
    let (result, a_b_c_overflow) = a_b.overflowing_add(c as _);

    if update_cond {
        let actual_result = i64::from(a) + i64::from(b) + i64::from(c);
        cpu.reg.cpsr.overflow = a_b_overflow || a_b_c_overflow;
        cpu.reg.cpsr.carry = actual_result as u64 > u32::MAX.into();
        cpu.reg.cpsr.set_nz_from(result as _);
    }

    result as _
}

#[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
fn execute_sub_impl(cpu: &mut Cpu, update_cond: bool, a: u32, b: u32, c: u32) -> u32 {
    // using overflowing_neg(), check if b == i32::MIN. if it is, we'll overflow negating it here
    // (-i32::MIN == i32::MIN in 2s complement!), so make sure the overflow flag is set after.
    // c is our implementation detail, so an overflow in c is our fault and isn't handled here.
    let (b_neg, overflow) = (b as i32).overflowing_neg();
    let result = execute_add_impl(cpu, update_cond, a, b_neg as _, -(c as i32) as _);
    cpu.reg.cpsr.overflow |= update_cond && overflow;

    result
}

impl Cpu {
    pub(super) fn execute_add_cmn(&mut self, update_cond: bool, a: u32, b: u32) -> u32 {
        execute_add_impl(self, update_cond, a, b, 0)
    }

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
    pub(super) fn execute_sub_cmp(&mut self, update_cond: bool, a: u32, b: u32) -> u32 {
        execute_sub_impl(self, update_cond, a, b, 0)
    }

    pub(super) fn execute_adc(&mut self, update_cond: bool, a: u32, b: u32) -> u32 {
        execute_add_impl(self, update_cond, a, b, self.reg.cpsr.carry.into())
    }

    pub(super) fn execute_sbc(&mut self, update_cond: bool, a: u32, b: u32) -> u32 {
        execute_sub_impl(self, update_cond, a, b, (!self.reg.cpsr.carry).into())
    }

    pub(super) fn execute_mul(&mut self, a: u32, b: u32) -> u32 {
        let result = a.wrapping_mul(b);
        self.reg.cpsr.set_nz_from(result); // TODO: MUL corrupts carry flag (lol), but how?

        result
    }

    pub(super) fn execute_mov(&mut self, update_cond: bool, value: u32) -> u32 {
        if update_cond {
            self.reg.cpsr.set_nz_from(value);
        }

        value
    }

    pub(super) fn execute_and_tst(&mut self, a: u32, b: u32) -> u32 {
        let result = a & b;
        self.reg.cpsr.set_nz_from(result);

        result
    }

    pub(super) fn execute_bic(&mut self, a: u32, b: u32) -> u32 {
        let result = a & !b;
        self.reg.cpsr.set_nz_from(result);

        result
    }

    pub(super) fn execute_eor(&mut self, a: u32, b: u32) -> u32 {
        let result = a ^ b;
        self.reg.cpsr.set_nz_from(result);

        result
    }

    pub(super) fn execute_orr(&mut self, a: u32, b: u32) -> u32 {
        let result = a | b;
        self.reg.cpsr.set_nz_from(result);

        result
    }

    pub(super) fn execute_mvn(&mut self, value: u32) -> u32 {
        let result = !value;
        self.reg.cpsr.set_nz_from(result);

        result
    }

    pub(super) fn execute_lsl(&mut self, value: u32, offset: u8) -> u32 {
        let mut result = value;
        if offset > 0 {
            result = result.checked_shl((offset - 1).into()).unwrap_or(0);
            self.reg.cpsr.carry = result & (1 << 31) != 0;
            result <<= 1;
        }
        self.reg.cpsr.set_nz_from(result);

        result
    }

    pub(super) fn execute_lsr(&mut self, value: u32, offset: u8) -> u32 {
        // LSR/ASR #0 is a special case that works like LSR/ASR #32
        let offset = if offset == 0 { 32 } else { offset.into() };

        let mut result = value;
        result = result.checked_shr(offset - 1).unwrap_or(0);
        self.reg.cpsr.carry = result & 1 != 0;
        result >>= 1;
        self.reg.cpsr.set_nz_from(result);

        result
    }

    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    pub(super) fn execute_asr(&mut self, value: u32, offset: u8) -> u32 {
        // LSR/ASR #0 is a special case that works like LSR/ASR #32
        let offset = if offset == 0 { 32 } else { offset.into() };

        // a value shifted 32 or more times is either 0 or has all bits set depending on the
        // initial value of the sign bit (due to sign extension)
        let mut result = value as i32;
        let overflow_result = if result.is_negative() {
            u32::MAX as _
        } else {
            0
        };

        result = result.checked_shr(offset - 1).unwrap_or(overflow_result);
        self.reg.cpsr.carry = result & 1 != 0;
        let result = (result >> 1) as _;
        self.reg.cpsr.set_nz_from(result);

        result
    }

    pub(super) fn execute_ror(&mut self, value: u32, offset: u8) -> u32 {
        let mut result = value;
        if offset > 0 {
            result = value.rotate_right(u32::from(offset) - 1);
            self.reg.cpsr.carry = result & 1 != 0;
            result = result.rotate_right(1);
        }
        self.reg.cpsr.set_nz_from(result);

        result
    }

    pub(super) fn execute_bx(&mut self, bus: &impl DataBus, pc: u32) {
        self.reg.cpsr.state = if pc & 1 == 0 {
            self.reg.r[Pc] = pc; // already half-word aligned (bit 0 unset)
            OperationState::Thumb
        } else {
            self.reg.r[Pc] = pc & !0b11;
            OperationState::Arm
        };

        self.reload_pipeline(bus);
    }
}
