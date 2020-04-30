// MIT License
//
// Copyright (c) 2019 Alasdair Armstrong
//
// Permission is hereby granted, free of charge, to any person
// obtaining a copy of this software and associated documentation
// files (the "Software"), to deal in the Software without
// restriction, including without limitation the rights to use, copy,
// modify, merge, publish, distribute, sublicense, and/or sell copies
// of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
// NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS
// BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN
// ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
// CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! This module is a big set of primitive operations and builtins
//! which are implemented over the [crate::ir::Val] type. Most are not
//! exported directly but instead are exposed via the [Primops] struct
//! which contains all the primops. During initialization (via the
//! [crate::init] module) textual references to primops in the IR are
//! replaced with direct function pointers to their implementation in
//! this module. The [Unary], [Binary], and [Variadic] types are
//! function pointers to unary, binary, and other primops, which are
//! contained within [Primops].

#![allow(clippy::comparison_chain)]
#![allow(clippy::cognitive_complexity)]

use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::ops::{BitAnd, BitOr, Not, Shl, Shr};
use std::str::FromStr;

use crate::concrete::{bzhi_u64, BV};
use crate::error::ExecError;
use crate::executor::LocalFrame;
use crate::ir::{UVal, Val, ELF_ENTRY};
use crate::smt::smtlib::*;
use crate::smt::*;

