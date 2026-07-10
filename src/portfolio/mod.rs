use crate::amount::{Amount, Bps};
use crate::ids::{AccountId, AssetId, Epoch, PoolId, PositionId};
use crate::position::PositionState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AccountExposure {
    pub account: AccountId,
    pub asset: AssetId,
    pub principal: Amount,
    pub collateral: Amount,
    pub pending_interest: Amount,
    pub pending_penalty: Amount,
    pub open_positions: usize,
    pub late_positions: usize,
}

impl AccountExposure {
    pub fn empty(account: AccountId, asset: AssetId) -> Self {
        Self {
            account,
            asset,
            principal: Amount::ZERO,
            collateral: Amount::ZERO,
            pending_interest: Amount::ZERO,
            pending_penalty: Amount::ZERO,
            open_positions: 0,
            late_positions: 0,
        }
    }

    pub fn add_position(
        &mut self,
        principal: Amount,
        collateral: Amount,
        pending_interest: Amount,
        pending_penalty: Amount,
        late: bool,
    ) {
        self.principal += principal;
        self.collateral += collateral;
        self.pending_interest += pending_interest;
        self.pending_penalty += pending_penalty;
        self.open_positions = self.open_positions.saturating_add(1);
        if late {
            self.late_positions = self.late_positions.saturating_add(1);
        }
    }

    pub fn total_claim(self) -> crate::error::ChronosResult<Amount> {
        self.principal
            .checked_add(self.pending_interest)?
            .checked_add(self.pending_penalty)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PoolExposureRollup {
    pub pool: PoolId,
    pub asset: AssetId,
    pub principal: Amount,
    pub collateral: Amount,
    pub pending_interest: Amount,
    pub pending_penalty: Amount,
    pub utilization_bps: Bps,
    pub active_positions: usize,
    pub locked_positions: usize,
    pub expired_positions: usize,
}

impl PoolExposureRollup {
    pub fn empty(pool: PoolId, asset: AssetId, utilization_bps: Bps) -> Self {
        Self {
            pool,
            asset,
            principal: Amount::ZERO,
            collateral: Amount::ZERO,
            pending_interest: Amount::ZERO,
            pending_penalty: Amount::ZERO,
            utilization_bps,
            active_positions: 0,
            locked_positions: 0,
            expired_positions: 0,
        }
    }

    pub fn add_position(
        &mut self,
        state: PositionState,
        principal: Amount,
        collateral: Amount,
        pending_interest: Amount,
        pending_penalty: Amount,
    ) {
        self.principal += principal;
        self.collateral += collateral;
        self.pending_interest += pending_interest;
        self.pending_penalty += pending_penalty;
        match state {
            PositionState::Locked => {
                self.locked_positions = self.locked_positions.saturating_add(1)
            }
            PositionState::Expired => {
                self.expired_positions = self.expired_positions.saturating_add(1)
            }
            PositionState::Closed | PositionState::Cancelled => {}
            _ => self.active_positions = self.active_positions.saturating_add(1),
        }
    }

    pub fn total_pending(self) -> crate::error::ChronosResult<Amount> {
        self.pending_interest.checked_add(self.pending_penalty)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PortfolioView {
    pub position: PositionId,
    pub account: AccountId,
    pub pool: PoolId,
    pub asset: AssetId,
    pub state: PositionState,
    pub opened_epoch: Epoch,
    pub effective_maturity_epoch: Epoch,
    pub principal: Amount,
    pub collateral: Amount,
    pub pending_interest: Amount,
    pub pending_penalty: Amount,
}

impl PortfolioView {
    pub fn is_late_at(self, epoch: Epoch) -> bool {
        epoch > self.effective_maturity_epoch
            && !matches!(self.state, PositionState::Closed | PositionState::Cancelled)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct PortfolioReport {
    pub views: Vec<PortfolioView>,
    pub account_exposures: Vec<AccountExposure>,
    pub pool_rollups: Vec<PoolExposureRollup>,
}

impl PortfolioReport {
    pub fn from_views<I>(views: I, utilization_by_pool: &HashMap<PoolId, Bps>, now: Epoch) -> Self
    where
        I: IntoIterator<Item = PortfolioView>,
    {
        let views = views.into_iter().collect::<Vec<_>>();
        let mut account_map: HashMap<(AccountId, AssetId), AccountExposure> = HashMap::new();
        let mut pool_map: HashMap<(PoolId, AssetId), PoolExposureRollup> = HashMap::new();
        for view in &views {
            if matches!(view.state, PositionState::Closed | PositionState::Cancelled) {
                continue;
            }
            account_map
                .entry((view.account, view.asset))
                .or_insert_with(|| AccountExposure::empty(view.account, view.asset))
                .add_position(
                    view.principal,
                    view.collateral,
                    view.pending_interest,
                    view.pending_penalty,
                    view.is_late_at(now),
                );
            pool_map
                .entry((view.pool, view.asset))
                .or_insert_with(|| {
                    PoolExposureRollup::empty(
                        view.pool,
                        view.asset,
                        utilization_by_pool
                            .get(&view.pool)
                            .copied()
                            .unwrap_or_default(),
                    )
                })
                .add_position(
                    view.state,
                    view.principal,
                    view.collateral,
                    view.pending_interest,
                    view.pending_penalty,
                );
        }
        let mut account_exposures = account_map.into_values().collect::<Vec<_>>();
        account_exposures.sort_by(|left, right| {
            left.account
                .cmp(&right.account)
                .then(left.asset.cmp(&right.asset))
        });
        let mut pool_rollups = pool_map.into_values().collect::<Vec<_>>();
        pool_rollups.sort_by(|left, right| {
            left.pool
                .cmp(&right.pool)
                .then(left.asset.cmp(&right.asset))
        });
        Self {
            views,
            account_exposures,
            pool_rollups,
        }
    }

    pub fn total_principal(&self) -> Amount {
        self.pool_rollups
            .iter()
            .map(|rollup| rollup.principal)
            .sum()
    }

    pub fn late_accounts(&self) -> Vec<AccountId> {
        self.account_exposures
            .iter()
            .filter(|exposure| exposure.late_positions > 0)
            .map(|exposure| exposure.account)
            .collect()
    }
}
