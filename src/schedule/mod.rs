use crate::amount::Amount;
use crate::error::{ChronosError, ChronosResult};
use crate::ids::{Epoch, PositionId};
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ScheduleStatus {
    Planned,
    Due,
    Paid,
    Skipped,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ScheduleLine {
    pub position: PositionId,
    pub due_epoch: Epoch,
    pub principal_due: Amount,
    pub expected_interest: Amount,
    pub expected_penalty: Amount,
    pub status: ScheduleStatus,
}

impl ScheduleLine {
    pub fn total_expected(self) -> ChronosResult<Amount> {
        self.principal_due
            .checked_add(self.expected_interest)?
            .checked_add(self.expected_penalty)
    }

    pub fn mark_due(mut self) -> Self {
        if self.status == ScheduleStatus::Planned {
            self.status = ScheduleStatus::Due;
        }
        self
    }

    pub fn mark_paid(mut self) -> Self {
        self.status = ScheduleStatus::Paid;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RepaymentSchedule {
    pub position: PositionId,
    pub lines: Vec<ScheduleLine>,
}

impl RepaymentSchedule {
    pub fn new(position: PositionId) -> Self {
        Self {
            position,
            lines: Vec::new(),
        }
    }

    pub fn push(&mut self, line: ScheduleLine) -> ChronosResult<()> {
        if line.position != self.position {
            return Err(ChronosError::invalid(
                "schedule line belongs to another position",
            ));
        }
        if self
            .lines
            .iter()
            .any(|existing| existing.due_epoch == line.due_epoch)
        {
            return Err(ChronosError::invalid("duplicate schedule epoch"));
        }
        self.lines.push(line);
        self.lines.sort_by_key(|line| line.due_epoch);
        Ok(())
    }

    pub fn due_at(&self, epoch: Epoch) -> Vec<ScheduleLine> {
        self.lines
            .iter()
            .copied()
            .filter(|line| line.due_epoch <= epoch && line.status != ScheduleStatus::Paid)
            .map(ScheduleLine::mark_due)
            .collect()
    }

    pub fn mark_paid(&mut self, epoch: Epoch) -> ChronosResult<()> {
        let line = self
            .lines
            .iter_mut()
            .find(|line| line.due_epoch == epoch)
            .ok_or_else(|| ChronosError::invalid("schedule line not found"))?;
        line.status = ScheduleStatus::Paid;
        Ok(())
    }

    pub fn remaining_principal(&self) -> Amount {
        self.lines
            .iter()
            .filter(|line| line.status != ScheduleStatus::Paid)
            .map(|line| line.principal_due)
            .sum()
    }

    pub fn expected_total(&self) -> ChronosResult<Amount> {
        self.lines.iter().try_fold(Amount::ZERO, |acc, line| {
            acc.checked_add(line.total_expected()?)
        })
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ScheduleBuilder {
    pub position: PositionId,
    pub start_epoch: Epoch,
    pub maturity_epoch: Epoch,
    pub principal: Amount,
    pub expected_interest: Amount,
}

impl ScheduleBuilder {
    pub fn new(
        position: PositionId,
        start_epoch: Epoch,
        maturity_epoch: Epoch,
        principal: Amount,
        expected_interest: Amount,
    ) -> Self {
        Self {
            position,
            start_epoch,
            maturity_epoch,
            principal,
            expected_interest,
        }
    }

    pub fn bullet(self) -> ChronosResult<RepaymentSchedule> {
        let mut schedule = RepaymentSchedule::new(self.position);
        schedule.push(ScheduleLine {
            position: self.position,
            due_epoch: self.maturity_epoch,
            principal_due: self.principal,
            expected_interest: self.expected_interest,
            expected_penalty: Amount::ZERO,
            status: ScheduleStatus::Planned,
        })?;
        Ok(schedule)
    }

    pub fn equal_principal(self, installments: u64) -> ChronosResult<RepaymentSchedule> {
        if installments == 0 {
            return Err(ChronosError::invalid("installments must be non-zero"));
        }
        let span = self.start_epoch.distance_to(self.maturity_epoch);
        if span < installments {
            return Err(ChronosError::invalid("not enough epochs for installments"));
        }
        let principal_each = Amount::new(self.principal.raw() / u128::from(installments));
        let interest_each = Amount::new(self.expected_interest.raw() / u128::from(installments));
        let mut schedule = RepaymentSchedule::new(self.position);
        for idx in 1..=installments {
            let due_epoch = self.start_epoch.saturating_add(span * idx / installments);
            let principal_due = if idx == installments {
                self.principal
                    .saturating_sub(principal_each.checked_mul(u128::from(installments - 1))?)
            } else {
                principal_each
            };
            schedule.push(ScheduleLine {
                position: self.position,
                due_epoch,
                principal_due,
                expected_interest: interest_each,
                expected_penalty: Amount::ZERO,
                status: ScheduleStatus::Planned,
            })?;
        }
        Ok(schedule)
    }
}
