use crate::amount::{AccrualIndex, Amount};
use crate::error::{ChronosError, ChronosResult};
use crate::ids::{AccountId, AssetId, Epoch, LockId, PoolId, PositionId};
use crate::rates::AccrualState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PositionState {
    Active,
    Locked,
    InGrace,
    Matured,
    Expired,
    Closed,
    Cancelled,
}

impl PositionState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Closed | Self::Cancelled)
    }

    pub fn admits_close(self) -> bool {
        matches!(
            self,
            Self::Active | Self::Locked | Self::InGrace | Self::Matured | Self::Expired
        )
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AccrualCheckpoint {
    pub epoch: Epoch,
    pub interest_index: AccrualIndex,
    pub penalty_index: AccrualIndex,
}

impl AccrualCheckpoint {
    pub fn new(epoch: Epoch, interest_index: AccrualIndex, penalty_index: AccrualIndex) -> Self {
        Self {
            epoch,
            interest_index,
            penalty_index,
        }
    }

    pub fn from_state(state: AccrualState) -> Self {
        Self::new(state.epoch, state.interest_index, state.penalty_index)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PositionTerms {
    pub principal: Amount,
    pub collateral: Amount,
    pub maturity_epoch: Epoch,
    pub min_close_amount: Amount,
    pub max_close_fee_bps: crate::amount::Bps,
}

impl PositionTerms {
    pub fn validate(self, opened_epoch: Epoch) -> ChronosResult<()> {
        self.principal.non_zero()?;
        self.collateral.non_zero()?;
        if self.maturity_epoch <= opened_epoch {
            return Err(ChronosError::invalid("maturity must be after open epoch"));
        }
        if self.min_close_amount > self.principal {
            return Err(ChronosError::invalid(
                "minimum close amount exceeds principal",
            ));
        }
        Ok(())
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StateTransition {
    pub from: PositionState,
    pub to: PositionState,
    pub epoch: Epoch,
    pub effective_maturity_epoch: Epoch,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PositionRecord {
    pub id: PositionId,
    pub borrower: AccountId,
    pub pool: PoolId,
    pub asset: AssetId,
    pub terms: PositionTerms,
    pub opened_epoch: Epoch,
    pub effective_maturity_epoch: Epoch,
    pub checkpoint: AccrualCheckpoint,
    pub pending_interest: Amount,
    pub pending_penalty: Amount,
    pub state: PositionState,
    pub state_version: u64,
    pub active_lock: Option<LockId>,
    pub transitions: Vec<StateTransition>,
}

impl PositionRecord {
    pub fn new(
        id: PositionId,
        borrower: AccountId,
        pool: PoolId,
        asset: AssetId,
        opened_epoch: Epoch,
        terms: PositionTerms,
        checkpoint: AccrualCheckpoint,
    ) -> ChronosResult<Self> {
        terms.validate(opened_epoch)?;
        Ok(Self {
            id,
            borrower,
            pool,
            asset,
            terms,
            opened_epoch,
            effective_maturity_epoch: terms.maturity_epoch,
            checkpoint,
            pending_interest: Amount::ZERO,
            pending_penalty: Amount::ZERO,
            state: PositionState::Active,
            state_version: 0,
            active_lock: None,
            transitions: Vec::new(),
        })
    }

    pub fn principal(&self) -> Amount {
        self.terms.principal
    }

    pub fn collateral(&self) -> Amount {
        self.terms.collateral
    }

    pub fn is_open(&self) -> bool {
        !self.state.is_terminal()
    }

    pub fn is_due_at(&self, epoch: Epoch) -> bool {
        epoch >= self.effective_maturity_epoch
    }

    pub fn is_late_at(&self, epoch: Epoch) -> bool {
        epoch > self.effective_maturity_epoch
    }

    pub fn transition_to(&mut self, state: PositionState, epoch: Epoch) {
        let from = self.state;
        self.state = state;
        self.state_version = self.state_version.saturating_add(1);
        self.transitions.push(StateTransition {
            from,
            to: state,
            epoch,
            effective_maturity_epoch: self.effective_maturity_epoch,
        });
    }

    pub fn add_pending_interest(&mut self, amount: Amount) -> ChronosResult<()> {
        self.pending_interest = self.pending_interest.checked_add(amount)?;
        Ok(())
    }

    pub fn add_pending_penalty(&mut self, amount: Amount) -> ChronosResult<()> {
        self.pending_penalty = self.pending_penalty.checked_add(amount)?;
        Ok(())
    }

    pub fn materialize(
        &mut self,
        interest: Amount,
        penalty: Amount,
        checkpoint: AccrualCheckpoint,
    ) -> ChronosResult<()> {
        self.add_pending_interest(interest)?;
        self.add_pending_penalty(penalty)?;
        self.checkpoint = checkpoint;
        Ok(())
    }

    pub fn attach_lock(
        &mut self,
        lock_id: LockId,
        release_epoch: Epoch,
        checkpoint: AccrualCheckpoint,
        epoch: Epoch,
    ) {
        self.active_lock = Some(lock_id);
        self.effective_maturity_epoch = release_epoch;
        self.checkpoint = checkpoint;
        self.transition_to(PositionState::Locked, epoch);
    }

    pub fn release_lock(&mut self, epoch: Epoch) {
        self.active_lock = None;
        if self.state == PositionState::Locked {
            self.transition_to(PositionState::Active, epoch);
        }
    }

    pub fn close(&mut self, epoch: Epoch) {
        self.active_lock = None;
        self.transition_to(PositionState::Closed, epoch);
    }

    pub fn expire(&mut self, epoch: Epoch) {
        self.active_lock = None;
        self.transition_to(PositionState::Expired, epoch);
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PositionBook {
    positions: HashMap<PositionId, PositionRecord>,
}

impl PositionBook {
    pub fn insert(&mut self, position: PositionRecord) -> ChronosResult<()> {
        if self.positions.contains_key(&position.id) {
            return Err(ChronosError::invalid(format!(
                "position {} already exists",
                position.id
            )));
        }
        self.positions.insert(position.id, position);
        Ok(())
    }

    pub fn get(&self, id: PositionId) -> ChronosResult<&PositionRecord> {
        self.positions
            .get(&id)
            .ok_or(ChronosError::UnknownPosition(id))
    }

    pub fn get_mut(&mut self, id: PositionId) -> ChronosResult<&mut PositionRecord> {
        self.positions
            .get_mut(&id)
            .ok_or(ChronosError::UnknownPosition(id))
    }

    pub fn iter(&self) -> impl Iterator<Item = &PositionRecord> {
        self.positions.values()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut PositionRecord> {
        self.positions.values_mut()
    }

    pub fn open_positions_for(&self, account: AccountId) -> Vec<PositionId> {
        self.positions
            .values()
            .filter(|position| position.borrower == account && position.is_open())
            .map(|position| position.id)
            .collect()
    }

    pub fn len(&self) -> usize {
        self.positions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }
}
