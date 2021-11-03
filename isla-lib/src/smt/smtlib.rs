// BSD 2-Clause License
//
// Copyright (c) 2019, 2020 Alasdair Armstrong
// Copyright (c) 2020 Brian Campbell
//
// All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are
// met:
//
// 1. Redistributions of source code must retain the above copyright
// notice, this list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright
// notice, this list of conditions and the following disclaimer in the
// documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
// "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
// LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
// A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
// HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
// LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
// DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
// THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
// (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
// OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

//! This module defines a subset of the SMTLIB format we use to
//! interact with the SMT solver, which mostly corresponds to the
//! theory of quantifier-free bitvectors and arrays.

use std::collections::HashMap;
use std::ops::{Add, BitAnd, BitOr, BitXor, Shl, Shr, Sub};

use super::Sym;
use crate::bitvector::b64::B64;
use crate::bitvector::BV;
use crate::ir::EnumMember;

#[derive(Clone, Debug)]
pub enum Ty {
    Bool,
    BitVec(u32),
    Enum(usize),
    Array(Box<Ty>, Box<Ty>),
}

#[derive(Clone, Debug)]
pub enum Exp<V> {
    Var(V),
    Bits(Vec<bool>),
    Bits64(B64),
    Enum(EnumMember),
    Bool(bool),
    Eq(Box<Exp<V>>, Box<Exp<V>>),
    Neq(Box<Exp<V>>, Box<Exp<V>>),
    And(Box<Exp<V>>, Box<Exp<V>>),
    Or(Box<Exp<V>>, Box<Exp<V>>),
    Not(Box<Exp<V>>),
    Bvnot(Box<Exp<V>>),
    Bvand(Box<Exp<V>>, Box<Exp<V>>),
    Bvor(Box<Exp<V>>, Box<Exp<V>>),
    Bvxor(Box<Exp<V>>, Box<Exp<V>>),
    Bvnand(Box<Exp<V>>, Box<Exp<V>>),
    Bvnor(Box<Exp<V>>, Box<Exp<V>>),
    Bvxnor(Box<Exp<V>>, Box<Exp<V>>),
    Bvneg(Box<Exp<V>>),
    Bvadd(Box<Exp<V>>, Box<Exp<V>>),
    Bvsub(Box<Exp<V>>, Box<Exp<V>>),
    Bvmul(Box<Exp<V>>, Box<Exp<V>>),
    Bvudiv(Box<Exp<V>>, Box<Exp<V>>),
    Bvsdiv(Box<Exp<V>>, Box<Exp<V>>),
    Bvurem(Box<Exp<V>>, Box<Exp<V>>),
    Bvsrem(Box<Exp<V>>, Box<Exp<V>>),
    Bvsmod(Box<Exp<V>>, Box<Exp<V>>),
    Bvult(Box<Exp<V>>, Box<Exp<V>>),
    Bvslt(Box<Exp<V>>, Box<Exp<V>>),
    Bvule(Box<Exp<V>>, Box<Exp<V>>),
    Bvsle(Box<Exp<V>>, Box<Exp<V>>),
    Bvuge(Box<Exp<V>>, Box<Exp<V>>),
    Bvsge(Box<Exp<V>>, Box<Exp<V>>),
    Bvugt(Box<Exp<V>>, Box<Exp<V>>),
    Bvsgt(Box<Exp<V>>, Box<Exp<V>>),
    Extract(u32, u32, Box<Exp<V>>),
    ZeroExtend(u32, Box<Exp<V>>),
    SignExtend(u32, Box<Exp<V>>),
    Bvshl(Box<Exp<V>>, Box<Exp<V>>),
    Bvlshr(Box<Exp<V>>, Box<Exp<V>>),
    Bvashr(Box<Exp<V>>, Box<Exp<V>>),
    Concat(Box<Exp<V>>, Box<Exp<V>>),
    Ite(Box<Exp<V>>, Box<Exp<V>>, Box<Exp<V>>),
    App(Sym, Vec<Exp<V>>),
    Select(Box<Exp<V>>, Box<Exp<V>>),
    Store(Box<Exp<V>>, Box<Exp<V>>, Box<Exp<V>>),
    Distinct(Vec<Exp<V>>),
}

