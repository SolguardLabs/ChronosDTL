use crate::amount::{AccrualIndex, Bps};
use crate::error::{ChronosError, ChronosResult};
use crate::ids::{Epoch, PoolId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CompoundingMode {
    Linear,
    EpochCompound,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RateModel {
    pub base_bps: Bps,
    pub utilization_slope_bps: Bps,
    pub penalty_bps: Bps,
    pub max_bps: Bps,
    pub compounding: CompoundingMode,
}

impl Default for RateModel {
    fn default() -> Self {
        Self {
            base_bps: Bps::from_raw_unchecked(30),
            utilization_slope_bps: Bps::from_raw_unchecked(220),
            penalty_bps: Bps::from_raw_unchecked(75),
            max_bps: Bps::from_raw_unchecked(2_500),
            compounding: CompoundingMode::EpochCompound,
        }
    }
}

impl RateModel {
    pub fn quote_bps(self, utilization: Bps) -> ChronosResult<Bps> {
        let utilization_component = self
            .utilization_slope_bps
            .raw()
            .saturating_mul(utilization.raw())
            / 10_000;
        let raw = self.base_bps.raw().saturating_add(utilization_component);
        Bps::new(raw).map(|bps| bps.clamp(self.max_bps))
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AccrualState {
    pub pool: PoolId,
    pub epoch: Epoch,
    pub interest_index: AccrualIndex,
    pub penalty_index: AccrualIndex,
    pub utilization_bps: Bps,
    pub rate_bps: Bps,
    pub penalty_bps: Bps,
}

impl AccrualState {
    pub fn initial(pool: PoolId, epoch: Epoch, model: RateModel) -> Self {
        Self {
            pool,
            epoch,
            interest_index: AccrualIndex::one(),
            penalty_index: AccrualIndex::one(),
            utilization_bps: Bps::ZERO,
            rate_bps: model.base_bps,
            penalty_bps: model.penalty_bps,
        }
    }

    pub fn advance_one(self, utilization: Bps, model: RateModel) -> ChronosResult<Self> {
        let rate_bps = model.quote_bps(utilization)?;
        let interest_index = match model.compounding {
            CompoundingMode::Linear | CompoundingMode::EpochCompound => {
                self.interest_index.compound_bps(rate_bps)?
            }
        };
        let penalty_index = self.penalty_index.compound_bps(model.penalty_bps)?;
        Ok(Self {
            pool: self.pool,
            epoch: self.epoch.next(),
            interest_index,
            penalty_index,
            utilization_bps: utilization,
            rate_bps,
            penalty_bps: model.penalty_bps,
        })
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AccrualSample {
    pub epoch: Epoch,
    pub interest_index: AccrualIndex,
    pub penalty_index: AccrualIndex,
    pub utilization_bps: Bps,
    pub rate_bps: Bps,
}

impl From<AccrualState> for AccrualSample {
    fn from(value: AccrualState) -> Self {
        Self {
            epoch: value.epoch,
            interest_index: value.interest_index,
            penalty_index: value.penalty_index,
            utilization_bps: value.utilization_bps,
            rate_bps: value.rate_bps,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RateTrack {
    pub model: RateModel,
    pub state: AccrualState,
    pub history: Vec<AccrualSample>,
}

impl RateTrack {
    pub fn new(pool: PoolId, epoch: Epoch, model: RateModel) -> Self {
        let state = AccrualState::initial(pool, epoch, model);
        Self {
            model,
            state,
            history: vec![state.into()],
        }
    }

    pub fn advance_to(
        &mut self,
        epoch: Epoch,
        utilization: Bps,
    ) -> ChronosResult<Vec<AccrualSample>> {
        let mut produced = Vec::new();
        while self.state.epoch < epoch {
            self.state = self.state.advance_one(utilization, self.model)?;
            let sample = AccrualSample::from(self.state);
            self.history.push(sample);
            produced.push(sample);
        }
        Ok(produced)
    }

    pub fn sample_at_or_before(&self, epoch: Epoch) -> AccrualSample {
        self.history
            .iter()
            .rev()
            .copied()
            .find(|sample| sample.epoch <= epoch)
            .unwrap_or_else(|| AccrualSample::from(self.state))
    }

    pub fn current(&self) -> AccrualState {
        self.state
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RateBook {
    tracks: HashMap<PoolId, RateTrack>,
}

impl RateBook {
    pub fn insert(&mut self, pool: PoolId, epoch: Epoch, model: RateModel) -> ChronosResult<()> {
        if self.tracks.contains_key(&pool) {
            return Err(ChronosError::invalid(format!(
                "rate track for {pool} already exists"
            )));
        }
        self.tracks.insert(pool, RateTrack::new(pool, epoch, model));
        Ok(())
    }

    pub fn get(&self, pool: PoolId) -> ChronosResult<&RateTrack> {
        self.tracks
            .get(&pool)
            .ok_or(ChronosError::UnknownPool(pool))
    }

    pub fn get_mut(&mut self, pool: PoolId) -> ChronosResult<&mut RateTrack> {
        self.tracks
            .get_mut(&pool)
            .ok_or(ChronosError::UnknownPool(pool))
    }

    pub fn current(&self, pool: PoolId) -> ChronosResult<AccrualState> {
        Ok(self.get(pool)?.current())
    }

    pub fn advance_pool(
        &mut self,
        pool: PoolId,
        epoch: Epoch,
        utilization: Bps,
    ) -> ChronosResult<Vec<AccrualSample>> {
        self.get_mut(pool)?.advance_to(epoch, utilization)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&PoolId, &RateTrack)> {
        self.tracks.iter()
    }
}
