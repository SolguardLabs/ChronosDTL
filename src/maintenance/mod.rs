use crate::amount::Amount;
use crate::error::{ChronosError, ChronosResult};
use crate::ids::{AccountId, Epoch, PoolId, PositionId};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum MaintenanceStatus {
    Queued,
    Running,
    Completed,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum MaintenanceAction {
    RecomputeAccrual {
        pool: PoolId,
        from_epoch: Epoch,
        to_epoch: Epoch,
    },
    ReviewPosition {
        position: PositionId,
        reviewer: AccountId,
    },
    SweepDust {
        pool: PoolId,
        max_amount: Amount,
    },
    ReconcilePool {
        pool: PoolId,
    },
}

impl MaintenanceAction {
    pub fn subject(&self) -> String {
        match self {
            Self::RecomputeAccrual { pool, .. } => pool.to_string(),
            Self::ReviewPosition { position, .. } => position.to_string(),
            Self::SweepDust { pool, .. } => pool.to_string(),
            Self::ReconcilePool { pool } => pool.to_string(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MaintenancePlan {
    pub action: MaintenanceAction,
    pub requested_epoch: Epoch,
    pub scheduled_epoch: Epoch,
    pub status: MaintenanceStatus,
    pub attempts: u32,
    pub memo: String,
}

impl MaintenancePlan {
    pub fn new(action: MaintenanceAction, requested_epoch: Epoch, scheduled_epoch: Epoch) -> Self {
        Self {
            action,
            requested_epoch,
            scheduled_epoch,
            status: MaintenanceStatus::Queued,
            attempts: 0,
            memo: String::new(),
        }
    }

    pub fn ready_at(&self, epoch: Epoch) -> bool {
        self.status == MaintenanceStatus::Queued && epoch >= self.scheduled_epoch
    }

    pub fn begin(&mut self) -> ChronosResult<()> {
        if self.status != MaintenanceStatus::Queued {
            return Err(ChronosError::invalid("maintenance plan is not queued"));
        }
        self.status = MaintenanceStatus::Running;
        self.attempts = self.attempts.saturating_add(1);
        Ok(())
    }

    pub fn complete(&mut self) -> ChronosResult<()> {
        if self.status != MaintenanceStatus::Running {
            return Err(ChronosError::invalid("maintenance plan is not running"));
        }
        self.status = MaintenanceStatus::Completed;
        Ok(())
    }

    pub fn cancel(&mut self) -> ChronosResult<()> {
        if matches!(
            self.status,
            MaintenanceStatus::Completed | MaintenanceStatus::Cancelled
        ) {
            return Err(ChronosError::invalid("maintenance plan is terminal"));
        }
        self.status = MaintenanceStatus::Cancelled;
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MaintenanceQueue {
    queue: VecDeque<MaintenancePlan>,
    completed: Vec<MaintenancePlan>,
}

impl MaintenanceQueue {
    pub fn push(&mut self, plan: MaintenancePlan) {
        let insert_at = self
            .queue
            .iter()
            .position(|existing| existing.scheduled_epoch > plan.scheduled_epoch)
            .unwrap_or(self.queue.len());
        self.queue.insert(insert_at, plan);
    }

    pub fn next_ready(&mut self, epoch: Epoch) -> Option<MaintenancePlan> {
        let idx = self.queue.iter().position(|plan| plan.ready_at(epoch))?;
        self.queue.remove(idx)
    }

    pub fn finish(&mut self, mut plan: MaintenancePlan) -> ChronosResult<()> {
        if plan.status == MaintenanceStatus::Running {
            plan.complete()?;
        }
        self.completed.push(plan);
        Ok(())
    }

    pub fn cancel_subject(&mut self, subject: &str) -> ChronosResult<usize> {
        let mut cancelled = 0usize;
        for plan in &mut self.queue {
            if plan.action.subject() == subject && plan.status == MaintenanceStatus::Queued {
                plan.cancel()?;
                cancelled = cancelled.saturating_add(1);
            }
        }
        Ok(cancelled)
    }

    pub fn queued_len(&self) -> usize {
        self.queue.len()
    }

    pub fn completed_len(&self) -> usize {
        self.completed.len()
    }
}