#[allow(clippy::needless_range_loop)]
pub fn bits64<V>(bits: u64, size: u32) -> Exp<V> {
    if size <= 64 {
        Exp::Bits64(B64::new(bits, size))
    } else {
        let size = size as usize;
        let mut bitvec = vec![false; size];
        for n in 0..size {
            if n < 64 && (bits >> n & 1) == 1 {
                bitvec[n] = true
            }
        }
        Exp::Bits(bitvec)
    }
}

pub fn bits_from_str<V>(s: &str) -> Option<Exp<V>> {
    if s.starts_with("0x") {
        let hex = &s[2..];
        let size = 4 * hex.len();
        if size <= 64 {
            Some(bits64(u64::from_str_radix(hex, 16).ok()?, size as u32))
        } else {
            let mut value = vec![false; size];
            let mut i = size - 4;
            for c in hex.chars() {
                let mut digit = c.to_digit(16)?;
                for j in 0..4 {
                    value[i + j] = digit & 1 == 1;
                    digit >>= 1;
                }
                i -= 4;
            }
            Some(Exp::Bits(value))
        }
    } else if s.starts_with("0b") {
        let bin = &s[2..];
        if bin.len() <= 64 {
            Some(bits64(u64::from_str_radix(bin, 2).ok()?, bin.len() as u32))
        } else {
            let size = bin.len();
            let mut value = vec![false; size];
            for (i, c) in bin.char_indices() {
                match c {
                    '0' => (),
                    '1' => value[size - i - 1] = true,
                    _ => return None,
                }
            }
            Some(Exp::Bits(value))
        }
    } else {
        None
    }
}

fn is_bits64<V>(exp: &Exp<V>) -> bool {
    matches!(exp, Exp::Bits64(_))
}

fn is_bits<V>(exp: &Exp<V>) -> bool {
    matches!(exp, Exp::Bits(_))
}

fn extract_bits64<V>(exp: &Exp<V>) -> B64 {
    match *exp {
        Exp::Bits64(bv) => bv,
        _ => unreachable!(),
    }
}

fn extract_bits<V>(exp: Exp<V>) -> Vec<bool> {
    match exp {
        Exp::Bits(bv) => bv,
        _ => unreachable!(),
    }
}

macro_rules! binary_eval {
    ($eval:path, $exp_op:path, $small_op:path, $lhs:ident, $rhs:ident) => {{
        *$lhs = $lhs.eval();
        *$rhs = $rhs.eval();
        if is_bits64(&$lhs) & is_bits64(&$rhs) {
            Exp::Bits64($small_op(extract_bits64(&$lhs), extract_bits64(&$rhs)))
        } else {
            $exp_op($lhs, $rhs)
        }
    }};
}

fn eval_extract<V>(hi: u32, lo: u32, exp: Box<Exp<V>>) -> Exp<V> {
    if is_bits64(&exp) {
        Exp::Bits64(extract_bits64(&exp).extract(hi, lo).unwrap())
    } else if is_bits(&exp) {
        let orig_vec = extract_bits(*exp);
        let len = (hi - lo) + 1;
        if len <= 64 {
            let mut bv = B64::zeros(len);
            for n in 0..len {
                if orig_vec[(n + lo) as usize] {
                    bv = bv.set_slice(n, B64::ones(1))
                }
            }
            Exp::Bits64(bv)
        } else {
            let mut vec = vec![false; len as usize];
            for n in 0..len {
                if orig_vec[(n + lo) as usize] {
                    vec[n as usize] = true
                }
            }
            Exp::Bits(vec)
        }
    } else {
        Exp::Extract(hi, lo, exp)
    }
}

