use crate::error::{ChronosError, ChronosResult};
use crate::ids::Epoch;
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EpochPolicy {
    pub genesis_epoch: Epoch,
    pub epoch_seconds: u64,
    pub settlement_cutoff_seconds: u64,
    pub lock_grace_epochs: u64,
    pub max_extension_epochs: u64,
    pub expiry_sweep_delay_epochs: u64,
}

impl Default for EpochPolicy {
    fn default() -> Self {
        Self {
            genesis_epoch: Epoch::ZERO,
            epoch_seconds: 86_400,
            settlement_cutoff_seconds: 3_600,
            lock_grace_epochs: 1,
            max_extension_epochs: 30,
            expiry_sweep_delay_epochs: 2,
        }
    }
}

impl EpochPolicy {
    pub fn validate(&self) -> ChronosResult<()> {
        if self.epoch_seconds == 0 {
            return Err(ChronosError::invalid("epoch duration must be non-zero"));
        }
        if self.settlement_cutoff_seconds >= self.epoch_seconds {
            return Err(ChronosError::invalid("cutoff must be inside epoch"));
        }
        if self.max_extension_epochs == 0 {
            return Err(ChronosError::invalid("extension window must be non-zero"));
        }
        Ok(())
    }

    pub fn extension_deadline(self, from: Epoch) -> ChronosResult<Epoch> {
        from.checked_add(self.max_extension_epochs)
            .ok_or(ChronosError::EpochOutOfRange(from))
    }

    pub fn grace_deadline(self, maturity: Epoch) -> ChronosResult<Epoch> {
        maturity
            .checked_add(self.lock_grace_epochs)
            .ok_or(ChronosError::EpochOutOfRange(maturity))
    }

    pub fn sweep_epoch(self, maturity: Epoch) -> ChronosResult<Epoch> {
        maturity
            .checked_add(self.lock_grace_epochs + self.expiry_sweep_delay_epochs)
            .ok_or(ChronosError::EpochOutOfRange(maturity))
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EpochWindow {
    pub start: Epoch,
    pub end: Epoch,
}

impl EpochWindow {
    pub fn new(start: Epoch, end: Epoch) -> ChronosResult<Self> {
        if end < start {
            return Err(ChronosError::EpochOutOfRange(end));
        }
        Ok(Self { start, end })
    }

    pub fn contains(self, epoch: Epoch) -> bool {
        self.start <= epoch && epoch <= self.end
    }

    pub fn length(self) -> u64 {
        self.start.distance_to(self.end).saturating_add(1)
    }

    pub fn overlaps(self, other: Self) -> bool {
        self.start <= other.end && other.start <= self.end
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum BoundaryMoment {
    BeforeCutoff,
    AtCutoff,
    AfterCutoff,
    EpochStart,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EpochClock {
    policy: EpochPolicy,
    current_epoch: Epoch,
    seconds_into_epoch: u64,
    boundary_count: u64,
}

impl EpochClock {
    pub fn new(policy: EpochPolicy) -> ChronosResult<Self> {
        policy.validate()?;
        Ok(Self {
            current_epoch: policy.genesis_epoch,
            policy,
            seconds_into_epoch: 0,
            boundary_count: 0,
        })
    }

    pub fn policy(&self) -> EpochPolicy {
        self.policy
    }

    pub fn current_epoch(&self) -> Epoch {
        self.current_epoch
    }

    pub fn seconds_into_epoch(&self) -> u64 {
        self.seconds_into_epoch
    }

    pub fn boundary_count(&self) -> u64 {
        self.boundary_count
    }

    pub fn moment(&self) -> BoundaryMoment {
        if self.seconds_into_epoch == 0 {
            BoundaryMoment::EpochStart
        } else if self.seconds_into_epoch < self.policy.settlement_cutoff_seconds {
            BoundaryMoment::BeforeCutoff
        } else if self.seconds_into_epoch == self.policy.settlement_cutoff_seconds {
            BoundaryMoment::AtCutoff
        } else {
            BoundaryMoment::AfterCutoff
        }
    }

    pub fn is_epoch_start(&self) -> bool {
        self.seconds_into_epoch == 0
    }

    pub fn set_seconds_into_epoch(&mut self, seconds: u64) -> ChronosResult<()> {
        if seconds >= self.policy.epoch_seconds {
            return Err(ChronosError::invalid("seconds exceed epoch length"));
        }
        self.seconds_into_epoch = seconds;
        Ok(())
    }

    pub fn advance_seconds(&mut self, seconds: u64) -> ChronosResult<Vec<Epoch>> {
        let mut crossed = Vec::new();
        let mut remaining = seconds;
        while remaining > 0 {
            let until_next = self.policy.epoch_seconds - self.seconds_into_epoch;
            if remaining < until_next {
                self.seconds_into_epoch += remaining;
                remaining = 0;
            } else {
                remaining -= until_next;
                self.current_epoch = self
                    .current_epoch
                    .checked_add(1)
                    .ok_or(ChronosError::EpochOutOfRange(self.current_epoch))?;
                self.seconds_into_epoch = 0;
                self.boundary_count = self.boundary_count.saturating_add(1);
                crossed.push(self.current_epoch);
            }
        }
        Ok(crossed)
    }

    pub fn advance_epochs(&mut self, epochs: u64) -> ChronosResult<Vec<Epoch>> {
        let mut crossed = Vec::with_capacity(epochs as usize);
        for _ in 0..epochs {
            self.current_epoch = self
                .current_epoch
                .checked_add(1)
                .ok_or(ChronosError::EpochOutOfRange(self.current_epoch))?;
            self.seconds_into_epoch = 0;
            self.boundary_count = self.boundary_count.saturating_add(1);
            crossed.push(self.current_epoch);
        }
        Ok(crossed)
    }

    pub fn window_until(&self, end: Epoch) -> ChronosResult<EpochWindow> {
        EpochWindow::new(self.current_epoch, end)
    }
}
