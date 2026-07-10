use crate::amount::{Amount, Bps};
use crate::debt::DebtQuote;
use crate::error::{ChronosError, ChronosResult};
use crate::ids::{AccountId, AssetId, Epoch, PoolId, PositionId, TxId};
use crate::position::{PositionRecord, PositionState};
use crate::time::EpochPolicy;
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExpiryPolicy {
    pub collateral_penalty_bps: Bps,
    pub minimum_sweep_delay_epochs: u64,
}

impl Default for ExpiryPolicy {
    fn default() -> Self {
        Self {
            collateral_penalty_bps: Bps::from_raw_unchecked(2_500),
            minimum_sweep_delay_epochs: 1,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ExpiryDecision {
    NotReady,
    SweepCollateral,
    AlreadyTerminal,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExpiryReceipt {
    pub tx: TxId,
    pub position: PositionId,
    pub borrower: AccountId,
    pub pool: PoolId,
    pub asset: AssetId,
    pub decided_at_epoch: Epoch,
    pub decision: ExpiryDecision,
    pub quote: DebtQuote,
    pub collateral_absorbed: Amount,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExpiryEngine {
    pub policy: ExpiryPolicy,
}

impl ExpiryEngine {
    pub fn new(policy: ExpiryPolicy) -> Self {
        Self { policy }
    }

    pub fn decision(
        &self,
        position: &PositionRecord,
        now: Epoch,
        epoch_policy: EpochPolicy,
    ) -> ChronosResult<ExpiryDecision> {
        if position.state.is_terminal() {
            return Ok(ExpiryDecision::AlreadyTerminal);
        }
        let sweep_epoch = epoch_policy.sweep_epoch(position.effective_maturity_epoch)?;
        let policy_epoch = position
            .effective_maturity_epoch
            .checked_add(self.policy.minimum_sweep_delay_epochs)
            .ok_or(ChronosError::EpochOutOfRange(
                position.effective_maturity_epoch,
            ))?;
        if now >= sweep_epoch.max(policy_epoch) {
            Ok(ExpiryDecision::SweepCollateral)
        } else {
            Ok(ExpiryDecision::NotReady)
        }
    }

    pub fn collateral_absorption(&self, position: &PositionRecord) -> ChronosResult<Amount> {
        let penalty = position
            .collateral()
            .mul_bps(self.policy.collateral_penalty_bps)?;
        Ok(position
            .collateral()
            .min(penalty.max(position.collateral())))
    }

    pub fn visible_state(&self, decision: ExpiryDecision) -> PositionState {
        match decision {
            ExpiryDecision::SweepCollateral => PositionState::Expired,
            ExpiryDecision::NotReady => PositionState::InGrace,
            ExpiryDecision::AlreadyTerminal => PositionState::Closed,
        }
    }
}