fn eval_zero_extend<V>(len: u32, exp: Box<Exp<V>>) -> Exp<V> {
    if is_bits64(&exp) {
        let bv = extract_bits64(&exp);
        if bv.len() + len <= 64 {
            Exp::Bits64(bv.zero_extend(bv.len() + len))
        } else {
            Exp::ZeroExtend(len, exp)
        }
    } else {
        Exp::ZeroExtend(len, exp)
    }
}

fn eval_sign_extend<V>(len: u32, exp: Box<Exp<V>>) -> Exp<V> {
    if is_bits64(&exp) {
        let bv = extract_bits64(&exp);
        if bv.len() + len <= 64 {
            Exp::Bits64(bv.sign_extend(bv.len() + len))
        } else {
            Exp::SignExtend(len, exp)
        }
    } else {
        Exp::SignExtend(len, exp)
    }
}

impl<V> Exp<V> {
    pub fn eval(self) -> Self {
        use Exp::*;
        match self {
            Bvnot(mut exp) => {
                *exp = exp.eval();
                match *exp {
                    Bits64(bv) => Bits64(!bv),
                    Bits(mut vec) => {
                        vec.iter_mut().for_each(|b| *b = !*b);
                        Bits(vec)
                    }
                    _ => Bvnot(exp),
                }
            }
            Eq(mut lhs, mut rhs) => {
                *lhs = lhs.eval();
                *rhs = rhs.eval();
                Eq(lhs, rhs)
            }
            Bvand(mut lhs, mut rhs) => binary_eval!(Exp::eval, Bvand, B64::bitand, lhs, rhs),
            Bvor(mut lhs, mut rhs) => binary_eval!(Exp::eval, Bvor, B64::bitor, lhs, rhs),
            Bvxor(mut lhs, mut rhs) => binary_eval!(Exp::eval, Bvxor, B64::bitxor, lhs, rhs),
            Bvadd(mut lhs, mut rhs) => binary_eval!(Exp::eval, Bvadd, B64::add, lhs, rhs),
            Bvsub(mut lhs, mut rhs) => binary_eval!(Exp::eval, Bvsub, B64::sub, lhs, rhs),
            Bvlshr(mut lhs, mut rhs) => binary_eval!(Exp::eval, Bvlshr, B64::shr, lhs, rhs),
            Bvshl(mut lhs, mut rhs) => binary_eval!(Exp::eval, Bvshl, B64::shl, lhs, rhs),
            Extract(hi, lo, mut exp) => {
                *exp = exp.eval();
                eval_extract(hi, lo, exp)
            }
            ZeroExtend(len, mut exp) => {
                *exp = exp.eval();
                eval_zero_extend(len, exp)
            }
            SignExtend(len, mut exp) => {
                *exp = exp.eval();
                eval_sign_extend(len, exp)
            }
            _ => self,
        }
    }

    /// Recursivly apply the supplied function to each sub-expression in a bottom-up order
    pub fn modify<F>(&mut self, f: &F)
    where
        F: Fn(&mut Exp<V>),
    {
        use Exp::*;
        match self {
            Var(_) | Bits(_) | Bits64(_) | Enum { .. } | Bool(_) => (),
            Not(exp) | Bvnot(exp) | Bvneg(exp) | Extract(_, _, exp) | ZeroExtend(_, exp) | SignExtend(_, exp) => {
                exp.modify(f)
            }
            Eq(lhs, rhs)
            | Neq(lhs, rhs)
            | And(lhs, rhs)
            | Or(lhs, rhs)
            | Bvand(lhs, rhs)
            | Bvor(lhs, rhs)
            | Bvxor(lhs, rhs)
            | Bvnand(lhs, rhs)
            | Bvnor(lhs, rhs)
            | Bvxnor(lhs, rhs)
            | Bvadd(lhs, rhs)
            | Bvsub(lhs, rhs)
            | Bvmul(lhs, rhs)
            | Bvudiv(lhs, rhs)
            | Bvsdiv(lhs, rhs)
            | Bvurem(lhs, rhs)
            | Bvsrem(lhs, rhs)
            | Bvsmod(lhs, rhs)
            | Bvult(lhs, rhs)
            | Bvslt(lhs, rhs)
            | Bvule(lhs, rhs)
            | Bvsle(lhs, rhs)
            | Bvuge(lhs, rhs)
            | Bvsge(lhs, rhs)
            | Bvugt(lhs, rhs)
            | Bvsgt(lhs, rhs)
            | Bvshl(lhs, rhs)
            | Bvlshr(lhs, rhs)
            | Bvashr(lhs, rhs)
            | Concat(lhs, rhs) => {
                lhs.modify(f);
                rhs.modify(f);
            }
            Ite(cond, then_exp, else_exp) => {
                cond.modify(f);
                then_exp.modify(f);
                else_exp.modify(f)
            }
            App(_, args) => {
                for exp in args {
                    exp.modify(f)
                }
            }
            Select(array, index) => {
                array.modify(f);
                index.modify(f);
            }
            Store(array, index, val) => {
                array.modify(f);
                index.modify(f);
                val.modify(f);
            }
            Distinct(exps) => {
                for exp in exps {
                    exp.modify(f)
                }
            }
        };
        f(self)
    }

