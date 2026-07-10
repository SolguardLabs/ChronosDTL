use crate::amount::{Amount, Bps};
use crate::error::{ChronosError, ChronosResult};
use crate::ids::Epoch;
use crate::position::{AccrualCheckpoint, PositionRecord, PositionState};
use crate::rates::AccrualState;
use crate::time::EpochPolicy;
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DebtQuoteInput {
    pub now: Epoch,
    pub policy: EpochPolicy,
    pub accrual: AccrualState,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DebtBreakdown {
    pub principal: Amount,
    pub interest: Amount,
    pub penalty: Amount,
    pub close_fee: Amount,
    pub reserve_cut: Amount,
}

impl DebtBreakdown {
    pub fn charges(self) -> ChronosResult<Amount> {
        self.interest
            .checked_add(self.penalty)?
            .checked_add(self.close_fee)
    }

    pub fn total(self) -> ChronosResult<Amount> {
        self.principal.checked_add(self.charges()?)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DebtQuote {
    pub state: PositionState,
    pub checkpoint: AccrualCheckpoint,
    pub breakdown: DebtBreakdown,
    pub effective_maturity_epoch: Epoch,
    pub quoted_at_epoch: Epoch,
}

impl DebtQuote {
    pub fn total_due(self) -> ChronosResult<Amount> {
        self.breakdown.total()
    }

    pub fn charges_due(self) -> ChronosResult<Amount> {
        self.breakdown.charges()
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DebtCalculator {
    pub close_fee_bps: Bps,
    pub reserve_factor_bps: Bps,
}

impl Default for DebtCalculator {
    fn default() -> Self {
        Self {
            close_fee_bps: Bps::from_raw_unchecked(5),
            reserve_factor_bps: Bps::from_raw_unchecked(1_000),
        }
    }
}

impl DebtCalculator {
    pub fn classify(
        &self,
        position: &PositionRecord,
        now: Epoch,
        policy: EpochPolicy,
    ) -> PositionState {
        if position.state.is_terminal() {
            return position.state;
        }
        if position.state == PositionState::Locked {
            return PositionState::Locked;
        }
        if now < position.effective_maturity_epoch {
            PositionState::Active
        } else if now == position.effective_maturity_epoch {
            PositionState::Matured
        } else if now
            <= policy
                .grace_deadline(position.effective_maturity_epoch)
                .unwrap_or(now)
        {
            PositionState::InGrace
        } else {
            PositionState::Expired
        }
    }

    pub fn quote(
        &self,
        position: &PositionRecord,
        input: DebtQuoteInput,
    ) -> ChronosResult<DebtQuote> {
        if !position.state.admits_close() {
            return Err(ChronosError::PositionState(position.id));
        }
        if input.accrual.pool != position.pool {
            return Err(ChronosError::UnknownPool(position.pool));
        }
        let state = self.classify(position, input.now, input.policy);
        let interest = input
            .accrual
            .interest_index
            .amount_delta(position.principal(), position.checkpoint.interest_index)?
            .checked_add(position.pending_interest)?;
        let penalty_delta = if input.now > position.effective_maturity_epoch {
            input
                .accrual
                .penalty_index
                .amount_delta(position.principal(), position.checkpoint.penalty_index)?
        } else {
            Amount::ZERO
        };
        let penalty = penalty_delta.checked_add(position.pending_penalty)?;
        let close_fee = position.principal().mul_bps(self.close_fee_bps)?;
        let reserve_cut = interest
            .checked_add(penalty)?
            .mul_bps(self.reserve_factor_bps)?;
        Ok(DebtQuote {
            state,
            checkpoint: AccrualCheckpoint::from_state(input.accrual),
            breakdown: DebtBreakdown {
                principal: position.principal(),
                interest,
                penalty,
                close_fee,
                reserve_cut,
            },
            effective_maturity_epoch: position.effective_maturity_epoch,
            quoted_at_epoch: input.now,
        })
    }

    pub fn quote_materialized_delta(
        &self,
        position: &PositionRecord,
        input: DebtQuoteInput,
    ) -> ChronosResult<(Amount, Amount, AccrualCheckpoint)> {
        let quote = self.quote(position, input)?;
        let interest_delta = quote
            .breakdown
            .interest
            .saturating_sub(position.pending_interest);
        let penalty_delta = quote
            .breakdown
            .penalty
            .saturating_sub(position.pending_penalty);
        Ok((interest_delta, penalty_delta, quote.checkpoint))
    }
}
