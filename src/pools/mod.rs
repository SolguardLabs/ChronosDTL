use crate::amount::{Amount, BPS_DENOMINATOR, Bps};
use crate::error::{ChronosError, ChronosResult};
use crate::ids::{AccountId, AssetId, PoolId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PoolStatus {
    Open,
    DepositsOnly,
    RepayOnly,
    Paused,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PoolConfig {
    pub id: PoolId,
    pub asset: AssetId,
    pub name: String,
    pub controller: AccountId,
    pub reserve_factor_bps: Bps,
    pub max_utilization_bps: Bps,
    pub min_liquidity: Amount,
}

impl PoolConfig {
    pub fn new(id: PoolId, asset: AssetId, controller: AccountId, name: impl Into<String>) -> Self {
        Self {
            id,
            asset,
            name: name.into(),
            controller,
            reserve_factor_bps: Bps::from_raw_unchecked(1_000),
            max_utilization_bps: Bps::from_raw_unchecked(9_000),
            min_liquidity: Amount::new(1),
        }
    }

    pub fn with_reserve_factor(mut self, bps: Bps) -> Self {
        self.reserve_factor_bps = bps;
        self
    }

    pub fn with_max_utilization(mut self, bps: Bps) -> Self {
        self.max_utilization_bps = bps;
        self
    }

    pub fn with_min_liquidity(mut self, amount: Amount) -> Self {
        self.min_liquidity = amount;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PoolState {
    pub config: PoolConfig,
    pub status: PoolStatus,
    pub liquidity_available: Amount,
    pub principal_outstanding: Amount,
    pub collateral_locked: Amount,
    pub interest_collected: Amount,
    pub penalty_collected: Amount,
    pub reserve_balance: Amount,
    pub lifetime_deposits: Amount,
    pub lifetime_withdrawals: Amount,
}

impl PoolState {
    pub fn new(config: PoolConfig) -> Self {
        Self {
            config,
            status: PoolStatus::Open,
            liquidity_available: Amount::ZERO,
            principal_outstanding: Amount::ZERO,
            collateral_locked: Amount::ZERO,
            interest_collected: Amount::ZERO,
            penalty_collected: Amount::ZERO,
            reserve_balance: Amount::ZERO,
            lifetime_deposits: Amount::ZERO,
            lifetime_withdrawals: Amount::ZERO,
        }
    }

    pub fn asset(&self) -> AssetId {
        self.config.asset
    }

    pub fn can_borrow(&self) -> bool {
        matches!(self.status, PoolStatus::Open)
    }

    pub fn can_deposit(&self) -> bool {
        matches!(self.status, PoolStatus::Open | PoolStatus::DepositsOnly)
    }

    pub fn can_repay(&self) -> bool {
        !matches!(self.status, PoolStatus::Paused)
    }

    pub fn total_assets(&self) -> ChronosResult<Amount> {
        self.liquidity_available
            .checked_add(self.principal_outstanding)?
            .checked_add(self.reserve_balance)
    }

    pub fn utilization_bps(&self) -> ChronosResult<Bps> {
        let total = self
            .liquidity_available
            .checked_add(self.principal_outstanding)?;
        if total.is_zero() {
            return Ok(Bps::ZERO);
        }
        let raw = self
            .principal_outstanding
            .raw()
            .checked_mul(BPS_DENOMINATOR)
            .and_then(|value| value.checked_div(total.raw()))
            .ok_or(ChronosError::AmountOverflow)?;
        Bps::new(raw.min(u128::from(u32::MAX)) as u32)
    }

    pub fn deposit_liquidity(&mut self, amount: Amount) -> ChronosResult<()> {
        amount.non_zero()?;
        if !self.can_deposit() {
            return Err(ChronosError::PoolUnavailable(self.config.id));
        }
        self.liquidity_available = self.liquidity_available.checked_add(amount)?;
        self.lifetime_deposits = self.lifetime_deposits.checked_add(amount)?;
        Ok(())
    }

    pub fn withdraw_liquidity(&mut self, amount: Amount) -> ChronosResult<()> {
        amount.non_zero()?;
        if self.liquidity_available < amount {
            return Err(ChronosError::InsufficientLiquidity {
                pool: self.config.id,
            });
        }
        let after = self.liquidity_available.checked_sub(amount)?;
        if after < self.config.min_liquidity && !self.principal_outstanding.is_zero() {
            return Err(ChronosError::risk(
                "pool minimum liquidity would be breached",
            ));
        }
        self.liquidity_available = after;
        self.lifetime_withdrawals = self.lifetime_withdrawals.checked_add(amount)?;
        Ok(())
    }

    pub fn borrow(&mut self, principal: Amount, collateral: Amount) -> ChronosResult<()> {
        principal.non_zero()?;
        if !self.can_borrow() {
            return Err(ChronosError::PoolUnavailable(self.config.id));
        }
        if self.liquidity_available < principal {
            return Err(ChronosError::InsufficientLiquidity {
                pool: self.config.id,
            });
        }
        self.liquidity_available = self.liquidity_available.checked_sub(principal)?;
        self.principal_outstanding = self.principal_outstanding.checked_add(principal)?;
        self.collateral_locked = self.collateral_locked.checked_add(collateral)?;
        Ok(())
    }

    pub fn repay(
        &mut self,
        principal: Amount,
        interest: Amount,
        penalty: Amount,
        collateral: Amount,
    ) -> ChronosResult<Amount> {
        if !self.can_repay() {
            return Err(ChronosError::PoolUnavailable(self.config.id));
        }
        if self.principal_outstanding < principal {
            return Err(ChronosError::invariant(
                "repay exceeds outstanding principal",
            ));
        }
        let charges = interest.checked_add(penalty)?;
        let reserve_cut = charges.mul_bps(self.config.reserve_factor_bps)?;
        self.principal_outstanding = self.principal_outstanding.checked_sub(principal)?;
        self.liquidity_available = self
            .liquidity_available
            .checked_add(principal)?
            .checked_add(charges)?;
        self.interest_collected = self.interest_collected.checked_add(interest)?;
        self.penalty_collected = self.penalty_collected.checked_add(penalty)?;
        self.reserve_balance = self.reserve_balance.checked_add(reserve_cut)?;
        self.collateral_locked = self.collateral_locked.saturating_sub(collateral);
        Ok(reserve_cut)
    }

    pub fn absorb_collateral(&mut self, collateral: Amount) -> ChronosResult<()> {
        if self.collateral_locked < collateral {
            return Err(ChronosError::invariant(
                "collateral release exceeds pool lock",
            ));
        }
        self.collateral_locked = self.collateral_locked.checked_sub(collateral)?;
        self.reserve_balance = self.reserve_balance.checked_add(collateral)?;
        Ok(())
    }

    pub fn charge_off(
        &mut self,
        principal: Amount,
        penalty: Amount,
        collateral: Amount,
    ) -> ChronosResult<()> {
        if self.principal_outstanding < principal {
            return Err(ChronosError::invariant(
                "charge-off exceeds outstanding principal",
            ));
        }
        if self.collateral_locked < collateral {
            return Err(ChronosError::invariant(
                "charge-off exceeds locked collateral",
            ));
        }
        self.principal_outstanding = self.principal_outstanding.checked_sub(principal)?;
        self.collateral_locked = self.collateral_locked.checked_sub(collateral)?;
        self.penalty_collected = self.penalty_collected.checked_add(penalty)?;
        self.reserve_balance = self
            .reserve_balance
            .checked_add(collateral)?
            .checked_add(penalty)?;
        Ok(())
    }

    pub fn set_status(&mut self, status: PoolStatus) {
        self.status = status;
    }

    pub fn snapshot(&self) -> ChronosResult<PoolSnapshot> {
        Ok(PoolSnapshot {
            id: self.config.id,
            asset: self.config.asset,
            status: self.status,
            liquidity_available: self.liquidity_available,
            principal_outstanding: self.principal_outstanding,
            collateral_locked: self.collateral_locked,
            interest_collected: self.interest_collected,
            penalty_collected: self.penalty_collected,
            reserve_balance: self.reserve_balance,
            utilization_bps: self.utilization_bps()?,
        })
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PoolSnapshot {
    pub id: PoolId,
    pub asset: AssetId,
    pub status: PoolStatus,
    pub liquidity_available: Amount,
    pub principal_outstanding: Amount,
    pub collateral_locked: Amount,
    pub interest_collected: Amount,
    pub penalty_collected: Amount,
    pub reserve_balance: Amount,
    pub utilization_bps: Bps,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PoolBook {
    pools: HashMap<PoolId, PoolState>,
}

impl PoolBook {
    pub fn insert(&mut self, config: PoolConfig) -> ChronosResult<()> {
        if self.pools.contains_key(&config.id) {
            return Err(ChronosError::invalid(format!(
                "pool {} already exists",
                config.id
            )));
        }
        self.pools.insert(config.id, PoolState::new(config));
        Ok(())
    }

    pub fn get(&self, id: PoolId) -> ChronosResult<&PoolState> {
        self.pools.get(&id).ok_or(ChronosError::UnknownPool(id))
    }

    pub fn get_mut(&mut self, id: PoolId) -> ChronosResult<&mut PoolState> {
        self.pools.get_mut(&id).ok_or(ChronosError::UnknownPool(id))
    }

    pub fn iter(&self) -> impl Iterator<Item = &PoolState> {
        self.pools.values()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut PoolState> {
        self.pools.values_mut()
    }

    pub fn snapshot(&self, id: PoolId) -> ChronosResult<PoolSnapshot> {
        self.get(id)?.snapshot()
    }

    pub fn len(&self) -> usize {
        self.pools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pools.is_empty()
    }
}