    /// Recursivly apply the supplied function to each sub-expression in a top down order
    pub fn modify_top_down<F>(&mut self, f: &F)
    where
        F: Fn(&mut Exp<V>),
    {
        use Exp::*;
        f(self);
        match self {
            Var(_) | Bits(_) | Bits64(_) | Enum { .. } | Bool(_) => (),
            Not(exp) | Bvnot(exp) | Bvneg(exp) | Extract(_, _, exp) | ZeroExtend(_, exp) | SignExtend(_, exp) => {
                exp.modify(f)
            }
            Eq(lhs, rhs)
            | Neq(lhs, rhs)
            | And(lhs, rhs)
            | Or(lhs, rhs)
            | Bvand(lhs, rhs)
            | Bvor(lhs, rhs)
            | Bvxor(lhs, rhs)
            | Bvnand(lhs, rhs)
            | Bvnor(lhs, rhs)
            | Bvxnor(lhs, rhs)
            | Bvadd(lhs, rhs)
            | Bvsub(lhs, rhs)
            | Bvmul(lhs, rhs)
            | Bvudiv(lhs, rhs)
            | Bvsdiv(lhs, rhs)
            | Bvurem(lhs, rhs)
            | Bvsrem(lhs, rhs)
            | Bvsmod(lhs, rhs)
            | Bvult(lhs, rhs)
            | Bvslt(lhs, rhs)
            | Bvule(lhs, rhs)
            | Bvsle(lhs, rhs)
            | Bvuge(lhs, rhs)
            | Bvsge(lhs, rhs)
            | Bvugt(lhs, rhs)
            | Bvsgt(lhs, rhs)
            | Bvshl(lhs, rhs)
            | Bvlshr(lhs, rhs)
            | Bvashr(lhs, rhs)
            | Concat(lhs, rhs) => {
                lhs.modify(f);
                rhs.modify(f);
            }
            Ite(cond, then_exp, else_exp) => {
                cond.modify(f);
                then_exp.modify(f);
                else_exp.modify(f)
            }
            App(_, args) => {
                for exp in args {
                    exp.modify(f)
                }
            }
            Select(array, index) => {
                array.modify(f);
                index.modify(f);
            }
            Store(array, index, val) => {
                array.modify(f);
                index.modify(f);
                val.modify(f);
            }
            Distinct(exps) => {
                for exp in exps {
                    exp.modify(f)
                }
            }
        }
    }
    /// Apply the given function at every [Var]
    pub fn visit_vars<F>(&self, f: &mut F)
    where
        F: FnMut(&V),
    {
        use Exp::*;
        match self {
            Var(v) => f(v),
            Bits(_) | Bits64(_) | Enum { .. } | Bool(_) => (),
            Not(exp) | Bvnot(exp) | Bvneg(exp) | Extract(_, _, exp) | ZeroExtend(_, exp) | SignExtend(_, exp) => {
                exp.visit_vars(f)
            }
            Eq(lhs, rhs)
            | Neq(lhs, rhs)
            | And(lhs, rhs)
            | Or(lhs, rhs)
            | Bvand(lhs, rhs)
            | Bvor(lhs, rhs)
            | Bvxor(lhs, rhs)
            | Bvnand(lhs, rhs)
            | Bvnor(lhs, rhs)
            | Bvxnor(lhs, rhs)
            | Bvadd(lhs, rhs)
            | Bvsub(lhs, rhs)
            | Bvmul(lhs, rhs)
            | Bvudiv(lhs, rhs)
            | Bvsdiv(lhs, rhs)
            | Bvurem(lhs, rhs)
            | Bvsrem(lhs, rhs)
            | Bvsmod(lhs, rhs)
            | Bvult(lhs, rhs)
            | Bvslt(lhs, rhs)
            | Bvule(lhs, rhs)
            | Bvsle(lhs, rhs)
            | Bvuge(lhs, rhs)
            | Bvsge(lhs, rhs)
            | Bvugt(lhs, rhs)
            | Bvsgt(lhs, rhs)
            | Bvshl(lhs, rhs)
            | Bvlshr(lhs, rhs)
            | Bvashr(lhs, rhs)
            | Concat(lhs, rhs) => {
                lhs.visit_vars(f);
                rhs.visit_vars(f);
            }
            Ite(cond, then_exp, else_exp) => {
                cond.visit_vars(f);
                then_exp.visit_vars(f);
                else_exp.visit_vars(f)
            }
            App(_, args) => {
                for exp in args {
                    exp.visit_vars(f)
                }
            }
            Select(array, index) => {
                array.visit_vars(f);
                index.visit_vars(f);
            }
            Store(array, index, val) => {
                array.visit_vars(f);
                index.visit_vars(f);
                val.visit_vars(f);
            }
            Distinct(exps) => {
                for exp in exps {
                    exp.visit_vars(f)
                }
            }
        }
    }

