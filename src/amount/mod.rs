use crate::error::{ChronosError, ChronosResult};
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};
use std::iter::Sum;
use std::ops::{Add, AddAssign, Sub, SubAssign};

pub const INDEX_SCALE: u128 = 1_000_000_000_000;
pub const BPS_DENOMINATOR: u128 = 10_000;
pub const MAX_BPS: u32 = 100_000;

#[derive(
    Copy, Clone, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
pub struct Amount(u128);

impl Amount {
    pub const ZERO: Self = Self(0);

    pub const fn new(value: u128) -> Self {
        Self(value)
    }

    pub const fn raw(self) -> u128 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub fn non_zero(self) -> ChronosResult<Self> {
        if self.is_zero() {
            Err(ChronosError::ZeroAmount)
        } else {
            Ok(self)
        }
    }

    pub fn checked_add(self, rhs: Self) -> ChronosResult<Self> {
        self.0
            .checked_add(rhs.0)
            .map(Self)
            .ok_or(ChronosError::AmountOverflow)
    }

    pub fn checked_sub(self, rhs: Self) -> ChronosResult<Self> {
        self.0
            .checked_sub(rhs.0)
            .map(Self)
            .ok_or(ChronosError::AmountOverflow)
    }

    pub fn saturating_sub(self, rhs: Self) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }

    pub fn checked_mul(self, rhs: u128) -> ChronosResult<Self> {
        self.0
            .checked_mul(rhs)
            .map(Self)
            .ok_or(ChronosError::AmountOverflow)
    }

    pub fn checked_div(self, rhs: u128) -> ChronosResult<Self> {
        if rhs == 0 {
            return Err(ChronosError::AmountOverflow);
        }
        Ok(Self(self.0 / rhs))
    }

    pub fn mul_bps(self, bps: Bps) -> ChronosResult<Self> {
        self.0
            .checked_mul(u128::from(bps.raw()))
            .and_then(|v| v.checked_div(BPS_DENOMINATOR))
            .map(Self)
            .ok_or(ChronosError::AmountOverflow)
    }

    pub fn ceil_bps(self, bps: Bps) -> ChronosResult<Self> {
        let numerator = self
            .0
            .checked_mul(u128::from(bps.raw()))
            .ok_or(ChronosError::AmountOverflow)?;
        let adjusted = numerator
            .checked_add(BPS_DENOMINATOR - 1)
            .ok_or(ChronosError::AmountOverflow)?;
        Ok(Self(adjusted / BPS_DENOMINATOR))
    }

    pub fn share_of(self, numerator: u128, denominator: u128) -> ChronosResult<Self> {
        if denominator == 0 {
            return Err(ChronosError::AmountOverflow);
        }
        self.0
            .checked_mul(numerator)
            .and_then(|value| value.checked_div(denominator))
            .map(Self)
            .ok_or(ChronosError::AmountOverflow)
    }

    pub fn min(self, rhs: Self) -> Self {
        if self <= rhs { self } else { rhs }
    }

    pub fn max(self, rhs: Self) -> Self {
        if self >= rhs { self } else { rhs }
    }
}

impl Add for Amount {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0.saturating_add(rhs.0))
    }
}

impl AddAssign for Amount {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl Sub for Amount {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0.saturating_sub(rhs.0))
    }
}

impl SubAssign for Amount {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl Sum for Amount {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, |acc, value| acc + value)
    }
}

impl From<u128> for Amount {
    fn from(value: u128) -> Self {
        Self::new(value)
    }
}

impl Display for Amount {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(
    Copy, Clone, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
pub struct Bps(u32);

impl Bps {
    pub const ZERO: Self = Self(0);

    pub fn new(value: u32) -> ChronosResult<Self> {
        if value > MAX_BPS {
            Err(ChronosError::BpsOutOfRange(value))
        } else {
            Ok(Self(value))
        }
    }

    pub const fn from_raw_unchecked(value: u32) -> Self {
        Self(value)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub fn checked_add(self, rhs: Self) -> ChronosResult<Self> {
        let value = self
            .0
            .checked_add(rhs.0)
            .ok_or(ChronosError::BpsOutOfRange(u32::MAX))?;
        Self::new(value)
    }

    pub fn checked_sub(self, rhs: Self) -> ChronosResult<Self> {
        let value = self
            .0
            .checked_sub(rhs.0)
            .ok_or(ChronosError::BpsOutOfRange(0))?;
        Self::new(value)
    }

    pub fn clamp(self, max: Self) -> Self {
        if self > max { max } else { self }
    }
}

impl Display for Bps {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}bps", self.0)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct AccrualIndex(u128);

impl Default for AccrualIndex {
    fn default() -> Self {
        Self::one()
    }
}

impl AccrualIndex {
    pub const fn new(value: u128) -> Self {
        Self(value)
    }

    pub const fn one() -> Self {
        Self(INDEX_SCALE)
    }

    pub const fn raw(self) -> u128 {
        self.0
    }

    pub fn checked_add_scaled(self, scaled_delta: u128) -> ChronosResult<Self> {
        self.0
            .checked_add(scaled_delta)
            .map(Self)
            .ok_or(ChronosError::AmountOverflow)
    }

    pub fn compound_bps(self, bps: Bps) -> ChronosResult<Self> {
        let delta = self
            .0
            .checked_mul(u128::from(bps.raw()))
            .and_then(|value| value.checked_div(BPS_DENOMINATOR))
            .ok_or(ChronosError::AmountOverflow)?;
        self.checked_add_scaled(delta)
    }

    pub fn amount_delta(self, principal: Amount, previous: Self) -> ChronosResult<Amount> {
        if self.0 <= previous.0 {
            return Ok(Amount::ZERO);
        }
        let delta = self.0 - previous.0;
        principal
            .raw()
            .checked_mul(delta)
            .and_then(|value| value.checked_div(INDEX_SCALE))
            .map(Amount::new)
            .ok_or(ChronosError::AmountOverflow)
    }

    pub fn scale_amount(self, amount: Amount) -> ChronosResult<Amount> {
        amount
            .raw()
            .checked_mul(self.0)
            .and_then(|value| value.checked_div(INDEX_SCALE))
            .map(Amount::new)
            .ok_or(ChronosError::AmountOverflow)
    }

    pub fn ratio_to(self, previous: Self) -> ChronosResult<u128> {
        if previous.0 == 0 {
            return Err(ChronosError::AmountOverflow);
        }
        self.0
            .checked_mul(INDEX_SCALE)
            .and_then(|value| value.checked_div(previous.0))
            .ok_or(ChronosError::AmountOverflow)
    }
}

impl Display for AccrualIndex {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AmountPair {
    pub principal: Amount,
    pub charges: Amount,
}

impl AmountPair {
    pub fn total(self) -> ChronosResult<Amount> {
        self.principal.checked_add(self.charges)
    }
}
