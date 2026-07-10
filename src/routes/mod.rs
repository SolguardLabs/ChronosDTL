use crate::amount::{Amount, Bps};
use crate::error::{ChronosError, ChronosResult};
use crate::ids::{AccountId, AssetId, Epoch, OperatorId, PoolId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RouteStatus {
    Active,
    DrainOnly,
    Paused,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SettlementLane {
    pub source_pool: PoolId,
    pub target_pool: PoolId,
    pub asset: AssetId,
    pub operator: OperatorId,
    pub fee_recipient: AccountId,
    pub max_notional: Amount,
    pub fee_bps: Bps,
    pub status: RouteStatus,
    pub valid_from: Epoch,
    pub valid_until: Epoch,
}

impl SettlementLane {
    pub fn accepts(
        self,
        source: PoolId,
        target: PoolId,
        asset: AssetId,
        epoch: Epoch,
        amount: Amount,
    ) -> bool {
        self.status == RouteStatus::Active
            && self.source_pool == source
            && self.target_pool == target
            && self.asset == asset
            && self.valid_from <= epoch
            && epoch <= self.valid_until
            && amount <= self.max_notional
    }

    pub fn fee(self, amount: Amount) -> ChronosResult<Amount> {
        amount.mul_bps(self.fee_bps)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RouteQuote {
    pub lane: SettlementLane,
    pub gross_amount: Amount,
    pub route_fee: Amount,
    pub net_amount: Amount,
    pub quoted_epoch: Epoch,
}

impl RouteQuote {
    pub fn minimum_received(self, slippage_bps: Bps) -> ChronosResult<Amount> {
        let slippage = self.net_amount.mul_bps(slippage_bps)?;
        self.net_amount.checked_sub(slippage)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RouteBook {
    lanes: HashMap<(PoolId, PoolId, AssetId), Vec<SettlementLane>>,
}

impl RouteBook {
    pub fn add_lane(&mut self, lane: SettlementLane) {
        self.lanes
            .entry((lane.source_pool, lane.target_pool, lane.asset))
            .or_default()
            .push(lane);
    }

    pub fn lanes_for(
        &self,
        source: PoolId,
        target: PoolId,
        asset: AssetId,
    ) -> impl Iterator<Item = &SettlementLane> {
        self.lanes
            .get(&(source, target, asset))
            .into_iter()
            .flat_map(|lanes| lanes.iter())
    }

    pub fn quote(
        &self,
        source: PoolId,
        target: PoolId,
        asset: AssetId,
        epoch: Epoch,
        amount: Amount,
    ) -> ChronosResult<RouteQuote> {
        let lane = self
            .lanes_for(source, target, asset)
            .copied()
            .filter(|lane| lane.accepts(source, target, asset, epoch, amount))
            .min_by(|left, right| left.fee_bps.cmp(&right.fee_bps))
            .ok_or_else(|| ChronosError::risk("no settlement lane available"))?;
        let route_fee = lane.fee(amount)?;
        let net_amount = amount.checked_sub(route_fee)?;
        Ok(RouteQuote {
            lane,
            gross_amount: amount,
            route_fee,
            net_amount,
            quoted_epoch: epoch,
        })
    }

    pub fn pause_lane(
        &mut self,
        source: PoolId,
        target: PoolId,
        asset: AssetId,
        operator: OperatorId,
    ) -> ChronosResult<()> {
        let lanes = self
            .lanes
            .get_mut(&(source, target, asset))
            .ok_or_else(|| ChronosError::invalid("route not found"))?;
        let lane = lanes
            .iter_mut()
            .find(|lane| lane.operator == operator)
            .ok_or_else(|| ChronosError::invalid("lane not found"))?;
        lane.status = RouteStatus::Paused;
        Ok(())
    }

    pub fn active_lane_count(&self) -> usize {
        self.lanes
            .values()
            .flat_map(|lanes| lanes.iter())
            .filter(|lane| lane.status == RouteStatus::Active)
            .count()
    }
}