    fn binary_commute_extract(self) -> Result<(fn(Box<Self>, Box<Self>) -> Self, Box<Self>, Box<Self>), Self> {
        use Exp::*;
        match self {
            Bvand(lhs, rhs) => Ok((Bvand, lhs, rhs)),
            Bvor(lhs, rhs) => Ok((Bvor, lhs, rhs)),
            Bvxor(lhs, rhs) => Ok((Bvxor, lhs, rhs)),
            Bvnand(lhs, rhs) => Ok((Bvnand, lhs, rhs)),
            Bvnor(lhs, rhs) => Ok((Bvnor, lhs, rhs)),
            Bvxnor(lhs, rhs) => Ok((Bvxnor, lhs, rhs)),
            Bvadd(lhs, rhs) => Ok((Bvadd, lhs, rhs)),
            Bvsub(lhs, rhs) => Ok((Bvsub, lhs, rhs)),
            _ => Err(self),
        }
    }

    pub fn commute_extract(&mut self) {
        use Exp::*;
        if let Extract(hi, lo, exp) = self {
            match std::mem::replace(&mut **exp, Bool(false)).binary_commute_extract() {
                Ok((op, lhs, rhs)) => *self = op(Box::new(Extract(*hi, *lo, lhs)), Box::new(Extract(*hi, *lo, rhs))),
                Err(mut orig_exp) => {
                    std::mem::swap(&mut **exp, &mut orig_exp);
                }
            }
        }
    }
}

