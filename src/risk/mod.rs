use crate::amount::{Amount, BPS_DENOMINATOR, Bps};
use crate::debt::DebtQuote;
use crate::error::{ChronosError, ChronosResult};
use crate::ids::Epoch;
use crate::locks::LockRequest;
use crate::pools::PoolState;
use crate::position::{PositionRecord, PositionTerms};
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RiskSignal {
    Accepted,
    CollateralTooLow,
    PoolUtilizationTooHigh,
    PositionTooLarge,
    TooManyOpenPositions,
    LockWindowInvalid,
    SettlementTooSmall,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RiskDecision {
    pub accepted: bool,
    pub signal: RiskSignal,
    pub message: String,
}

impl RiskDecision {
    pub fn accept(message: impl Into<String>) -> Self {
        Self {
            accepted: true,
            signal: RiskSignal::Accepted,
            message: message.into(),
        }
    }

    pub fn reject(signal: RiskSignal, message: impl Into<String>) -> Self {
        Self {
            accepted: false,
            signal,
            message: message.into(),
        }
    }

    pub fn into_result(self) -> ChronosResult<()> {
        if self.accepted {
            Ok(())
        } else {
            Err(ChronosError::risk(self.message))
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RiskLimits {
    pub max_position_principal: Amount,
    pub min_collateral_bps: Bps,
    pub max_open_positions_per_account: usize,
    pub min_lock_epochs: u64,
    pub max_lock_epochs: u64,
}

impl Default for RiskLimits {
    fn default() -> Self {
        Self {
            max_position_principal: Amount::new(50_000_000_000),
            min_collateral_bps: Bps::from_raw_unchecked(1_200),
            max_open_positions_per_account: 64,
            min_lock_epochs: 1,
            max_lock_epochs: 45,
        }
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct RiskEngine {
    pub limits: RiskLimits,
}

impl RiskEngine {
    pub fn new(limits: RiskLimits) -> Self {
        Self { limits }
    }

    pub fn evaluate_open(
        &self,
        terms: PositionTerms,
        pool: &PoolState,
        open_positions_for_account: usize,
    ) -> ChronosResult<RiskDecision> {
        if terms.principal > self.limits.max_position_principal {
            return Ok(RiskDecision::reject(
                RiskSignal::PositionTooLarge,
                "position principal exceeds policy",
            ));
        }
        if open_positions_for_account >= self.limits.max_open_positions_per_account {
            return Ok(RiskDecision::reject(
                RiskSignal::TooManyOpenPositions,
                "account has too many open positions",
            ));
        }
        let required_collateral = terms.principal.mul_bps(self.limits.min_collateral_bps)?;
        if terms.collateral < required_collateral {
            return Ok(RiskDecision::reject(
                RiskSignal::CollateralTooLow,
                "collateral below policy",
            ));
        }
        let total_after = pool
            .liquidity_available
            .checked_add(pool.principal_outstanding)?
            .max(Amount::new(1));
        let utilization_after_raw = pool
            .principal_outstanding
            .checked_add(terms.principal)?
            .raw()
            .checked_mul(BPS_DENOMINATOR)
            .and_then(|value| value.checked_div(total_after.raw()))
            .ok_or(ChronosError::AmountOverflow)?;
        let utilization_after = Bps::new(utilization_after_raw as u32)?;
        if utilization_after > pool.config.max_utilization_bps {
            return Ok(RiskDecision::reject(
                RiskSignal::PoolUtilizationTooHigh,
                "pool utilization would exceed limit",
            ));
        }
        Ok(RiskDecision::accept("open position accepted"))
    }

    pub fn evaluate_lock(
        &self,
        position: &PositionRecord,
        request: &LockRequest,
        now: Epoch,
    ) -> RiskDecision {
        if request.owner != position.borrower {
            return RiskDecision::reject(RiskSignal::LockWindowInvalid, "owner mismatch");
        }
        if request.release_epoch <= now {
            return RiskDecision::reject(
                RiskSignal::LockWindowInvalid,
                "release epoch is not future",
            );
        }
        let span = now.distance_to(request.release_epoch);
        if span < self.limits.min_lock_epochs || span > self.limits.max_lock_epochs {
            return RiskDecision::reject(RiskSignal::LockWindowInvalid, "lock span outside policy");
        }
        RiskDecision::accept("lock accepted")
    }

    pub fn evaluate_close(&self, position: &PositionRecord, quote: DebtQuote) -> RiskDecision {
        match quote.total_due() {
            Ok(total) if total >= position.terms.min_close_amount => {
                RiskDecision::accept("close accepted")
            }
            Ok(_) => {
                RiskDecision::reject(RiskSignal::SettlementTooSmall, "settlement below minimum")
            }
            Err(error) => RiskDecision::reject(RiskSignal::SettlementTooSmall, error.to_string()),
        }
    }
}
