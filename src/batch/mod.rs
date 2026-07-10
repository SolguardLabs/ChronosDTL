use crate::amount::Amount;
use crate::error::{ChronosError, ChronosResult};
use crate::ids::{AccountId, BatchId, Epoch, PoolId, PositionId, TxId};
use crate::locks::LockMode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum BatchStatus {
    Draft,
    Sealed,
    Executing,
    Completed,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum BatchInstruction {
    DepositLiquidity {
        provider: AccountId,
        pool: PoolId,
        amount: Amount,
    },
    ClosePosition {
        payer: AccountId,
        position: PositionId,
        max_total_due: Amount,
    },
    LockPosition {
        owner: AccountId,
        position: PositionId,
        release_epoch: Epoch,
        mode: LockMode,
    },
    ExpirePosition {
        position: PositionId,
    },
}

impl BatchInstruction {
    pub fn pool_hint(&self) -> Option<PoolId> {
        match self {
            Self::DepositLiquidity { pool, .. } => Some(*pool),
            Self::ClosePosition { .. }
            | Self::LockPosition { .. }
            | Self::ExpirePosition { .. } => None,
        }
    }

    pub fn position_hint(&self) -> Option<PositionId> {
        match self {
            Self::ClosePosition { position, .. }
            | Self::LockPosition { position, .. }
            | Self::ExpirePosition { position } => Some(*position),
            Self::DepositLiquidity { .. } => None,
        }
    }

    pub fn notional_hint(&self) -> Amount {
        match self {
            Self::DepositLiquidity { amount, .. } => *amount,
            Self::ClosePosition { max_total_due, .. } => *max_total_due,
            Self::LockPosition { .. } | Self::ExpirePosition { .. } => Amount::ZERO,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BatchManifest {
    pub id: BatchId,
    pub owner: AccountId,
    pub created_epoch: Epoch,
    pub sealed_epoch: Option<Epoch>,
    pub status: BatchStatus,
    pub instructions: Vec<BatchInstruction>,
    pub executed_txs: Vec<TxId>,
    pub memo: String,
}

impl BatchManifest {
    pub fn new(id: BatchId, owner: AccountId, created_epoch: Epoch) -> Self {
        Self {
            id,
            owner,
            created_epoch,
            sealed_epoch: None,
            status: BatchStatus::Draft,
            instructions: Vec::new(),
            executed_txs: Vec::new(),
            memo: String::new(),
        }
    }

    pub fn push(&mut self, instruction: BatchInstruction) -> ChronosResult<()> {
        if self.status != BatchStatus::Draft {
            return Err(ChronosError::invalid("batch is not editable"));
        }
        self.instructions.push(instruction);
        Ok(())
    }

    pub fn seal(&mut self, epoch: Epoch) -> ChronosResult<()> {
        if self.status != BatchStatus::Draft {
            return Err(ChronosError::invalid("batch cannot be sealed"));
        }
        if self.instructions.is_empty() {
            return Err(ChronosError::invalid("empty batch"));
        }
        self.status = BatchStatus::Sealed;
        self.sealed_epoch = Some(epoch);
        Ok(())
    }

    pub fn begin_execution(&mut self) -> ChronosResult<()> {
        if self.status != BatchStatus::Sealed {
            return Err(ChronosError::invalid("batch is not sealed"));
        }
        self.status = BatchStatus::Executing;
        Ok(())
    }

    pub fn record_tx(&mut self, tx: TxId) -> ChronosResult<()> {
        if self.status != BatchStatus::Executing {
            return Err(ChronosError::invalid("batch is not executing"));
        }
        self.executed_txs.push(tx);
        Ok(())
    }

    pub fn complete(&mut self) -> ChronosResult<()> {
        if self.status != BatchStatus::Executing {
            return Err(ChronosError::invalid("batch is not executing"));
        }
        if self.executed_txs.len() > self.instructions.len() {
            return Err(ChronosError::invariant(
                "batch has more txs than instructions",
            ));
        }
        self.status = BatchStatus::Completed;
        Ok(())
    }

    pub fn cancel(&mut self) -> ChronosResult<()> {
        if matches!(self.status, BatchStatus::Completed | BatchStatus::Cancelled) {
            return Err(ChronosError::invalid("batch is terminal"));
        }
        self.status = BatchStatus::Cancelled;
        Ok(())
    }

    pub fn total_notional_hint(&self) -> Amount {
        self.instructions
            .iter()
            .map(BatchInstruction::notional_hint)
            .sum()
    }

    pub fn summary(&self) -> BatchSummary {
        BatchSummary {
            id: self.id,
            owner: self.owner,
            status: self.status,
            instruction_count: self.instructions.len(),
            executed_count: self.executed_txs.len(),
            total_notional_hint: self.total_notional_hint(),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BatchSummary {
    pub id: BatchId,
    pub owner: AccountId,
    pub status: BatchStatus,
    pub instruction_count: usize,
    pub executed_count: usize,
    pub total_notional_hint: Amount,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BatchBook {
    batches: HashMap<BatchId, BatchManifest>,
}

impl BatchBook {
    pub fn insert(&mut self, manifest: BatchManifest) -> ChronosResult<()> {
        if self.batches.contains_key(&manifest.id) {
            return Err(ChronosError::invalid(format!(
                "batch {} already exists",
                manifest.id
            )));
        }
        self.batches.insert(manifest.id, manifest);
        Ok(())
    }

    pub fn get(&self, id: BatchId) -> ChronosResult<&BatchManifest> {
        self.batches
            .get(&id)
            .ok_or_else(|| ChronosError::invalid(format!("unknown batch {id}")))
    }

    pub fn get_mut(&mut self, id: BatchId) -> ChronosResult<&mut BatchManifest> {
        self.batches
            .get_mut(&id)
            .ok_or_else(|| ChronosError::invalid(format!("unknown batch {id}")))
    }

    pub fn open_for_owner(&self, owner: AccountId) -> Vec<&BatchManifest> {
        self.batches
            .values()
            .filter(|batch| {
                batch.owner == owner
                    && matches!(
                        batch.status,
                        BatchStatus::Draft | BatchStatus::Sealed | BatchStatus::Executing
                    )
            })
            .collect()
    }

    pub fn summaries(&self) -> Vec<BatchSummary> {
        let mut summaries = self
            .batches
            .values()
            .map(BatchManifest::summary)
            .collect::<Vec<_>>();
        summaries.sort_by_key(|summary| summary.id);
        summaries
    }
}