impl<'a, V: 'a> Exp<V> {
    pub fn map_var<F,Err,V2>(&'a self, f: &mut F) -> Result<Exp<V2>, Err> where F: FnMut(&'a V) -> Result<Exp<V2>, Err> {
        use Exp::*;
        match self {
            Var(v) => Ok(f(v)?),
            Bits(bv) => Ok(Bits(bv.clone())),
            Bits64(bs) => Ok(Bits64(*bs)),
            Enum(mem) => Ok(Enum(*mem)),
            Bool(b) => Ok(Bool(*b)),
            Not(exp) => Ok(Not(Box::new(exp.map_var(f)?))),
            Bvnot(exp) => Ok(Bvnot(Box::new(exp.map_var(f)?))),
            Bvneg(exp) => Ok(Bvneg(Box::new(exp.map_var(f)?))),
            Extract(hi, lo, exp) => Ok(Extract(*hi, *lo, Box::new(exp.map_var(f)?))),
            ZeroExtend(n, exp) => Ok(ZeroExtend(*n, Box::new(exp.map_var(f)?))),
            SignExtend(n, exp) => Ok(SignExtend(*n, Box::new(exp.map_var(f)?))),
            Eq(lhs, rhs) => Ok(Eq(Box::new(lhs.map_var(f)?), Box::new(rhs.map_var(f)?))),
            Neq(lhs, rhs) => Ok(Neq(Box::new(lhs.map_var(f)?), Box::new(rhs.map_var(f)?))),
            And(lhs, rhs) => Ok(And(Box::new(lhs.map_var(f)?), Box::new(rhs.map_var(f)?))),
            Or(lhs, rhs) => Ok(Or(Box::new(lhs.map_var(f)?), Box::new(rhs.map_var(f)?))),
            Bvand(lhs, rhs) => Ok(Bvand(Box::new(lhs.map_var(f)?), Box::new(rhs.map_var(f)?))),
            Bvor(lhs, rhs) => Ok(Bvor(Box::new(lhs.map_var(f)?), Box::new(rhs.map_var(f)?))),
            Bvxor(lhs, rhs) => Ok(Bvxor(Box::new(lhs.map_var(f)?), Box::new(rhs.map_var(f)?))),
            Bvnand(lhs, rhs) => Ok(Bvnand(Box::new(lhs.map_var(f)?), Box::new(rhs.map_var(f)?))),
            Bvnor(lhs, rhs) => Ok(Bvnor(Box::new(lhs.map_var(f)?), Box::new(rhs.map_var(f)?))),
            Bvxnor(lhs, rhs) => Ok(Bvxnor(Box::new(lhs.map_var(f)?), Box::new(rhs.map_var(f)?))),
            Bvadd(lhs, rhs) => Ok(Bvadd(Box::new(lhs.map_var(f)?), Box::new(rhs.map_var(f)?))),
            Bvsub(lhs, rhs) => Ok(Bvsub(Box::new(lhs.map_var(f)?), Box::new(rhs.map_var(f)?))),
            Bvmul(lhs, rhs) => Ok(Bvmul(Box::new(lhs.map_var(f)?), Box::new(rhs.map_var(f)?))),
            Bvudiv(lhs, rhs) => Ok(Bvudiv(Box::new(lhs.map_var(f)?), Box::new(rhs.map_var(f)?))),
            Bvsdiv(lhs, rhs) => Ok(Bvsdiv(Box::new(lhs.map_var(f)?), Box::new(rhs.map_var(f)?))),
            Bvurem(lhs, rhs) => Ok(Bvurem(Box::new(lhs.map_var(f)?), Box::new(rhs.map_var(f)?))),
            Bvsrem(lhs, rhs) => Ok(Bvsrem(Box::new(lhs.map_var(f)?), Box::new(rhs.map_var(f)?))),
            Bvsmod(lhs, rhs) => Ok(Bvsmod(Box::new(lhs.map_var(f)?), Box::new(rhs.map_var(f)?))),
            Bvult(lhs, rhs) => Ok(Bvult(Box::new(lhs.map_var(f)?), Box::new(rhs.map_var(f)?))),
            Bvslt(lhs, rhs) => Ok(Bvslt(Box::new(lhs.map_var(f)?), Box::new(rhs.map_var(f)?))),
            Bvule(lhs, rhs) => Ok(Bvule(Box::new(lhs.map_var(f)?), Box::new(rhs.map_var(f)?))),
            Bvsle(lhs, rhs) => Ok(Bvsle(Box::new(lhs.map_var(f)?), Box::new(rhs.map_var(f)?))),
            Bvuge(lhs, rhs) => Ok(Bvuge(Box::new(lhs.map_var(f)?), Box::new(rhs.map_var(f)?))),
            Bvsge(lhs, rhs) => Ok(Bvsge(Box::new(lhs.map_var(f)?), Box::new(rhs.map_var(f)?))),
            Bvugt(lhs, rhs) => Ok(Bvugt(Box::new(lhs.map_var(f)?), Box::new(rhs.map_var(f)?))),
            Bvsgt(lhs, rhs) => Ok(Bvsgt(Box::new(lhs.map_var(f)?), Box::new(rhs.map_var(f)?))),
            Bvshl(lhs, rhs) => Ok(Bvshl(Box::new(lhs.map_var(f)?), Box::new(rhs.map_var(f)?))),
            Bvlshr(lhs, rhs) => Ok(Bvlshr(Box::new(lhs.map_var(f)?), Box::new(rhs.map_var(f)?))),
            Bvashr(lhs, rhs) => Ok(Bvashr(Box::new(lhs.map_var(f)?), Box::new(rhs.map_var(f)?))),
            Concat(lhs, rhs) => Ok(Concat(Box::new(lhs.map_var(f)?), Box::new(rhs.map_var(f)?))),
            Ite(cond, then_exp, else_exp) => Ok(Ite(Box::new(cond.map_var(f)?), Box::new(then_exp.map_var(f)?), Box::new(else_exp.map_var(f)?))),
            App(name, args) => Ok(App(*name, args.iter().map(|exp| exp.map_var(f)).collect::<Result::<Vec<_>, _>>()?)),
            Select(array, index) => Ok(Select(Box::new(array.map_var(f)?), Box::new(index.map_var(f)?))),
            Store(array, index, val) => Ok(Store(Box::new(array.map_var(f)?), Box::new(index.map_var(f)?), Box::new(val.map_var(f)?))),
            Distinct(exps) => Ok(Distinct(exps.iter().map(|exp| exp.map_var(f)).collect::<Result::<Vec<_>, _>>()?)),
        }
    }
}