pub type Unary<B> = fn(Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError>;
pub type Binary<B> = fn(Val<B>, Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError>;
pub type Variadic<B> = fn(Vec<Val<B>>, solver: &mut Solver<B>, frame: &mut LocalFrame<B>) -> Result<Val<B>, ExecError>;

#[allow(clippy::needless_range_loop)]
pub fn smt_i128(i: i128) -> Exp {
    let mut bitvec = [false; 128];
    for n in 0..128 {
        if (i >> n & 1) == 1 {
            bitvec[n] = true
        }
    }
    Exp::Bits(bitvec.to_vec())
}

#[allow(clippy::needless_range_loop)]
pub fn smt_i64(i: i64) -> Exp {
    let mut bitvec = [false; 64];
    for n in 0..64 {
        if (i >> n & 1) == 1 {
            bitvec[n] = true
        }
    }
    Exp::Bits(bitvec.to_vec())
}

#[allow(clippy::needless_range_loop)]
pub fn smt_u8(i: u8) -> Exp {
    let mut bitvec = [false; 8];
    for n in 0..8 {
        if (i >> n & 1) == 1 {
            bitvec[n] = true
        }
    }
    Exp::Bits(bitvec.to_vec())
}

#[allow(clippy::needless_range_loop)]
fn smt_mask_lower(len: usize, mask_width: usize) -> Exp {
    let mut bitvec = vec![false; len];
    for i in 0..mask_width {
        bitvec[i] = true
    }
    Exp::Bits(bitvec)
}

fn smt_zeros(i: i128) -> Exp {
    Exp::Bits(vec![false; i as usize])
}

fn smt_ones(i: i128) -> Exp {
    Exp::Bits(vec![true; i as usize])
}

fn smt_sbits<B: BV>(bv: B) -> Exp {
    if let Ok(u) = bv.try_into() {
        Exp::Bits64(u, bv.len())
    } else {
        panic!("smt_sbits for > 64")
    }
}

macro_rules! unary_primop_copy {
    ($f:ident, $name:expr, $unwrap:path, $wrap:path, $concrete_op:path, $smt_op:path) => {
        pub(crate) fn $f<B: BV>(x: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
            match x {
                Val::Symbolic(x) => solver.define_const($smt_op(Box::new(Exp::Var(x)))).into(),
                $unwrap(x) => Ok($wrap($concrete_op(x))),
                _ => Err(ExecError::Type($name)),
            }
        }
    };
}

macro_rules! binary_primop_copy {
    ($f:ident, $name:expr, $unwrap:path, $wrap:path, $concrete_op:path, $smt_op:path, $to_symbolic:path) => {
        pub(crate) fn $f<B: BV>(x: Val<B>, y: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
            match (x, y) {
                (Val::Symbolic(x), Val::Symbolic(y)) => {
                    solver.define_const($smt_op(Box::new(Exp::Var(x)), Box::new(Exp::Var(y)))).into()
                }
                (Val::Symbolic(x), $unwrap(y)) => {
                    solver.define_const($smt_op(Box::new(Exp::Var(x)), Box::new($to_symbolic(y)))).into()
                }
                ($unwrap(x), Val::Symbolic(y)) => {
                    solver.define_const($smt_op(Box::new($to_symbolic(x)), Box::new(Exp::Var(y)))).into()
                }
                ($unwrap(x), $unwrap(y)) => Ok($wrap($concrete_op(x, y))),
                (_, _) => Err(ExecError::Type($name)),
            }
        }
    };
}

macro_rules! binary_primop {
    ($f:ident, $name:expr, $unwrap:path, $wrap:path, $concrete_op:path, $smt_op:path, $to_symbolic:path) => {
        pub(crate) fn $f<B: BV>(x: Val<B>, y: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
            match (x, y) {
                (Val::Symbolic(x), Val::Symbolic(y)) => {
                    solver.define_const($smt_op(Box::new(Exp::Var(x)), Box::new(Exp::Var(y)))).into()
                }
                (Val::Symbolic(x), $unwrap(y)) => {
                    solver.define_const($smt_op(Box::new(Exp::Var(x)), Box::new($to_symbolic(y)))).into()
                }
                ($unwrap(x), Val::Symbolic(y)) => {
                    solver.define_const($smt_op(Box::new($to_symbolic(x)), Box::new(Exp::Var(y)))).into()
                }
                ($unwrap(x), $unwrap(y)) => Ok($wrap($concrete_op(&x, &y))),
                (_, _) => Err(ExecError::Type($name)),
            }
        }
    };
}

fn assume<B: BV>(x: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match x {
        Val::Symbolic(v) => {
            solver.add(Def::Assert(Exp::Var(v)));
            Ok(Val::Unit)
        }
        Val::Bool(b) => {
            if b {
                Ok(Val::Unit)
            } else {
                solver.add(Def::Assert(Exp::Bool(false)));
                Ok(Val::Unit)
            }
        }
        _ => Err(ExecError::Type("assert")),
    }
}

// If the assertion can succeed, it will
fn optimistic_assert<B: BV>(x: Val<B>, message: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    let message = match message {
        Val::String(message) => message,
        _ => return Err(ExecError::Type("optimistic_assert")),
    };
    match x {
        Val::Symbolic(v) => {
            let test_true = Box::new(Exp::Var(v));
            let can_be_true = solver.check_sat_with(&test_true).is_sat()?;
            if can_be_true {
                solver.add(Def::Assert(Exp::Var(v)));
                Ok(Val::Unit)
            } else {
                Err(ExecError::AssertionFailed(message))
            }
        }
        Val::Bool(b) => {
            if b {
                Ok(Val::Unit)
            } else {
                Err(ExecError::AssertionFailed(message))
            }
        }
        _ => Err(ExecError::Type("optimistic_assert")),
    }
}

// If the assertion can fail, it will
fn pessimistic_assert<B: BV>(x: Val<B>, message: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    let message = match message {
        Val::String(message) => message,
        _ => return Err(ExecError::Type("pessimistic_assert")),
    };
    match x {
        Val::Symbolic(v) => {
            let test_false = Exp::Not(Box::new(Exp::Var(v)));
            let can_be_false = solver.check_sat_with(&test_false).is_sat()?;
            if can_be_false {
                Err(ExecError::AssertionFailed(message))
            } else {
                Ok(Val::Unit)
            }
        }
        Val::Bool(b) => {
            if b {
                Ok(Val::Unit)
            } else {
                Err(ExecError::AssertionFailed(message))
            }
        }
        _ => Err(ExecError::Type("pessimistic_assert")),
    }
}

// Conversion functions

fn i64_to_i128<B: BV>(x: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match x {
        Val::I64(x) => Ok(Val::I128(i128::from(x))),
        Val::Symbolic(x) => solver.define_const(Exp::SignExtend(64, Box::new(Exp::Var(x)))).into(),
        _ => Err(ExecError::Type("%i64->%i")),
    }
}

fn i128_to_i64<B: BV>(x: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match x {
        Val::I128(x) => match i64::try_from(x) {
            Ok(y) => Ok(Val::I64(y)),
            Err(_) => Err(ExecError::Overflow),
        },
        Val::Symbolic(x) => solver.define_const(Exp::Extract(63, 0, Box::new(Exp::Var(x)))).into(),
        _ => Err(ExecError::Type("%i->%i64")),
    }
}

// FIXME: The Sail->C compilation uses xs == NULL to check if a list
// is empty, so we replicate that here for now, but we should
// introduce a separate @is_empty operator instead.
pub(crate) fn op_eq<B: BV>(x: Val<B>, y: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match (x, y) {
        (Val::List(xs), Val::List(ys)) => {
            if xs.len() != ys.len() {
                Ok(Val::Bool(false))
            } else if xs.is_empty() && ys.is_empty() {
                Ok(Val::Bool(true))
            } else {
                Err(ExecError::Type("op_eq"))
            }
        }
        (x, y) => eq_anything(x, y, solver),
    }
}

pub(crate) fn op_neq<B: BV>(x: Val<B>, y: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match (x, y) {
        (Val::List(xs), Val::List(ys)) => {
            if xs.len() != ys.len() {
                Ok(Val::Bool(true))
            } else if xs.is_empty() && ys.is_empty() {
                Ok(Val::Bool(false))
            } else {
                Err(ExecError::Type("op_neq"))
            }
        }
        (x, y) => neq_anything(x, y, solver),
    }
}

pub(crate) fn op_head<B: BV>(xs: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match xs {
        Val::List(mut xs) => match xs.pop() {
            Some(x) => Ok(x),
            None => Err(ExecError::Type("op_head")),
        },
        _ => Err(ExecError::Type("op_head")),
    }
}

pub(crate) fn op_tail<B: BV>(xs: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match xs {
        Val::List(mut xs) => {
            xs.pop();
            Ok(Val::List(xs))
        }
        _ => Err(ExecError::Type("op_tail")),
    }
}

binary_primop!(op_lt, "op_lt", Val::I64, Val::Bool, i64::lt, Exp::Bvslt, smt_i64);
binary_primop!(op_gt, "op_gt", Val::I64, Val::Bool, i64::gt, Exp::Bvsgt, smt_i64);
binary_primop!(op_lteq, "op_lteq", Val::I64, Val::Bool, i64::le, Exp::Bvsle, smt_i64);
binary_primop!(op_gteq, "op_gteq", Val::I64, Val::Bool, i64::ge, Exp::Bvsge, smt_i64);
binary_primop_copy!(op_add, "op_add", Val::I64, Val::I64, i64::wrapping_add, Exp::Bvadd, smt_i64);
binary_primop_copy!(op_sub, "op_sub", Val::I64, Val::I64, i64::wrapping_sub, Exp::Bvsub, smt_i64);

pub(crate) fn bit_to_bool<B: BV>(bit: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match bit {
        Val::Bits(bit) => Ok(Val::Bool(bit.bits() & 1 == 1)),
        Val::Symbolic(bit) => {
            solver.define_const(Exp::Eq(Box::new(Exp::Bits([true].to_vec())), Box::new(Exp::Var(bit)))).into()
        }
        _ => Err(ExecError::Type("bit_to_bool")),
    }
}

pub(crate) fn op_unsigned<B: BV>(bits: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match bits {
        Val::Bits(bits) => Ok(Val::I64(bits.unsigned() as i64)),
        Val::Symbolic(bits) => match solver.length(bits) {
            Some(length) => solver.define_const(Exp::ZeroExtend(64 - length, Box::new(Exp::Var(bits)))).into(),
            None => Err(ExecError::Type("op_unsigned")),
        },
        _ => Err(ExecError::Type("op_unsigned")),
    }
}

pub(crate) fn op_signed<B: BV>(bits: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match bits {
        Val::Bits(bits) => Ok(Val::I64(bits.signed() as i64)),
        Val::Symbolic(bits) => match solver.length(bits) {
            Some(length) => solver.define_const(Exp::SignExtend(64 - length, Box::new(Exp::Var(bits)))).into(),
            None => Err(ExecError::Type("op_unsigned")),
        },
        _ => Err(ExecError::Type("op_unsigned")),
    }
}

// Basic comparisons

unary_primop_copy!(not_bool, "not", Val::Bool, Val::Bool, bool::not, Exp::Not);
binary_primop_copy!(and_bool, "and_bool", Val::Bool, Val::Bool, bool::bitand, Exp::And, Exp::Bool);
binary_primop_copy!(or_bool, "or_bool", Val::Bool, Val::Bool, bool::bitor, Exp::Or, Exp::Bool);
binary_primop!(eq_int, "eq_int", Val::I128, Val::Bool, i128::eq, Exp::Eq, smt_i128);
binary_primop!(eq_bool, "eq_bool", Val::Bool, Val::Bool, bool::eq, Exp::Eq, Exp::Bool);
binary_primop!(lteq_int, "lteq", Val::I128, Val::Bool, i128::le, Exp::Bvsle, smt_i128);
binary_primop!(gteq_int, "gteq", Val::I128, Val::Bool, i128::ge, Exp::Bvsge, smt_i128);
binary_primop!(lt_int, "lt", Val::I128, Val::Bool, i128::lt, Exp::Bvslt, smt_i128);
binary_primop!(gt_int, "gt", Val::I128, Val::Bool, i128::gt, Exp::Bvsgt, smt_i128);

fn abs_int<B: BV>(x: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match x {
        Val::I128(x) => Ok(Val::I128(x.abs())),
        Val::Symbolic(x) => {
            let y = solver.fresh();
            solver.add(Def::DefineConst(
                y,
                Exp::Ite(
                    Box::new(Exp::Bvslt(Box::new(Exp::Var(x)), Box::new(smt_i128(0)))),
                    Box::new(Exp::Bvneg(Box::new(Exp::Var(x)))),
                    Box::new(Exp::Var(x)),
                ),
            ));
            Ok(Val::Symbolic(y))
        }
        _ => Err(ExecError::Type("abs_int")),
    }
}

// Arithmetic operations

binary_primop_copy!(sub_int, "sub_int", Val::I128, Val::I128, i128::wrapping_sub, Exp::Bvsub, smt_i128);
binary_primop_copy!(mult_int, "mult_int", Val::I128, Val::I128, i128::wrapping_mul, Exp::Bvmul, smt_i128);
unary_primop_copy!(neg_int, "neg_int", Val::I128, Val::I128, i128::wrapping_neg, Exp::Bvneg);
binary_primop_copy!(tdiv_int, "tdiv_int", Val::I128, Val::I128, i128::wrapping_div, Exp::Bvsdiv, smt_i128);
binary_primop_copy!(tmod_int, "tmod_int", Val::I128, Val::I128, i128::wrapping_rem, Exp::Bvsmod, smt_i128);
binary_primop_copy!(shl_int, "shl_int", Val::I128, Val::I128, i128::shl, Exp::Bvshl, smt_i128);
binary_primop_copy!(shr_int, "shr_int", Val::I128, Val::I128, i128::shr, Exp::Bvashr, smt_i128);
binary_primop_copy!(shl_mach_int, "shl_mach_int", Val::I64, Val::I64, i64::shl, Exp::Bvshl, smt_i64);
binary_primop_copy!(shr_mach_int, "shr_mach_int", Val::I64, Val::I64, i64::shr, Exp::Bvashr, smt_i64);
binary_primop_copy!(udiv_int, "udiv_int", Val::I128, Val::I128, i128::wrapping_div, Exp::Bvudiv, smt_i128);

pub(crate) fn add_int<B: BV>(x: Val<B>, y: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match (x, y) {
        (Val::Symbolic(x), Val::Symbolic(y)) => {
            solver.define_const(Exp::Bvadd(Box::new(Exp::Var(x)), Box::new(Exp::Var(y)))).into()
        }
        (Val::Symbolic(x), Val::I128(y)) => {
            if y != 0 {
                solver.define_const(Exp::Bvadd(Box::new(Exp::Var(x)), Box::new(smt_i128(y)))).into()
            } else {
                Ok(Val::Symbolic(x))
            }
        }
        (Val::I128(x), Val::Symbolic(y)) => {
            if x != 0 {
                solver.define_const(Exp::Bvadd(Box::new(smt_i128(x)), Box::new(Exp::Var(y)))).into()
            } else {
                Ok(Val::Symbolic(y))
            }
        }
        (Val::I128(x), Val::I128(y)) => Ok(Val::I128(i128::wrapping_add(x, y))),
        (_, _) => Err(ExecError::Type("add_int")),
    }
}

macro_rules! symbolic_compare {
    ($op: path, $x: expr, $y: expr, $solver: ident) => {{
        let z = $solver.fresh();
        $solver
            .add(Def::DefineConst(z, Exp::Ite(Box::new($op(Box::new($x), Box::new($y))), Box::new($x), Box::new($y))));
        Ok(Val::Symbolic(z))
    }};
}

fn max_int<B: BV>(x: Val<B>, y: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match (x, y) {
        (Val::I128(x), Val::I128(y)) => Ok(Val::I128(i128::max(x, y))),
        (Val::I128(x), Val::Symbolic(y)) => symbolic_compare!(Exp::Bvsgt, smt_i128(x), Exp::Var(y), solver),
        (Val::Symbolic(x), Val::I128(y)) => symbolic_compare!(Exp::Bvsgt, Exp::Var(x), smt_i128(y), solver),
        (Val::Symbolic(x), Val::Symbolic(y)) => symbolic_compare!(Exp::Bvsgt, Exp::Var(x), Exp::Var(y), solver),
        (_, _) => Err(ExecError::Type("max_int")),
    }
}

fn min_int<B: BV>(x: Val<B>, y: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match (x, y) {
        (Val::I128(x), Val::I128(y)) => Ok(Val::I128(i128::min(x, y))),
        (Val::I128(x), Val::Symbolic(y)) => symbolic_compare!(Exp::Bvslt, smt_i128(x), Exp::Var(y), solver),
        (Val::Symbolic(x), Val::I128(y)) => symbolic_compare!(Exp::Bvslt, Exp::Var(x), smt_i128(y), solver),
        (Val::Symbolic(x), Val::Symbolic(y)) => symbolic_compare!(Exp::Bvslt, Exp::Var(x), Exp::Var(y), solver),
        (_, _) => Err(ExecError::Type("max_int")),
    }
}

fn pow2<B: BV>(x: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match x {
        Val::I128(x) => Ok(Val::I128(1 << x)),
        Val::Symbolic(x) => solver.define_const(Exp::Bvshl(Box::new(smt_i128(1)), Box::new(Exp::Var(x)))).into(),
        _ => Err(ExecError::Type("pow2")),
    }
}

fn pow_int<B: BV>(x: Val<B>, y: Val<B>, _solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match (x, y) {
        (Val::I128(x), Val::I128(y)) => Ok(Val::I128(x.pow(y.try_into().map_err(|_| ExecError::Overflow)?))),
        (_, _) => Err(ExecError::Type("pow_int")),
    }
}

fn sub_nat<B: BV>(x: Val<B>, y: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match (x, y) {
        (Val::I128(x), Val::I128(y)) => Ok(Val::I128(i128::max(x - y, 0))),
        (Val::I128(x), Val::Symbolic(y)) => {
            symbolic_compare!(Exp::Bvsgt, Exp::Bvsub(Box::new(smt_i128(x)), Box::new(Exp::Var(y))), smt_i128(0), solver)
        }
        (Val::Symbolic(x), Val::I128(y)) => {
            symbolic_compare!(Exp::Bvsgt, Exp::Bvsub(Box::new(Exp::Var(x)), Box::new(smt_i128(y))), smt_i128(0), solver)
        }
        (Val::Symbolic(x), Val::Symbolic(y)) => {
            symbolic_compare!(Exp::Bvsgt, Exp::Bvsub(Box::new(Exp::Var(x)), Box::new(Exp::Var(y))), smt_i128(0), solver)
        }
        (_, _) => Err(ExecError::Type("sub_nat")),
    }
}

// Bitvector operations

fn length<B: BV>(x: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match x {
        Val::Symbolic(v) => match solver.length(v) {
            Some(len) => Ok(Val::I128(i128::from(len))),
            None => Err(ExecError::Type("length")),
        },
        Val::Bits(bv) => Ok(Val::I128(bv.len_i128())),
        _ => Err(ExecError::Type("length")),
    }
}

binary_primop!(eq_bits, "eq_bits", Val::Bits, Val::Bool, B::eq, Exp::Eq, smt_sbits);
binary_primop!(neq_bits, "neq_bits", Val::Bits, Val::Bool, B::ne, Exp::Neq, smt_sbits);
unary_primop_copy!(not_bits, "not_bits", Val::Bits, Val::Bits, B::not, Exp::Bvnot);
binary_primop_copy!(xor_bits, "xor_bits", Val::Bits, Val::Bits, B::bitxor, Exp::Bvxor, smt_sbits);
binary_primop_copy!(or_bits, "or_bits", Val::Bits, Val::Bits, B::bitor, Exp::Bvor, smt_sbits);
binary_primop_copy!(and_bits, "and_bits", Val::Bits, Val::Bits, B::bitand, Exp::Bvand, smt_sbits);
binary_primop_copy!(add_bits, "add_bits", Val::Bits, Val::Bits, B::add, Exp::Bvadd, smt_sbits);
binary_primop_copy!(sub_bits, "sub_bits", Val::Bits, Val::Bits, B::sub, Exp::Bvsub, smt_sbits);

fn add_bits_int<B: BV>(bits: Val<B>, n: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match (bits, n) {
        (Val::Bits(bits), Val::I128(n)) => {
            Ok(Val::Bits(B::new(bzhi_u64(bits.bits() + n as u64, bits.len()), bits.len())))
        }
        (Val::Symbolic(bits), Val::I128(n)) => {
            let result = solver.fresh();
            let len = match solver.length(bits) {
                Some(len) => len,
                None => return Err(ExecError::Type("add_bits_int")),
            };
            assert!(len <= 128);
            solver.add(Def::DefineConst(
                result,
                Exp::Bvadd(Box::new(Exp::Var(bits)), Box::new(Exp::Extract(len - 1, 0, Box::new(smt_i128(n))))),
            ));
            Ok(Val::Symbolic(result))
        }
        (Val::Symbolic(bits), Val::Symbolic(n)) => {
            let result = solver.fresh();
            let len = match solver.length(bits) {
                Some(len) => len,
                None => return Err(ExecError::Type("add_bits_int")),
            };
            assert!(len <= 128);
            solver.add(Def::DefineConst(
                result,
                Exp::Bvadd(Box::new(Exp::Var(bits)), Box::new(Exp::Extract(len - 1, 0, Box::new(Exp::Var(n))))),
            ));
            Ok(Val::Symbolic(result))
        }
        (_, _) => Err(ExecError::Type("add_bits_int")),
    }
}

fn sub_bits_int<B: BV>(bits: Val<B>, n: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match (bits, n) {
        (Val::Bits(bits), Val::I128(n)) => {
            Ok(Val::Bits(B::new(bzhi_u64(bits.bits() - n as u64, bits.len()), bits.len())))
        }
        (Val::Symbolic(bits), Val::I128(n)) => {
            let result = solver.fresh();
            let len = match solver.length(bits) {
                Some(len) => len,
                None => return Err(ExecError::Type("sub_bits_int")),
            };
            assert!(len <= 128);
            solver.add(Def::DefineConst(
                result,
                Exp::Bvsub(Box::new(Exp::Var(bits)), Box::new(Exp::Extract(len - 1, 0, Box::new(smt_i128(n))))),
            ));
            Ok(Val::Symbolic(result))
        }
        (Val::Symbolic(bits), Val::Symbolic(n)) => {
            let result = solver.fresh();
            let len = match solver.length(bits) {
                Some(len) => len,
                None => return Err(ExecError::Type("sub_bits_int")),
            };
            assert!(len <= 128);
            solver.add(Def::DefineConst(
                result,
                Exp::Bvsub(Box::new(Exp::Var(bits)), Box::new(Exp::Extract(len - 1, 0, Box::new(Exp::Var(n))))),
            ));
            Ok(Val::Symbolic(result))
        }
        (_, _) => Err(ExecError::Type("sub_bits_int")),
    }
}

fn zeros<B: BV>(len: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match len {
        Val::I128(len) => {
            if len <= 64 {
                Ok(Val::Bits(B::zeros(len as u32)))
            } else {
                solver.define_const(smt_zeros(len)).into()
            }
        }
        Val::Symbolic(_) => Err(ExecError::SymbolicLength("zeros")),
        _ => Err(ExecError::Type("zeros")),
    }
}

fn ones<B: BV>(len: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match len {
        Val::I128(len) => {
            if len <= 64 {
                Ok(Val::Bits(B::ones(len as u32)))
            } else {
                solver.define_const(smt_ones(len)).into()
            }
        }
        Val::Symbolic(_) => Err(ExecError::SymbolicLength("ones")),
        _ => Err(ExecError::Type("ones")),
    }
}

/// The zero_extend and sign_extend functions are essentially the
/// same, so use a macro to define both.
macro_rules! extension {
    ($id: ident, $name: expr, $smt_extension: path, $concrete_extension: path) => {
        fn $id<B: BV>(bits: Val<B>, len: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
            match (bits, len) {
                (Val::Bits(bits), Val::I128(len)) => {
                    let len = len as u32;
                    if len > 64 {
                        let ext = len - bits.len();
                        solver.define_const($smt_extension(ext, Box::new(smt_sbits(bits)))).into()
                    } else {
                        Ok(Val::Bits($concrete_extension(bits, len)))
                    }
                }
                (Val::Symbolic(bits), Val::I128(len)) => {
                    let ext = match solver.length(bits) {
                        Some(orig_len) => len as u32 - orig_len,
                        None => return Err(ExecError::Type($name)),
                    };
                    solver.define_const($smt_extension(ext, Box::new(Exp::Var(bits)))).into()
                }
                (_, Val::Symbolic(_)) => Err(ExecError::SymbolicLength("extension")),
                (_, _) => Err(ExecError::Type($name)),
            }
        }
    };
}

extension!(zero_extend, "zero_extend", Exp::ZeroExtend, B::zero_extend);
extension!(sign_extend, "sign_extend", Exp::SignExtend, B::sign_extend);

fn replicate_exp(bits: Exp, times: i128) -> Exp {
    if times == 0 {
        Exp::Bits64(0, 0)
    } else if times == 1 {
        bits
    } else {
        Exp::Concat(Box::new(bits.clone()), Box::new(replicate_exp(bits, times - 1)))
    }
}

fn replicate_bits<B: BV>(bits: Val<B>, times: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match (bits, times) {
        (Val::Bits(bits), Val::I128(times)) => match bits.replicate(times) {
            Some(replicated) => Ok(Val::Bits(replicated)),
            None => solver.define_const(replicate_exp(smt_sbits(bits), times)).into(),
        },
        (Val::Symbolic(bits), Val::I128(times)) => {
            if times == 0 {
                Ok(Val::Bits(B::zeros(0)))
            } else {
                solver.define_const(replicate_exp(Exp::Var(bits), times)).into()
            }
        }
        (_, _) => Err(ExecError::Type("replicate_bits")),
    }
}

/// Return the length of a concrete or symbolic bitvector, or return
/// [ExecError::Type] if the argument value is not a
/// bitvector.
pub fn length_bits<B: BV>(bits: &Val<B>, solver: &mut Solver<B>) -> Result<u32, ExecError> {
    match bits {
        Val::Bits(bits) => Ok(bits.len()),
        Val::Symbolic(bits) => match solver.length(*bits) {
            Some(len) => Ok(len),
            None => Err(ExecError::Type("length_bits")),
        },
        _ => Err(ExecError::Type("length_bits")),
    }
}

/// This macro implements the symbolic slice operation for anything
/// that is implemented as a bitvector in the SMT solver, so it can be
/// used for slice, get_slice_int, etc.
macro_rules! slice {
    ($bits_length: expr, $bits: expr, $from: expr, $slice_length: expr, $solver: ident) => {{
        assert!(($slice_length as u32) <= $bits_length);
        match $from {
            Val::Symbolic(from) => {
                let sliced = $solver.fresh();
                // As from is symbolic we need to use bvlshr to do a
                // left shift before extracting between length - 1 to
                // 0. We therefore need to make from the correct
                // length so the bvlshr is type-correct.
                let shift = if $bits_length > 128 {
                    Exp::ZeroExtend($bits_length - 128, Box::new(Exp::Var(from)))
                } else if $bits_length < 128 {
                    Exp::Extract($bits_length - 1, 0, Box::new(Exp::Var(from)))
                } else {
                    Exp::Var(from)
                };
                $solver.add(Def::DefineConst(
                    sliced,
                    Exp::Extract($slice_length as u32 - 1, 0, Box::new(Exp::Bvlshr(Box::new($bits), Box::new(shift)))),
                ));
                Ok(Val::Symbolic(sliced))
            }

            Val::I128(from) => {
                let sliced = $solver.fresh();
                if from == 0 && ($slice_length as u32) == $bits_length {
                    $solver.add(Def::DefineConst(sliced, $bits))
                } else {
                    $solver.add(Def::DefineConst(
                        sliced,
                        Exp::Extract((from + $slice_length - 1) as u32, from as u32, Box::new($bits)),
                    ))
                }
                Ok(Val::Symbolic(sliced))
            }

            Val::I64(from) => {
                let sliced = $solver.fresh();
                if from == 0 && ($slice_length as u32) == $bits_length {
                    $solver.add(Def::DefineConst(sliced, $bits))
                } else {
                    $solver.add(Def::DefineConst(
                        sliced,
                        Exp::Extract((from as i128 + $slice_length - 1) as u32, from as u32, Box::new($bits)),
                    ))
                }
                Ok(Val::Symbolic(sliced))
            }

            _ => Err(ExecError::Type("slice!")),
        }
    }};
}

pub(crate) fn op_slice<B: BV>(
    bits: Val<B>,
    from: Val<B>,
    length: u32,
    solver: &mut Solver<B>,
) -> Result<Val<B>, ExecError> {
    let bits_length = length_bits(&bits, solver)?;
    match bits {
        Val::Symbolic(bits) => slice!(bits_length, Exp::Var(bits), from, length as i128, solver),
        Val::Bits(bits) => match from {
            Val::I64(from) => match bits.slice(from as u32, length) {
                Some(bits) => Ok(Val::Bits(bits)),
                None => Err(ExecError::Type("op_slice")),
            },
            _ if bits.is_zero() => Ok(Val::Bits(B::zeros(bits_length))),
            _ => slice!(bits_length, smt_sbits(bits), from, length as i128, solver),
        },
        _ => Err(ExecError::Type("op_slice")),
    }
}

fn slice_internal<B: BV>(
    bits: Val<B>,
    from: Val<B>,
    length: Val<B>,
    solver: &mut Solver<B>,
) -> Result<Val<B>, ExecError> {
    let bits_length = length_bits(&bits, solver)?;
    match length {
        Val::I128(length) => match bits {
            Val::Symbolic(bits) => slice!(bits_length, Exp::Var(bits), from, length, solver),
            Val::Bits(bits) => match from {
                Val::I128(from) => match bits.slice(from as u32, length as u32) {
                    Some(bits) => Ok(Val::Bits(bits)),
                    None => Err(ExecError::Type("slice_internal")),
                },
                _ if bits.is_zero() => Ok(Val::Bits(B::zeros(bits_length))),
                _ => slice!(bits_length, smt_sbits(bits), from, length, solver),
            },
            _ => Err(ExecError::Type("slice_internal")),
        },
        Val::Symbolic(_) => Err(ExecError::SymbolicLength("slice_internal")),
        _ => Err(ExecError::Type("slice_internal")),
    }
}

fn slice<B: BV>(args: Vec<Val<B>>, solver: &mut Solver<B>, _: &mut LocalFrame<B>) -> Result<Val<B>, ExecError> {
    slice_internal(args[0].clone(), args[1].clone(), args[2].clone(), solver)
}

fn subrange_internal<B: BV>(
    bits: Val<B>,
    high: Val<B>,
    low: Val<B>,
    solver: &mut Solver<B>,
) -> Result<Val<B>, ExecError> {
    match (bits, high, low) {
        (Val::Symbolic(bits), Val::I128(high), Val::I128(low)) => {
            solver.define_const(Exp::Extract(high as u32, low as u32, Box::new(Exp::Var(bits)))).into()
        }
        (Val::Bits(bits), Val::I128(high), Val::I128(low)) => match bits.extract(high as u32, low as u32) {
            Some(bits) => Ok(Val::Bits(bits)),
            None => Err(ExecError::Type("subrange_internal")),
        },
        (_, _, Val::Symbolic(_)) => Err(ExecError::SymbolicLength("subrange_internal")),
        (_, Val::Symbolic(_), _) => Err(ExecError::SymbolicLength("subrange_internal")),
        (_, _, _) => Err(ExecError::Type("subrange_internal")),
    }
}

fn subrange<B: BV>(args: Vec<Val<B>>, solver: &mut Solver<B>, _: &mut LocalFrame<B>) -> Result<Val<B>, ExecError> {
    subrange_internal(args[0].clone(), args[1].clone(), args[2].clone(), solver)
}

fn sail_truncate<B: BV>(bits: Val<B>, len: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    slice_internal(bits, Val::I128(0), len, solver)
}

fn sail_truncate_lsb<B: BV>(bits: Val<B>, len: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match (bits, len) {
        (Val::Bits(bits), Val::I128(len)) => match bits.truncate_lsb(len) {
            Some(truncated) => Ok(Val::Bits(truncated)),
            None => Err(ExecError::Type("sail_truncateLSB")),
        },
        (Val::Symbolic(bits), Val::I128(len)) => {
            if len == 0 {
                Ok(Val::Bits(B::new(0, 0)))
            } else if let Some(orig_len) = solver.length(bits) {
                let low = orig_len - (len as u32);
                solver.define_const(Exp::Extract(orig_len - 1, low, Box::new(Exp::Var(bits)))).into()
            } else {
                Err(ExecError::Type("sail_truncateLSB"))
            }
        }
        (_, Val::Symbolic(_)) => Err(ExecError::SymbolicLength("sail_truncateLSB")),
        (_, _) => Err(ExecError::Type("sail_truncateLSB")),
    }
}

fn sail_unsigned<B: BV>(bits: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match bits {
        Val::Bits(bits) => Ok(Val::I128(bits.unsigned())),
        Val::Symbolic(bits) => match solver.length(bits) {
            Some(length) => solver.define_const(Exp::ZeroExtend(128 - length, Box::new(Exp::Var(bits)))).into(),
            None => Err(ExecError::Type("sail_unsigned")),
        },
        _ => Err(ExecError::Type("sail_unsigned")),
    }
}

fn sail_signed<B: BV>(bits: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match bits {
        Val::Bits(bits) => Ok(Val::I128(bits.signed())),
        Val::Symbolic(bits) => match solver.length(bits) {
            Some(length) => solver.define_const(Exp::SignExtend(128 - length, Box::new(Exp::Var(bits)))).into(),
            None => Err(ExecError::Type("sail_signed")),
        },
        _ => Err(ExecError::Type("sail_signed")),
    }
}

fn shiftr<B: BV>(bits: Val<B>, shift: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match (bits, shift) {
        (Val::Symbolic(x), Val::Symbolic(y)) => match solver.length(x) {
            Some(length) => {
                let shift = if length < 128 {
                    Exp::Extract(length - 1, 0, Box::new(Exp::Var(y)))
                } else if length > 128 {
                    Exp::ZeroExtend(length - 128, Box::new(Exp::Var(y)))
                } else {
                    Exp::Var(y)
                };
                solver.define_const(Exp::Bvlshr(Box::new(Exp::Var(x)), Box::new(shift))).into()
            }
            None => Err(ExecError::Type("shiftr")),
        },
        (Val::Symbolic(x), Val::I128(0)) => Ok(Val::Symbolic(x)),
        (Val::Symbolic(x), Val::I128(y)) => match solver.length(x) {
            Some(length) => {
                let shift = if length < 128 {
                    Exp::Extract(length - 1, 0, Box::new(smt_i128(y)))
                } else if length > 128 {
                    Exp::ZeroExtend(length - 128, Box::new(smt_i128(y)))
                } else {
                    smt_i128(y)
                };
                solver.define_const(Exp::Bvlshr(Box::new(Exp::Var(x)), Box::new(shift))).into()
            }
            None => Err(ExecError::Type("shiftr")),
        },
        (Val::Bits(x), Val::Symbolic(y)) => solver
            .define_const(Exp::Bvlshr(
                Box::new(smt_sbits(x)),
                Box::new(Exp::Extract(x.len() - 1, 0, Box::new(Exp::Var(y)))),
            ))
            .into(),
        (Val::Bits(x), Val::I128(y)) => Ok(Val::Bits(x.shiftr(y))),
        (_, _) => Err(ExecError::Type("shiftr")),
    }
}

fn shiftl<B: BV>(bits: Val<B>, len: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match (bits, len) {
        (Val::Symbolic(x), Val::Symbolic(y)) => match solver.length(x) {
            Some(length) => {
                let shift = if length < 128 {
                    Exp::Extract(length - 1, 0, Box::new(Exp::Var(y)))
                } else if length > 128 {
                    Exp::ZeroExtend(length - 128, Box::new(Exp::Var(y)))
                } else {
                    Exp::Var(y)
                };
                solver.define_const(Exp::Bvshl(Box::new(Exp::Var(x)), Box::new(shift))).into()
            }
            None => Err(ExecError::Type("shiftl")),
        },
        (Val::Symbolic(x), Val::I128(0)) => Ok(Val::Symbolic(x)),
        (Val::Symbolic(x), Val::I128(y)) => match solver.length(x) {
            Some(length) => {
                let shift = if length < 128 {
                    Exp::Extract(length - 1, 0, Box::new(smt_i128(y)))
                } else if length > 128 {
                    Exp::ZeroExtend(length - 128, Box::new(smt_i128(y)))
                } else {
                    smt_i128(y)
                };
                solver.define_const(Exp::Bvshl(Box::new(Exp::Var(x)), Box::new(shift))).into()
            }
            None => Err(ExecError::Type("shiftl")),
        },
        (Val::Bits(x), Val::Symbolic(y)) => solver
            .define_const(Exp::Bvshl(
                Box::new(smt_sbits(x)),
                Box::new(Exp::Extract(x.len() - 1, 0, Box::new(Exp::Var(y)))),
            ))
            .into(),
        (Val::Bits(x), Val::I128(y)) => Ok(Val::Bits(x.shiftl(y))),
        (_, _) => Err(ExecError::Type("shiftl")),
    }
}

fn shift_bits_right<B: BV>(bits: Val<B>, shift: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    let bits_len = length_bits(&bits, solver)?;
    let shift_len = length_bits(&bits, solver)?;
    match (&bits, &shift) {
        (Val::Symbolic(_), Val::Symbolic(_)) | (Val::Bits(_), Val::Symbolic(_)) | (Val::Symbolic(_), Val::Bits(_)) => {
            let shift = if bits_len < shift_len {
                Exp::Extract(bits_len - 1, 0, Box::new(smt_value(&shift)?))
            } else if bits_len > shift_len {
                Exp::ZeroExtend(bits_len - shift_len, Box::new(smt_value(&shift)?))
            } else {
                smt_value(&shift)?
            };
            solver.define_const(Exp::Bvlshr(Box::new(smt_value(&bits)?), Box::new(shift))).into()
        }
        (Val::Bits(x), Val::Bits(y)) => Ok(Val::Bits(x.shiftr(y.bits() as i128))),
        (_, _) => Err(ExecError::Type("shift_bits_right")),
    }
}

fn shift_bits_left<B: BV>(bits: Val<B>, shift: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    let bits_len = length_bits(&bits, solver)?;
    let shift_len = length_bits(&bits, solver)?;
    match (&bits, &shift) {
        (Val::Symbolic(_), Val::Symbolic(_)) | (Val::Bits(_), Val::Symbolic(_)) | (Val::Symbolic(_), Val::Bits(_)) => {
            let shift = if bits_len < shift_len {
                Exp::Extract(bits_len - 1, 0, Box::new(smt_value(&shift)?))
            } else if bits_len > shift_len {
                Exp::ZeroExtend(bits_len - shift_len, Box::new(smt_value(&shift)?))
            } else {
                smt_value(&shift)?
            };
            solver.define_const(Exp::Bvshl(Box::new(smt_value(&bits)?), Box::new(shift))).into()
        }
        (Val::Bits(x), Val::Bits(y)) => Ok(Val::Bits(x.shiftl(y.bits() as i128))),
        (_, _) => Err(ExecError::Type("shift_bits_left")),
    }
}

pub(crate) fn append<B: BV>(lhs: Val<B>, rhs: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match (lhs, rhs) {
        (Val::Symbolic(x), Val::Symbolic(y)) => {
            solver.define_const(Exp::Concat(Box::new(Exp::Var(x)), Box::new(Exp::Var(y)))).into()
        }
        (Val::Symbolic(x), Val::Bits(y)) => {
            if y.len() == 0 {
                solver.define_const(Exp::Var(x)).into()
            } else {
                solver.define_const(Exp::Concat(Box::new(Exp::Var(x)), Box::new(smt_sbits(y)))).into()
            }
        }
        (Val::Bits(x), Val::Symbolic(y)) => {
            if x.len() == 0 {
                solver.define_const(Exp::Var(y)).into()
            } else {
                solver.define_const(Exp::Concat(Box::new(smt_sbits(x)), Box::new(Exp::Var(y)))).into()
            }
        }
        (Val::Bits(x), Val::Bits(y)) => match x.append(y) {
            Some(z) => Ok(Val::Bits(z)),
            None => solver.define_const(Exp::Concat(Box::new(smt_sbits(x)), Box::new(smt_sbits(y)))).into(),
        },
        (_, _) => Err(ExecError::Type("append")),
    }
}

pub(crate) fn vector_access<B: BV>(vec: Val<B>, n: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match (vec, n) {
        (Val::Symbolic(bits), Val::Symbolic(n)) => match solver.length(bits) {
            Some(length) => {
                let shift = if length < 128 {
                    Exp::Extract(length - 1, 0, Box::new(Exp::Var(n)))
                } else if length > 128 {
                    Exp::ZeroExtend(length - 128, Box::new(Exp::Var(n)))
                } else {
                    Exp::Var(n)
                };
                solver
                    .define_const(Exp::Extract(0, 0, Box::new(Exp::Bvlshr(Box::new(Exp::Var(bits)), Box::new(shift)))))
                    .into()
            }
            None => Err(ExecError::Type("vector_access")),
        },
        (Val::Symbolic(bits), Val::I128(n)) => match solver.length(bits) {
            Some(length) => {
                let shift = if length < 128 {
                    Exp::Extract(length - 1, 0, Box::new(smt_i128(n)))
                } else if length > 128 {
                    Exp::ZeroExtend(length - 128, Box::new(smt_i128(n)))
                } else {
                    smt_i128(n)
                };
                solver
                    .define_const(Exp::Extract(0, 0, Box::new(Exp::Bvlshr(Box::new(Exp::Var(bits)), Box::new(shift)))))
                    .into()
            }
            None => Err(ExecError::Type("vector_access")),
        },
        (Val::Bits(bits), Val::Symbolic(n)) => {
            let shift = Exp::Extract(bits.len() - 1, 0, Box::new(Exp::Var(n)));
            solver
                .define_const(Exp::Extract(0, 0, Box::new(Exp::Bvlshr(Box::new(smt_sbits(bits)), Box::new(shift)))))
                .into()
        }
        (Val::Bits(bits), Val::I128(n)) => match bits.slice(n as u32, 1) {
            Some(bit) => Ok(Val::Bits(bit)),
            None => Err(ExecError::Type("vector_access")),
        },
        (Val::Vector(vec), Val::I128(n)) => match vec.get(n as usize) {
            Some(elem) => Ok(elem.clone()),
            None => Err(ExecError::OutOfBounds("vector_access")),
        },
        (_, _) => Err(ExecError::Type("vector_access")),
    }
}

/// The set_slice! macro implements the Sail set_slice builtin for any
/// combination of symbolic or concrete operands, with the result
/// always being symbolic. The argument order is the same as the Sail
/// function it implements, plus the solver as a final argument.
macro_rules! set_slice {
    ($bits_length: expr, $update_length: ident, $bits: expr, $n: expr, $update: expr, $solver: ident) => {{
        let mask_lower = smt_mask_lower($bits_length as usize, $update_length as usize);
        let update = if $bits_length == $update_length {
            $update
        } else {
            Exp::ZeroExtend($bits_length - $update_length, Box::new($update))
        };
        let shift = if $bits_length < 128 {
            Exp::Extract($bits_length - 1, 0, Box::new($n))
        } else if $bits_length > 128 {
            Exp::ZeroExtend($bits_length - 128, Box::new($n))
        } else {
            $n
        };
        let sliced = $solver.fresh();
        $solver.add(Def::DefineConst(
            sliced,
            Exp::Bvor(
                Box::new(Exp::Bvand(
                    Box::new($bits),
                    Box::new(Exp::Bvnot(Box::new(Exp::Bvshl(Box::new(mask_lower), Box::new(shift.clone()))))),
                )),
                Box::new(Exp::Bvshl(Box::new(update), Box::new(shift))),
            ),
        ));
        Ok(Val::Symbolic(sliced))
    }};
}

/// A special case of set_slice! for when $n == 0, and therefore no shift needs to be applied.
macro_rules! set_slice_n0 {
    ($bits_length: expr, $update_length: ident, $bits: expr, $update: expr, $solver: ident) => {{
        let mask_lower = smt_mask_lower($bits_length as usize, $update_length as usize);
        let update = if $bits_length == $update_length {
            $update
        } else {
            Exp::ZeroExtend($bits_length - $update_length, Box::new($update))
        };
        let sliced = $solver.fresh();
        $solver.add(Def::DefineConst(
            sliced,
            Exp::Bvor(
                Box::new(Exp::Bvand(Box::new($bits), Box::new(Exp::Bvnot(Box::new(mask_lower))))),
                Box::new(update),
            ),
        ));
        Ok(Val::Symbolic(sliced))
    }};
}

fn set_slice_internal<B: BV>(
    bits: Val<B>,
    n: Val<B>,
    update: Val<B>,
    solver: &mut Solver<B>,
) -> Result<Val<B>, ExecError> {
    let bits_length = length_bits(&bits, solver)?;
    let update_length = length_bits(&update, solver)?;
    match (bits, n, update) {
        (Val::Symbolic(bits), Val::Symbolic(n), Val::Symbolic(update)) => {
            set_slice!(bits_length, update_length, Exp::Var(bits), Exp::Var(n), Exp::Var(update), solver)
        }
        (Val::Symbolic(bits), Val::Symbolic(n), Val::Bits(update)) => {
            set_slice!(bits_length, update_length, Exp::Var(bits), Exp::Var(n), smt_sbits(update), solver)
        }
        (Val::Symbolic(bits), Val::I128(n), Val::Symbolic(update)) => {
            if n == 0 {
                set_slice_n0!(bits_length, update_length, Exp::Var(bits), Exp::Var(update), solver)
            } else {
                set_slice!(bits_length, update_length, Exp::Var(bits), smt_i128(n), Exp::Var(update), solver)
            }
        }
        (Val::Symbolic(bits), Val::I128(n), Val::Bits(update)) => {
            if n == 0 {
                set_slice_n0!(bits_length, update_length, Exp::Var(bits), smt_sbits(update), solver)
            } else {
                set_slice!(bits_length, update_length, Exp::Var(bits), smt_i128(n), smt_sbits(update), solver)
            }
        }
        (Val::Bits(bits), Val::Symbolic(n), Val::Symbolic(update)) => {
            set_slice!(bits_length, update_length, smt_sbits(bits), Exp::Var(n), Exp::Var(update), solver)
        }
        (Val::Bits(bits), Val::Symbolic(n), Val::Bits(update)) => {
            set_slice!(bits_length, update_length, smt_sbits(bits), Exp::Var(n), smt_sbits(update), solver)
        }
        (Val::Bits(bits), Val::I128(n), Val::Symbolic(update)) => {
            if n == 0 {
                set_slice_n0!(bits_length, update_length, smt_sbits(bits), Exp::Var(update), solver)
            } else {
                set_slice!(bits_length, update_length, smt_sbits(bits), smt_i128(n), Exp::Var(update), solver)
            }
        }
        (Val::Bits(bits), Val::I128(n), Val::Bits(update)) => Ok(Val::Bits(bits.set_slice(n as u32, update))),
        (_, _, _) => Err(ExecError::Type("set_slice")),
    }
}

fn set_slice<B: BV>(args: Vec<Val<B>>, solver: &mut Solver<B>, _: &mut LocalFrame<B>) -> Result<Val<B>, ExecError> {
    // set_slice Sail builtin takes 2 additional integer parameters
    // for the bitvector lengths, which we can ignore.
    set_slice_internal(args[2].clone(), args[3].clone(), args[4].clone(), solver)
}

fn set_slice_int_internal<B: BV>(
    int: Val<B>,
    n: Val<B>,
    update: Val<B>,
    solver: &mut Solver<B>,
) -> Result<Val<B>, ExecError> {
    let update_length = length_bits(&update, solver)?;
    match (int, n, update) {
        (Val::Symbolic(int), Val::Symbolic(n), Val::Symbolic(update)) => {
            set_slice!(128, update_length, Exp::Var(int), Exp::Var(n), Exp::Var(update), solver)
        }
        (Val::Symbolic(int), Val::Symbolic(n), Val::Bits(update)) => {
            set_slice!(128, update_length, Exp::Var(int), Exp::Var(n), smt_sbits(update), solver)
        }
        (Val::Symbolic(int), Val::I128(n), Val::Symbolic(update)) => {
            if n == 0 {
                set_slice_n0!(128, update_length, Exp::Var(int), Exp::Var(update), solver)
            } else {
                set_slice!(128, update_length, Exp::Var(int), smt_i128(n), Exp::Var(update), solver)
            }
        }
        (Val::Symbolic(int), Val::I128(n), Val::Bits(update)) => {
            if n == 0 {
                set_slice_n0!(128, update_length, Exp::Var(int), smt_sbits(update), solver)
            } else {
                set_slice!(128, update_length, Exp::Var(int), smt_i128(n), smt_sbits(update), solver)
            }
        }
        (Val::I128(int), Val::Symbolic(n), Val::Symbolic(update)) => {
            set_slice!(128, update_length, smt_i128(int), Exp::Var(n), Exp::Var(update), solver)
        }
        (Val::I128(int), Val::Symbolic(n), Val::Bits(update)) => {
            set_slice!(128, update_length, smt_i128(int), Exp::Var(n), smt_sbits(update), solver)
        }
        (Val::I128(int), Val::I128(n), Val::Symbolic(update)) => {
            if n == 0 {
                set_slice_n0!(128, update_length, smt_i128(int), Exp::Var(update), solver)
            } else {
                set_slice!(128, update_length, smt_i128(int), smt_i128(n), Exp::Var(update), solver)
            }
        }
        (Val::I128(int), Val::I128(n), Val::Bits(update)) => Ok(Val::I128(B::set_slice_int(int, n as u32, update))),
        (_, _, _) => Err(ExecError::Type("set_slice_int")),
    }
}

fn set_slice_int<B: BV>(args: Vec<Val<B>>, solver: &mut Solver<B>, _: &mut LocalFrame<B>) -> Result<Val<B>, ExecError> {
    // set_slice_int Sail builtin takes 1 additional integer parameter for the bitvector length,
    // which we can ignore.
    set_slice_int_internal(args[1].clone(), args[2].clone(), args[3].clone(), solver)
}

/// op_set_slice is just set_slice_internal with 64-bit integers rather than 128-bit.
pub(crate) fn op_set_slice<B: BV>(
    bits: Val<B>,
    n: Val<B>,
    update: Val<B>,
    solver: &mut Solver<B>,
) -> Result<Val<B>, ExecError> {
    let bits_length = length_bits(&bits, solver)?;
    let update_length = length_bits(&update, solver)?;
    match (bits, n, update) {
        (Val::Symbolic(bits), Val::Symbolic(n), Val::Symbolic(update)) => {
            set_slice!(bits_length, update_length, Exp::Var(bits), Exp::Var(n), Exp::Var(update), solver)
        }
        (Val::Symbolic(bits), Val::Symbolic(n), Val::Bits(update)) => {
            set_slice!(bits_length, update_length, Exp::Var(bits), Exp::Var(n), smt_sbits(update), solver)
        }
        (Val::Symbolic(bits), Val::I64(n), Val::Symbolic(update)) => {
            if n == 0 {
                set_slice_n0!(bits_length, update_length, Exp::Var(bits), Exp::Var(update), solver)
            } else {
                set_slice!(bits_length, update_length, Exp::Var(bits), smt_i64(n), Exp::Var(update), solver)
            }
        }
        (Val::Symbolic(bits), Val::I64(n), Val::Bits(update)) => {
            if n == 0 {
                set_slice_n0!(bits_length, update_length, Exp::Var(bits), smt_sbits(update), solver)
            } else {
                set_slice!(bits_length, update_length, Exp::Var(bits), smt_i64(n), smt_sbits(update), solver)
            }
        }
        (Val::Bits(bits), Val::Symbolic(n), Val::Symbolic(update)) => {
            set_slice!(bits_length, update_length, smt_sbits(bits), Exp::Var(n), Exp::Var(update), solver)
        }
        (Val::Bits(bits), Val::Symbolic(n), Val::Bits(update)) => {
            set_slice!(bits_length, update_length, smt_sbits(bits), Exp::Var(n), smt_sbits(update), solver)
        }
        (Val::Bits(bits), Val::I64(n), Val::Symbolic(update)) => {
            if n == 0 {
                set_slice_n0!(bits_length, update_length, smt_sbits(bits), Exp::Var(update), solver)
            } else {
                set_slice!(bits_length, update_length, smt_sbits(bits), smt_i64(n), Exp::Var(update), solver)
            }
        }
        (Val::Bits(bits), Val::I64(n), Val::Bits(update)) => Ok(Val::Bits(bits.set_slice(n as u32, update))),
        (_, _, _) => Err(ExecError::Type("set_slice")),
    }
}

/// `vector_update` is a special case of `set_slice` where the update
/// is a bitvector of length 1
fn vector_update<B: BV>(args: Vec<Val<B>>, solver: &mut Solver<B>, _: &mut LocalFrame<B>) -> Result<Val<B>, ExecError> {
    let arg0 = args[0].clone();
    match arg0 {
        Val::Vector(mut vec) => match args[1] {
            Val::I128(n) => {
                vec[n as usize] = args[2].clone();
                Ok(Val::Vector(vec))
            }
            Val::Symbolic(n) => {
                for (i, item) in vec.iter_mut().enumerate() {
                    let var = solver.fresh();
                    solver.add(Def::DefineConst(
                        var,
                        Exp::Ite(
                            Box::new(Exp::Eq(Box::new(Exp::Var(n)), Box::new(Exp::Bits64(i as u64, 128)))),
                            Box::new(smt_value(&args[2])?),
                            Box::new(smt_value(&item)?),
                        ),
                    ));
                    *item = Val::Symbolic(var);
                }
                Ok(Val::Vector(vec))
            }
            _ => {
                eprintln!("{:?}", args);
                Err(ExecError::Type("vector_update (index)"))
            }
        },
        Val::Bits(_) => {
            // If the argument is a bitvector then `vector_update` is a special case of `set_slice`
            // where the update is a bitvector of length 1
            set_slice_internal(arg0, args[1].clone(), args[2].clone(), solver)
        }
        Val::Symbolic(v) if solver.is_bitvector(v) => {
            set_slice_internal(arg0, args[1].clone(), args[2].clone(), solver)
        }
        _ => Err(ExecError::Type("vector_update")),
    }
}

fn vector_update_subrange<B: BV>(
    args: Vec<Val<B>>,
    solver: &mut Solver<B>,
    _: &mut LocalFrame<B>,
) -> Result<Val<B>, ExecError> {
    set_slice_internal(args[0].clone(), args[2].clone(), args[3].clone(), solver)
}

fn undefined_vector<B: BV>(len: Val<B>, elem: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    if let Val::I128(len) = len {
        if let Ok(len) = usize::try_from(len) {
            Ok(Val::Vector(vec![elem; len]))
        } else {
            Err(ExecError::Overflow)
        }
    } else {
        Err(ExecError::SymbolicLength("undefined_vector"))
    }
}

fn bitvector_update<B: BV>(
    args: Vec<Val<B>>,
    solver: &mut Solver<B>,
    _: &mut LocalFrame<B>,
) -> Result<Val<B>, ExecError> {
    op_set_slice(args[0].clone(), args[1].clone(), args[2].clone(), solver)
}

fn get_slice_int_internal<B: BV>(
    length: Val<B>,
    n: Val<B>,
    from: Val<B>,
    solver: &mut Solver<B>,
) -> Result<Val<B>, ExecError> {
    match length {
        Val::I128(length) => match n {
            Val::Symbolic(n) => slice!(128, Exp::Var(n), from, length, solver),
            Val::I128(n) => match from {
                Val::I128(from) if length <= 64 => {
                    let slice = bzhi_u64((n >> from) as u64, length as u32);
                    Ok(Val::Bits(B::new(slice, length as u32)))
                }
                _ => slice!(128, smt_i128(n), from, length, solver),
            },
            _ => Err(ExecError::Type("get_slice_int")),
        },
        Val::Symbolic(_) => Err(ExecError::SymbolicLength("get_slice_int")),
        _ => Err(ExecError::Type("get_slice_int")),
    }
}

fn get_slice_int<B: BV>(args: Vec<Val<B>>, solver: &mut Solver<B>, _: &mut LocalFrame<B>) -> Result<Val<B>, ExecError> {
    get_slice_int_internal(args[0].clone(), args[1].clone(), args[2].clone(), solver)
}

fn unimplemented<B: BV>(_: Vec<Val<B>>, _: &mut Solver<B>, _: &mut LocalFrame<B>) -> Result<Val<B>, ExecError> {
    Err(ExecError::Unimplemented)
}

fn eq_string<B: BV>(lhs: Val<B>, rhs: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match (lhs, rhs) {
        (Val::String(lhs), Val::String(rhs)) => Ok(Val::Bool(lhs == rhs)),
        (_, _) => Err(ExecError::Type("eq_string")),
    }
}

fn concat_str<B: BV>(lhs: Val<B>, rhs: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match (lhs, rhs) {
        (Val::String(lhs), Val::String(rhs)) => Ok(Val::String(format!("{}{}", lhs, rhs))),
        (_, _) => Err(ExecError::Type("concat_str")),
    }
}

fn hex_str<B: BV>(n: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match n {
        Val::I128(n) => Ok(Val::String(format!("0x{:x}", n))),
        Val::Symbolic(v) => Ok(Val::String(format!("0x[{}]", v))),
        _ => Err(ExecError::Type("hex_str")),
    }
}

fn dec_str<B: BV>(n: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match n {
        Val::I128(n) => Ok(Val::String(format!("{}", n))),
        Val::Symbolic(v) => Ok(Val::String(format!("[{}]", v))),
        _ => Err(ExecError::Type("dec_str")),
    }
}

// Strings can never be symbolic
fn undefined_string<B: BV>(_: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    Ok(Val::Poison)
}

fn string_to_i128<B: BV>(s: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    if let Val::String(s) = s {
        if let Ok(n) = i128::from_str(&s) {
            Ok(Val::I128(n))
        } else {
            Err(ExecError::Overflow)
        }
    } else {
        Err(ExecError::Type("%string->%int"))
    }
}

fn eq_anything<B: BV>(lhs: Val<B>, rhs: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match (lhs, rhs) {
        (Val::Symbolic(lhs), Val::Symbolic(rhs)) => {
            solver.define_const(Exp::Eq(Box::new(Exp::Var(lhs)), Box::new(Exp::Var(rhs)))).into()
        }
        (Val::Bits(lhs), Val::Symbolic(rhs)) => {
            solver.define_const(Exp::Eq(Box::new(smt_sbits(lhs)), Box::new(Exp::Var(rhs)))).into()
        }
        (Val::Symbolic(lhs), Val::Bits(rhs)) => {
            solver.define_const(Exp::Eq(Box::new(Exp::Var(lhs)), Box::new(smt_sbits(rhs)))).into()
        }
        (Val::Bits(lhs), Val::Bits(rhs)) => Ok(Val::Bool(lhs == rhs)),

        (Val::Symbolic(lhs), Val::Enum(rhs)) => {
            solver.define_const(Exp::Eq(Box::new(Exp::Var(lhs)), Box::new(Exp::Enum(rhs)))).into()
        }
        (Val::Enum(lhs), Val::Symbolic(rhs)) => {
            solver.define_const(Exp::Eq(Box::new(Exp::Enum(lhs)), Box::new(Exp::Var(rhs)))).into()
        }
        (Val::Enum(lhs), Val::Enum(rhs)) => Ok(Val::Bool(lhs == rhs)),

        (_, _) => Err(ExecError::Type("eq_anything")),
    }
}

fn neq_anything<B: BV>(lhs: Val<B>, rhs: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match (lhs, rhs) {
        (Val::Symbolic(lhs), Val::Symbolic(rhs)) => {
            solver.define_const(Exp::Neq(Box::new(Exp::Var(lhs)), Box::new(Exp::Var(rhs)))).into()
        }
        (Val::Bits(lhs), Val::Symbolic(rhs)) => {
            solver.define_const(Exp::Neq(Box::new(smt_sbits(lhs)), Box::new(Exp::Var(rhs)))).into()
        }
        (Val::Symbolic(lhs), Val::Bits(rhs)) => {
            solver.define_const(Exp::Neq(Box::new(Exp::Var(lhs)), Box::new(smt_sbits(rhs)))).into()
        }
        (Val::Bits(lhs), Val::Bits(rhs)) => Ok(Val::Bool(lhs != rhs)),

        (Val::Symbolic(lhs), Val::Enum(rhs)) => {
            solver.define_const(Exp::Neq(Box::new(Exp::Var(lhs)), Box::new(Exp::Enum(rhs)))).into()
        }
        (Val::Enum(lhs), Val::Symbolic(rhs)) => {
            solver.define_const(Exp::Neq(Box::new(Exp::Enum(lhs)), Box::new(Exp::Var(rhs)))).into()
        }
        (Val::Enum(lhs), Val::Enum(rhs)) => Ok(Val::Bool(lhs != rhs)),

        (_, _) => Err(ExecError::Type("neq_anything")),
    }
}

fn string_startswith<B: BV>(s: Val<B>, prefix: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match (s, prefix) {
        (Val::String(s), Val::String(prefix)) => Ok(Val::Bool(s.starts_with(&prefix))),
        _ => Err(ExecError::Type("string_startswith")),
    }
}

fn string_length<B: BV>(s: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    if let Val::String(s) = s {
        Ok(Val::I128(s.len() as i128))
    } else {
        Err(ExecError::Type("string_length"))
    }
}

fn string_drop<B: BV>(s: Val<B>, n: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match (s, n) {
        (Val::String(s), Val::I128(n)) => Ok(Val::String(s.get((n as usize)..).unwrap_or("").to_string())),
        _ => Err(ExecError::Type("string_drop")),
    }
}

fn string_take<B: BV>(s: Val<B>, n: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match (s, n) {
        (Val::String(s), Val::I128(n)) => Ok(Val::String(s.get(..(n as usize)).unwrap_or(&s).to_string())),
        _ => Err(ExecError::Type("string_take")),
    }
}

fn string_of_bits<B: BV>(bv: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match bv {
        Val::Bits(bv) => Ok(Val::String(format!("{}", bv))),
        Val::Symbolic(v) => Ok(Val::String(format!("v{}", v))),
        _ => Err(ExecError::Type("string_of_bits")),
    }
}

fn decimal_string_of_bits<B: BV>(bv: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match bv {
        Val::Bits(bv) => Ok(Val::String(format!("{}", bv.signed()))),
        Val::Symbolic(v) => Ok(Val::String(format!("v{}", v))),
        _ => Err(ExecError::Type("decimal_string_of_bits")),
    }
}

fn string_of_int<B: BV>(n: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match n {
        Val::I128(n) => Ok(Val::String(format!("{}", n))),
        Val::Symbolic(v) => Ok(Val::String(format!("v{}", v))),
        _ => Err(ExecError::Type("string_of_int")),
    }
}

fn putchar<B: BV>(_c: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    //if let Val::I128(c) = c {
    //    eprintln!("Stdout: {}", char::from(c as u8))
    //}
    Ok(Val::Unit)
}

fn print<B: BV>(_message: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    //if let Val::String(message) = message {
    //    eprintln!("Stdout: {}", message)
    //}
    Ok(Val::Unit)
}

fn prerr<B: BV>(_message: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    //if let Val::String(message) = message {
    //    eprintln!("Stderr: {}", message)
    //}
    Ok(Val::Unit)
}

fn print_string<B: BV>(_prefix: Val<B>, _message: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    Ok(Val::Unit)
}

fn prerr_string<B: BV>(_prefix: Val<B>, _message: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    Ok(Val::Unit)
}

fn print_int<B: BV>(_prefix: Val<B>, _n: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    Ok(Val::Unit)
}

fn prerr_int<B: BV>(_prefix: Val<B>, _n: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    Ok(Val::Unit)
}

fn print_endline<B: BV>(_message: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    Ok(Val::Unit)
}

fn prerr_endline<B: BV>(_message: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    Ok(Val::Unit)
}

fn print_bits<B: BV>(_message: Val<B>, _bits: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    //if let Val::String(message) = message {
    //    eprintln!("Stdout: {}{:?}", message, bits)
    //}
    Ok(Val::Unit)
}

fn prerr_bits<B: BV>(_message: Val<B>, _bits: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    //if let Val::String(message) = message {
    //    eprintln!("Stderr: {}{:?}", message, bits)
    //}
    Ok(Val::Unit)
}

fn undefined_bitvector<B: BV>(sz: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    if let Val::I128(sz) = sz {
        solver.declare_const(Ty::BitVec(sz as u32)).into()
    } else {
        Err(ExecError::Type("undefined_bitvector"))
    }
}

fn undefined_bool<B: BV>(_: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    solver.declare_const(Ty::Bool).into()
}

fn undefined_int<B: BV>(_: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    solver.declare_const(Ty::BitVec(128)).into()
}

fn undefined_nat<B: BV>(_: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    let sym = solver.fresh();
    solver.add(Def::DeclareConst(sym, Ty::BitVec(128)));
    solver.add(Def::Assert(Exp::Bvsge(Box::new(Exp::Var(sym)), Box::new(smt_i128(0)))));
    Ok(Val::Symbolic(sym))
}

fn undefined_range<B: BV>(lo: Val<B>, hi: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    let sym = solver.fresh();
    solver.add(Def::DeclareConst(sym, Ty::BitVec(128)));
    solver.add(Def::Assert(Exp::Bvsle(Box::new(smt_value(&lo)?), Box::new(Exp::Var(sym)))));
    solver.add(Def::Assert(Exp::Bvsle(Box::new(Exp::Var(sym)), Box::new(smt_value(&hi)?))));
    Ok(Val::Symbolic(sym))
}

fn undefined_unit<B: BV>(_: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    Ok(Val::Unit)
}

fn one_if<B: BV>(condition: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match condition {
        Val::Bool(true) => Ok(Val::Bits(B::BIT_ONE)),
        Val::Bool(false) => Ok(Val::Bits(B::BIT_ZERO)),
        Val::Symbolic(v) => solver
            .define_const(Exp::Ite(
                Box::new(Exp::Var(v)),
                Box::new(smt_sbits(B::BIT_ONE)),
                Box::new(smt_sbits(B::BIT_ZERO)),
            ))
            .into(),
        _ => Err(ExecError::Type("one_if")),
    }
}

fn zero_if<B: BV>(condition: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match condition {
        Val::Bool(true) => Ok(Val::Bits(B::BIT_ZERO)),
        Val::Bool(false) => Ok(Val::Bits(B::BIT_ONE)),
        Val::Symbolic(v) => solver
            .define_const(Exp::Ite(
                Box::new(Exp::Var(v)),
                Box::new(smt_sbits(B::BIT_ZERO)),
                Box::new(smt_sbits(B::BIT_ONE)),
            ))
            .into(),
        _ => Err(ExecError::Type("one_if")),
    }
}

fn cons<B: BV>(x: Val<B>, xs: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match xs {
        /* TODO: Make this not a hack */
        Val::Poison => Ok(Val::List(vec![x])),
        Val::List(mut xs) => {
            xs.push(x);
            Ok(Val::List(xs))
        }
        _ => Err(ExecError::Type("cons")),
    }
}

/// Convert base values into SMT equivalents.
pub fn smt_value<B: BV>(v: &Val<B>) -> Result<Exp, ExecError> {
    Ok(match v {
        Val::I128(n) => smt_i128(*n),
        Val::I64(n) => smt_i64(*n),
        Val::Bits(bv) => smt_sbits(*bv),
        Val::Bool(b) => Exp::Bool(*b),
        Val::Enum(e) => Exp::Enum(*e),
        Val::Symbolic(v) => Exp::Var(*v),
        _ => return Err(ExecError::Type("smt_value")),
    })
}

fn choice_chain<B: BV>(sym: Sym, n: u64, sz: u32, mut xs: Vec<Val<B>>) -> Result<Exp, ExecError> {
    if xs.len() == 1 {
        smt_value(&xs[0])
    } else {
        let x = xs.pop().unwrap();
        Ok(Exp::Ite(
            Box::new(Exp::Eq(Box::new(Exp::Var(sym)), Box::new(Exp::Bits64(n, sz)))),
            Box::new(smt_value(&x)?),
            Box::new(choice_chain(sym, n + 1, sz, xs)?),
        ))
    }
}

fn choice<B: BV>(xs: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match xs {
        Val::List(xs) => {
            // We need to choose an element between 0 and n - 1 where
            // n is the list length, this choice is represented as a
            // bitvector that is just long enough to represent the
            // numbers 0 to n.
            let sz = ((xs.len() + 1) as f64).log2().ceil() as u32;
            let sym = solver.fresh();
            let choice = solver.fresh();
            solver.add(Def::DeclareConst(sym, Ty::BitVec(sz)));
            solver.add(Def::DefineConst(choice, choice_chain(sym, 0, sz, xs)?));
            Ok(Val::Symbolic(choice))
        }
        _ => Err(ExecError::Type("cons")),
    }
}

fn read_mem<B: BV>(args: Vec<Val<B>>, solver: &mut Solver<B>, frame: &mut LocalFrame<B>) -> Result<Val<B>, ExecError> {
    frame.memory().read(args[0].clone(), args[2].clone(), args[3].clone(), solver)
}

fn bad_read<B: BV>(_: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    Err(ExecError::BadRead)
}

fn write_mem<B: BV>(args: Vec<Val<B>>, solver: &mut Solver<B>, frame: &mut LocalFrame<B>) -> Result<Val<B>, ExecError> {
    frame.memory_mut().write(args[0].clone(), args[2].clone(), args[4].clone(), solver)
}

fn bad_write<B: BV>(_: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    Err(ExecError::BadWrite)
}

fn write_mem_ea<B: BV>(
    _: Vec<Val<B>>,
    _solver: &mut Solver<B>,
    _frame: &mut LocalFrame<B>,
) -> Result<Val<B>, ExecError> {
    Ok(Val::Unit)
}

fn cycle_count<B: BV>(_: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    solver.cycle_count();
    Ok(Val::Unit)
}

fn get_cycle_count<B: BV>(_: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    Ok(Val::I128(solver.get_cycle_count()))
}

fn get_verbosity<B: BV>(_: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    Ok(Val::Bits(B::zeros(64)))
}

fn sleeping<B: BV>(_: Val<B>, _solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    // let sym = solver.fresh();
    // solver.add(Def::DeclareConst(sym, Ty::Bool));
    // solver.add_event(Event::Sleeping(sym));
    // Ok(Val::Symbolic(sym))
    Ok(Val::Bool(false))
}

fn wakeup_request<B: BV>(_: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    solver.add_event(Event::WakeupRequest);
    Ok(Val::Unit)
}

fn sleep_request<B: BV>(_: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    solver.add_event(Event::WakeupRequest);
    Ok(Val::Unit)
}

fn instr_announce<B: BV>(opcode: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    solver.add_event(Event::Instr(opcode));
    Ok(Val::Unit)
}

fn branch_announce<B: BV>(_: Val<B>, target: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    solver.add_event(Event::Branch { address: target });
    Ok(Val::Unit)
}

fn barrier<B: BV>(barrier_kind: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    solver.add_event(Event::Barrier { barrier_kind });
    Ok(Val::Unit)
}

fn cache_maintenance<B: BV>(
    args: Vec<Val<B>>,
    solver: &mut Solver<B>,
    _: &mut LocalFrame<B>,
) -> Result<Val<B>, ExecError> {
    solver.add_event(Event::CacheOp { cache_op_kind: args[0].clone(), address: args[2].clone() });
    Ok(Val::Unit)
}

fn elf_entry<B: BV>(_: Vec<Val<B>>, _: &mut Solver<B>, frame: &mut LocalFrame<B>) -> Result<Val<B>, ExecError> {
    match frame.lets().get(&ELF_ENTRY) {
        Some(UVal::Init(value)) => Ok(value.clone()),
        _ => Err(ExecError::NoElfEntry),
    }
}

fn monomorphize<B: BV>(val: Val<B>, _: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    Ok(val)
}

fn mark_register<B: BV>(val: Val<B>, mark: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    match (val, mark) {
        (Val::Ref(reg), Val::String(mark)) => {
            solver.add_event(Event::MarkReg { reg, mark });
            Ok(Val::Unit)
        }
        _ => Err(ExecError::Type("mark_register")),
    }
}

fn align_bits<B: BV>(bv: Val<B>, alignment: Val<B>, solver: &mut Solver<B>) -> Result<Val<B>, ExecError> {
    let bv_len = length_bits(&bv, solver)?;
    match (bv, alignment) {
        // Fast path for small bitvectors with power of two alignments
        (Val::Symbolic(bv), Val::I128(alignment)) if (bv_len <= 64) & ((alignment & (alignment - 1)) == 0) => {
            let mask = !B::new((alignment as u64) - 1, bv_len);
            solver.define_const(Exp::Bvand(Box::new(Exp::Var(bv)), Box::new(smt_sbits(mask)))).into()
        }
        (bv, alignment) => {
            let x = sail_unsigned(bv, solver)?;
            let aligned_x = mult_int(alignment.clone(), udiv_int(x, alignment, solver)?, solver)?;
            get_slice_int_internal(Val::I128(bv_len as i128), aligned_x, Val::I128(0), solver)
        }
    }
}

fn ite<B: BV>(args: Vec<Val<B>>, solver: &mut Solver<B>, _: &mut LocalFrame<B>) -> Result<Val<B>, ExecError> {
    match args[0] {
        Val::Symbolic(b) => solver
            .define_const(Exp::Ite(
                Box::new(Exp::Var(b)),
                Box::new(smt_value(&args[1])?),
                Box::new(smt_value(&args[2])?),
            ))
            .into(),
        Val::Bool(true) => Ok(args[1].clone()),
        Val::Bool(false) => Ok(args[2].clone()),
        _ => Err(ExecError::Type("ite")),
    }
}

pub fn unary_primops<B: BV>() -> HashMap<String, Unary<B>> {
    let mut primops = HashMap::new();
    primops.insert("%i64->%i".to_string(), i64_to_i128 as Unary<B>);
    primops.insert("%i->%i64".to_string(), i128_to_i64 as Unary<B>);
    primops.insert("%string->%i".to_string(), string_to_i128 as Unary<B>);
    primops.insert("bit_to_bool".to_string(), bit_to_bool as Unary<B>);
    primops.insert("assume".to_string(), assume as Unary<B>);
    primops.insert("not".to_string(), not_bool as Unary<B>);
    primops.insert("neg_int".to_string(), neg_int as Unary<B>);
    primops.insert("abs_int".to_string(), abs_int as Unary<B>);
    primops.insert("pow2".to_string(), pow2 as Unary<B>);
    primops.insert("not_bits".to_string(), not_bits as Unary<B>);
    primops.insert("length".to_string(), length as Unary<B>);
    primops.insert("zeros".to_string(), zeros as Unary<B>);
    primops.insert("ones".to_string(), ones as Unary<B>);
    primops.insert("sail_unsigned".to_string(), sail_unsigned as Unary<B>);
    primops.insert("sail_signed".to_string(), sail_signed as Unary<B>);
    primops.insert("sail_putchar".to_string(), putchar as Unary<B>);
    primops.insert("print".to_string(), print as Unary<B>);
    primops.insert("prerr".to_string(), prerr as Unary<B>);
    primops.insert("print_endline".to_string(), print_endline as Unary<B>);
    primops.insert("prerr_endline".to_string(), prerr_endline as Unary<B>);
    primops.insert("undefined_bitvector".to_string(), undefined_bitvector as Unary<B>);
    primops.insert("undefined_bool".to_string(), undefined_bool as Unary<B>);
    primops.insert("undefined_int".to_string(), undefined_int as Unary<B>);
    primops.insert("undefined_nat".to_string(), undefined_nat as Unary<B>);
    primops.insert("undefined_unit".to_string(), undefined_unit as Unary<B>);
    primops.insert("undefined_string".to_string(), undefined_string as Unary<B>);
    primops.insert("one_if".to_string(), one_if as Unary<B>);
    primops.insert("zero_if".to_string(), zero_if as Unary<B>);
    primops.insert("internal_pick".to_string(), choice as Unary<B>);
    primops.insert("bad_read".to_string(), bad_read as Unary<B>);
    primops.insert("bad_write".to_string(), bad_write as Unary<B>);
    primops.insert("hex_str".to_string(), hex_str as Unary<B>);
    primops.insert("dec_str".to_string(), dec_str as Unary<B>);
    primops.insert("string_length".to_string(), string_length as Unary<B>);
    primops.insert("string_of_bits".to_string(), string_of_bits as Unary<B>);
    primops.insert("decimal_string_of_bits".to_string(), decimal_string_of_bits as Unary<B>);
    primops.insert("string_of_int".to_string(), string_of_int as Unary<B>);
    primops.insert("cycle_count".to_string(), cycle_count as Unary<B>);
    primops.insert("get_cycle_count".to_string(), get_cycle_count as Unary<B>);
    primops.insert("sail_get_verbosity".to_string(), get_verbosity as Unary<B>);
    primops.insert("sleeping".to_string(), sleeping as Unary<B>);
    primops.insert("sleep_request".to_string(), sleep_request as Unary<B>);
    primops.insert("wakeup_request".to_string(), wakeup_request as Unary<B>);
    primops.insert("platform_instr_announce".to_string(), instr_announce as Unary<B>);
    primops.insert("platform_barrier".to_string(), barrier as Unary<B>);
    primops.insert("monomorphize".to_string(), monomorphize as Unary<B>);
    primops
}

pub fn binary_primops<B: BV>() -> HashMap<String, Binary<B>> {
    let mut primops = HashMap::new();
    primops.insert("optimistic_assert".to_string(), optimistic_assert as Binary<B>);
    primops.insert("pessimistic_assert".to_string(), pessimistic_assert as Binary<B>);
    primops.insert("and_bool".to_string(), and_bool as Binary<B>);
    primops.insert("strict_and_bool".to_string(), and_bool as Binary<B>);
    primops.insert("or_bool".to_string(), or_bool as Binary<B>);
    primops.insert("strict_or_bool".to_string(), or_bool as Binary<B>);
    primops.insert("eq_int".to_string(), eq_int as Binary<B>);
    primops.insert("eq_bool".to_string(), eq_bool as Binary<B>);
    primops.insert("lteq".to_string(), lteq_int as Binary<B>);
    primops.insert("gteq".to_string(), gteq_int as Binary<B>);
    primops.insert("lt".to_string(), lt_int as Binary<B>);
    primops.insert("gt".to_string(), gt_int as Binary<B>);
    primops.insert("add_int".to_string(), add_int as Binary<B>);
    primops.insert("sub_int".to_string(), sub_int as Binary<B>);
    primops.insert("sub_nat".to_string(), sub_nat as Binary<B>);
    primops.insert("mult_int".to_string(), mult_int as Binary<B>);
    primops.insert("tdiv_int".to_string(), tdiv_int as Binary<B>);
    primops.insert("tmod_int".to_string(), tmod_int as Binary<B>);
    // FIXME: use correct euclidian operations
    primops.insert("ediv_int".to_string(), tdiv_int as Binary<B>);
    primops.insert("emod_int".to_string(), tmod_int as Binary<B>);
    primops.insert("pow_int".to_string(), pow_int as Binary<B>);
    primops.insert("shl_int".to_string(), shl_int as Binary<B>);
    primops.insert("shr_int".to_string(), shr_int as Binary<B>);
    primops.insert("shl_mach_int".to_string(), shl_mach_int as Binary<B>);
    primops.insert("shr_mach_int".to_string(), shr_mach_int as Binary<B>);
    primops.insert("max_int".to_string(), max_int as Binary<B>);
    primops.insert("min_int".to_string(), min_int as Binary<B>);
    primops.insert("eq_bit".to_string(), eq_bits as Binary<B>);
    primops.insert("eq_bits".to_string(), eq_bits as Binary<B>);
    primops.insert("neq_bits".to_string(), neq_bits as Binary<B>);
    primops.insert("xor_bits".to_string(), xor_bits as Binary<B>);
    primops.insert("or_bits".to_string(), or_bits as Binary<B>);
    primops.insert("and_bits".to_string(), and_bits as Binary<B>);
    primops.insert("add_bits".to_string(), add_bits as Binary<B>);
    primops.insert("sub_bits".to_string(), sub_bits as Binary<B>);
    primops.insert("add_bits_int".to_string(), add_bits_int as Binary<B>);
    primops.insert("sub_bits_int".to_string(), sub_bits_int as Binary<B>);
    primops.insert("align_bits".to_string(), align_bits as Binary<B>);
    primops.insert("undefined_range".to_string(), undefined_range as Binary<B>);
    primops.insert("zero_extend".to_string(), zero_extend as Binary<B>);
    primops.insert("sign_extend".to_string(), sign_extend as Binary<B>);
    primops.insert("sail_truncate".to_string(), sail_truncate as Binary<B>);
    primops.insert("sail_truncateLSB".to_string(), sail_truncate_lsb as Binary<B>);
    primops.insert("replicate_bits".to_string(), replicate_bits as Binary<B>);
    primops.insert("shiftr".to_string(), shiftr as Binary<B>);
    primops.insert("shiftl".to_string(), shiftl as Binary<B>);
    primops.insert("shift_bits_right".to_string(), shift_bits_right as Binary<B>);
    primops.insert("shift_bits_left".to_string(), shift_bits_left as Binary<B>);
    primops.insert("append".to_string(), append as Binary<B>);
    primops.insert("append_64".to_string(), append as Binary<B>);
    primops.insert("vector_access".to_string(), vector_access as Binary<B>);
    primops.insert("eq_anything".to_string(), eq_anything as Binary<B>);
    primops.insert("eq_string".to_string(), eq_string as Binary<B>);
    primops.insert("concat_str".to_string(), concat_str as Binary<B>);
    primops.insert("string_startswith".to_string(), string_startswith as Binary<B>);
    primops.insert("string_drop".to_string(), string_drop as Binary<B>);
    primops.insert("string_take".to_string(), string_take as Binary<B>);
    primops.insert("cons".to_string(), cons as Binary<B>);
    primops.insert("undefined_vector".to_string(), undefined_vector as Binary<B>);
    primops.insert("print_string".to_string(), print_string as Binary<B>);
    primops.insert("prerr_string".to_string(), prerr_string as Binary<B>);
    primops.insert("print_int".to_string(), print_int as Binary<B>);
    primops.insert("prerr_int".to_string(), prerr_int as Binary<B>);
    primops.insert("print_bits".to_string(), print_bits as Binary<B>);
    primops.insert("prerr_bits".to_string(), prerr_bits as Binary<B>);
    primops.insert("platform_branch_announce".to_string(), branch_announce as Binary<B>);
    primops.insert("mark_register".to_string(), mark_register as Binary<B>);
    primops
}

pub fn variadic_primops<B: BV>() -> HashMap<String, Variadic<B>> {
    let mut primops = HashMap::new();
    primops.insert("slice".to_string(), slice as Variadic<B>);
    primops.insert("vector_subrange".to_string(), subrange as Variadic<B>);
    primops.insert("vector_update".to_string(), vector_update as Variadic<B>);
    primops.insert("vector_update_subrange".to_string(), vector_update_subrange as Variadic<B>);
    primops.insert("bitvector_update".to_string(), bitvector_update as Variadic<B>);
    primops.insert("set_slice".to_string(), set_slice as Variadic<B>);
    primops.insert("get_slice_int".to_string(), get_slice_int as Variadic<B>);
    primops.insert("set_slice_int".to_string(), set_slice_int as Variadic<B>);
    primops.insert("platform_read_mem".to_string(), read_mem as Variadic<B>);
    primops.insert("platform_write_mem".to_string(), write_mem as Variadic<B>);
    primops.insert("platform_write_mem_ea".to_string(), write_mem_ea as Variadic<B>);
    primops.insert("platform_cache_maintenance".to_string(), cache_maintenance as Variadic<B>);
    primops.insert("elf_entry".to_string(), elf_entry as Variadic<B>);
    primops.insert("ite".to_string(), ite as Variadic<B>);
    // We explicitly don't handle anything real number related right now
    primops.insert("%string->%real".to_string(), unimplemented as Variadic<B>);
    primops.insert("neg_real".to_string(), unimplemented as Variadic<B>);
    primops.insert("mult_real".to_string(), unimplemented as Variadic<B>);
    primops.insert("sub_real".to_string(), unimplemented as Variadic<B>);
    primops.insert("add_real".to_string(), unimplemented as Variadic<B>);
    primops.insert("div_real".to_string(), unimplemented as Variadic<B>);
    primops.insert("sqrt_real".to_string(), unimplemented as Variadic<B>);
    primops.insert("abs_real".to_string(), unimplemented as Variadic<B>);
    primops.insert("round_down".to_string(), unimplemented as Variadic<B>);
    primops.insert("round_up".to_string(), unimplemented as Variadic<B>);
    primops.insert("to_real".to_string(), unimplemented as Variadic<B>);
    primops.insert("eq_real".to_string(), unimplemented as Variadic<B>);
    primops.insert("lt_real".to_string(), unimplemented as Variadic<B>);
    primops.insert("gt_real".to_string(), unimplemented as Variadic<B>);
    primops.insert("lteq_real".to_string(), unimplemented as Variadic<B>);
    primops.insert("gteq_real".to_string(), unimplemented as Variadic<B>);
    primops.insert("real_power".to_string(), unimplemented as Variadic<B>);
    primops.insert("print_real".to_string(), unimplemented as Variadic<B>);
    primops.insert("prerr_real".to_string(), unimplemented as Variadic<B>);
    primops.insert("undefined_real".to_string(), unimplemented as Variadic<B>);
    primops
}

pub struct Primops<B> {
    pub unary: HashMap<String, Unary<B>>,
    pub binary: HashMap<String, Binary<B>>,
    pub variadic: HashMap<String, Variadic<B>>,
}

impl<B: BV> Default for Primops<B> {
    fn default() -> Self {
        Primops { unary: unary_primops(), binary: binary_primops(), variadic: variadic_primops() }
    }
}
