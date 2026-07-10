use crate::amount::Amount;
use crate::debt::DebtQuote;
use crate::error::{ChronosError, ChronosResult};
use crate::ids::{AccountId, Epoch, LockId, OperatorId, PositionId};
use crate::position::{AccrualCheckpoint, PositionState};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum LockMode {
    Repayment,
    Rollover,
    GraceExtension,
    OperatorReview,
    AuctionHold,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum LockStatus {
    Active,
    Released,
    Cancelled,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ReleaseDecision {
    Pending,
    Releasable,
    Expired,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LockRequest {
    pub position: PositionId,
    pub owner: AccountId,
    pub release_epoch: Epoch,
    pub mode: LockMode,
    pub operator: Option<OperatorId>,
    pub reference: String,
}

impl LockRequest {
    pub fn new(
        position: PositionId,
        owner: AccountId,
        release_epoch: Epoch,
        mode: LockMode,
    ) -> Self {
        Self {
            position,
            owner,
            release_epoch,
            mode,
            operator: None,
            reference: String::new(),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LockSnapshot {
    pub previous_state: PositionState,
    pub previous_effective_maturity_epoch: Epoch,
    pub previous_checkpoint: AccrualCheckpoint,
    pub quoted_interest: Amount,
    pub quoted_penalty: Amount,
    pub state_version: u64,
}

impl LockSnapshot {
    pub fn from_quote(
        state: PositionState,
        maturity: Epoch,
        checkpoint: AccrualCheckpoint,
        state_version: u64,
        quote: DebtQuote,
    ) -> Self {
        Self {
            previous_state: state,
            previous_effective_maturity_epoch: maturity,
            previous_checkpoint: checkpoint,
            quoted_interest: quote.breakdown.interest,
            quoted_penalty: quote.breakdown.penalty,
            state_version,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LockRecord {
    pub id: LockId,
    pub position: PositionId,
    pub owner: AccountId,
    pub mode: LockMode,
    pub operator: Option<OperatorId>,
    pub created_epoch: Epoch,
    pub release_epoch: Epoch,
    pub status: LockStatus,
    pub snapshot: LockSnapshot,
    pub reference: String,
}

impl LockRecord {
    pub fn decision_at(&self, epoch: Epoch) -> ReleaseDecision {
        match self.status {
            LockStatus::Released | LockStatus::Cancelled => ReleaseDecision::Expired,
            LockStatus::Active if epoch >= self.release_epoch => ReleaseDecision::Releasable,
            LockStatus::Active => ReleaseDecision::Pending,
        }
    }

    pub fn release(&mut self, epoch: Epoch) -> ChronosResult<()> {
        if self.decision_at(epoch) != ReleaseDecision::Releasable {
            return Err(ChronosError::LockNotReleasable(self.id));
        }
        self.status = LockStatus::Released;
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LockBook {
    locks: HashMap<LockId, LockRecord>,
}

impl LockBook {
    pub fn insert(&mut self, record: LockRecord) -> ChronosResult<()> {
        if self.locks.contains_key(&record.id) {
            return Err(ChronosError::invalid(format!(
                "lock {} already exists",
                record.id
            )));
        }
        self.locks.insert(record.id, record);
        Ok(())
    }

    pub fn get(&self, id: LockId) -> ChronosResult<&LockRecord> {
        self.locks.get(&id).ok_or(ChronosError::UnknownLock(id))
    }

    pub fn get_mut(&mut self, id: LockId) -> ChronosResult<&mut LockRecord> {
        self.locks.get_mut(&id).ok_or(ChronosError::UnknownLock(id))
    }

    pub fn active_for_position(&self, position: PositionId) -> Option<&LockRecord> {
        self.locks
            .values()
            .find(|lock| lock.position == position && lock.status == LockStatus::Active)
    }

    pub fn iter(&self) -> impl Iterator<Item = &LockRecord> {
        self.locks.values()
    }

    pub fn len(&self) -> usize {
        self.locks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.locks.is_empty()
    }
}