impl Exp<Sym> {
    pub fn subst_once_in_place(&mut self, substs: &mut HashMap<Sym, Option<Self>>) {
        use Exp::*;
        match self {
            Var(v) => {
                if let Some(exp) = substs.get_mut(v) {
                    if let Some(exp) = exp.take() {
                        *self = exp
                    } else {
                        panic!("Tried to substitute twice in subst_once_in_place")
                    }
                }
            }
            Bits(_) | Bits64(_) | Enum { .. } | Bool(_) => (),
            Not(exp) | Bvnot(exp) | Bvneg(exp) | Extract(_, _, exp) | ZeroExtend(_, exp) | SignExtend(_, exp) => {
                exp.subst_once_in_place(substs)
            }
            Eq(lhs, rhs)
            | Neq(lhs, rhs)
            | And(lhs, rhs)
            | Or(lhs, rhs)
            | Bvand(lhs, rhs)
            | Bvor(lhs, rhs)
            | Bvxor(lhs, rhs)
            | Bvnand(lhs, rhs)
            | Bvnor(lhs, rhs)
            | Bvxnor(lhs, rhs)
            | Bvadd(lhs, rhs)
            | Bvsub(lhs, rhs)
            | Bvmul(lhs, rhs)
            | Bvudiv(lhs, rhs)
            | Bvsdiv(lhs, rhs)
            | Bvurem(lhs, rhs)
            | Bvsrem(lhs, rhs)
            | Bvsmod(lhs, rhs)
            | Bvult(lhs, rhs)
            | Bvslt(lhs, rhs)
            | Bvule(lhs, rhs)
            | Bvsle(lhs, rhs)
            | Bvuge(lhs, rhs)
            | Bvsge(lhs, rhs)
            | Bvugt(lhs, rhs)
            | Bvsgt(lhs, rhs)
            | Bvshl(lhs, rhs)
            | Bvlshr(lhs, rhs)
            | Bvashr(lhs, rhs)
            | Concat(lhs, rhs) => {
                lhs.subst_once_in_place(substs);
                rhs.subst_once_in_place(substs);
            }
            Ite(cond, then_exp, else_exp) => {
                cond.subst_once_in_place(substs);
                then_exp.subst_once_in_place(substs);
                else_exp.subst_once_in_place(substs)
            }
            App(_, args) => {
                for exp in args {
                    exp.subst_once_in_place(substs)
                }
            }
            Select(array, index) => {
                array.subst_once_in_place(substs);
                index.subst_once_in_place(substs);
            }
            Store(array, index, val) => {
                array.subst_once_in_place(substs);
                index.subst_once_in_place(substs);
                val.subst_once_in_place(substs);
            }
            Distinct(exps) => {
                for exp in exps {
                    exp.subst_once_in_place(substs)
                }
            }
        }
    }

    /// Infer the type of an already well-formed SMTLIB expression
    pub fn infer(&self, tcx: &HashMap<Sym, Ty>, ftcx: &HashMap<Sym, (Vec<Ty>, Ty)>) -> Option<Ty> {
        use Exp::*;
        match self {
            Var(v) => tcx.get(v).map(Ty::clone),
            Bits(bv) => Some(Ty::BitVec(bv.len() as u32)),
            Bits64(bv) => Some(Ty::BitVec(bv.len())),
            Enum(e) => Some(Ty::Enum(e.enum_id)),
            Bool(_)
            | Not(_)
            | Eq(_, _)
            | Neq(_, _)
            | And(_, _)
            | Or(_, _)
            | Bvult(_, _)
            | Bvslt(_, _)
            | Bvule(_, _)
            | Bvsle(_, _)
            | Bvuge(_, _)
            | Bvsge(_, _)
            | Bvugt(_, _)
            | Bvsgt(_, _)
            | Distinct(_) => Some(Ty::Bool),
            Bvnot(exp) | Bvneg(exp) => exp.infer(tcx, ftcx),
            Extract(i, j, _) => Some(Ty::BitVec((i - j) + 1)),
            ZeroExtend(ext, exp) | SignExtend(ext, exp) => match exp.infer(tcx, ftcx) {
                Some(Ty::BitVec(sz)) => Some(Ty::BitVec(sz + ext)),
                _ => None,
            },
            Bvand(lhs, _)
            | Bvor(lhs, _)
            | Bvxor(lhs, _)
            | Bvnand(lhs, _)
            | Bvnor(lhs, _)
            | Bvxnor(lhs, _)
            | Bvadd(lhs, _)
            | Bvsub(lhs, _)
            | Bvmul(lhs, _)
            | Bvudiv(lhs, _)
            | Bvsdiv(lhs, _)
            | Bvurem(lhs, _)
            | Bvsrem(lhs, _)
            | Bvsmod(lhs, _)
            | Bvshl(lhs, _)
            | Bvlshr(lhs, _)
            | Bvashr(lhs, _) => lhs.infer(tcx, ftcx),
            Concat(lhs, rhs) => match (lhs.infer(tcx, ftcx), rhs.infer(tcx, ftcx)) {
                (Some(Ty::BitVec(lsz)), Some(Ty::BitVec(rsz))) => Some(Ty::BitVec(lsz + rsz)),
                (_, _) => None,
            },
            Ite(_, then_exp, _) => then_exp.infer(tcx, ftcx),
            App(f, _) => ftcx.get(f).map(|x| x.1.clone()),
            Select(array, _) => match array.infer(tcx, ftcx) {
                Some(Ty::Array(_, codom_ty)) => Some(*codom_ty),
                _ => None,
            },
            Store(array, _, _) => array.infer(tcx, ftcx),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Def {
    DeclareConst(Sym, Ty),
    DeclareFun(Sym, Vec<Ty>, Ty),
    DefineConst(Sym, Exp<Sym>),
    DefineEnum(Sym, usize),
    Assert(Exp<Sym>),
}
