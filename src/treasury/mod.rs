use crate::amount::Amount;
use crate::error::{ChronosError, ChronosResult};
use crate::ids::{AccountId, AssetId, PoolId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum FeeBucket {
    InterestReserve,
    PenaltyReserve,
    CloseFees,
    OperatorFees,
    Insurance,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FeeRoute {
    pub pool: PoolId,
    pub asset: AssetId,
    pub bucket: FeeBucket,
    pub recipient: AccountId,
    pub accrued: Amount,
    pub distributed: Amount,
}

impl FeeRoute {
    pub fn new(pool: PoolId, asset: AssetId, bucket: FeeBucket, recipient: AccountId) -> Self {
        Self {
            pool,
            asset,
            bucket,
            recipient,
            accrued: Amount::ZERO,
            distributed: Amount::ZERO,
        }
    }

    pub fn undistributed(self) -> Amount {
        self.accrued.saturating_sub(self.distributed)
    }

    pub fn accrue(&mut self, amount: Amount) -> ChronosResult<()> {
        self.accrued = self.accrued.checked_add(amount)?;
        Ok(())
    }

    pub fn distribute(&mut self, amount: Amount) -> ChronosResult<()> {
        if self.undistributed() < amount {
            return Err(ChronosError::AmountOverflow);
        }
        self.distributed = self.distributed.checked_add(amount)?;
        Ok(())
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TreasuryAccount {
    pub owner: AccountId,
    pub asset: AssetId,
    pub balance: Amount,
    pub pending_distribution: Amount,
}

impl TreasuryAccount {
    pub fn new(owner: AccountId, asset: AssetId) -> Self {
        Self {
            owner,
            asset,
            balance: Amount::ZERO,
            pending_distribution: Amount::ZERO,
        }
    }

    pub fn receive(&mut self, amount: Amount) -> ChronosResult<()> {
        self.balance = self.balance.checked_add(amount)?;
        self.pending_distribution = self.pending_distribution.checked_add(amount)?;
        Ok(())
    }

    pub fn mark_distributed(&mut self, amount: Amount) -> ChronosResult<()> {
        if self.pending_distribution < amount {
            return Err(ChronosError::AmountOverflow);
        }
        self.pending_distribution = self.pending_distribution.checked_sub(amount)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FeeRouter {
    routes: HashMap<(PoolId, FeeBucket), FeeRoute>,
    treasuries: HashMap<(AccountId, AssetId), TreasuryAccount>,
}

impl FeeRouter {
    pub fn upsert_route(&mut self, route: FeeRoute) {
        self.routes.insert((route.pool, route.bucket), route);
    }

    pub fn route(&self, pool: PoolId, bucket: FeeBucket) -> ChronosResult<FeeRoute> {
        self.routes
            .get(&(pool, bucket))
            .copied()
            .ok_or_else(|| ChronosError::invalid("fee route not configured"))
    }

    pub fn accrue(&mut self, pool: PoolId, bucket: FeeBucket, amount: Amount) -> ChronosResult<()> {
        let route = self
            .routes
            .get_mut(&(pool, bucket))
            .ok_or_else(|| ChronosError::invalid("fee route not configured"))?;
        route.accrue(amount)?;
        let treasury = self
            .treasuries
            .entry((route.recipient, route.asset))
            .or_insert_with(|| TreasuryAccount::new(route.recipient, route.asset));
        treasury.receive(amount)?;
        Ok(())
    }

    pub fn distribute(
        &mut self,
        pool: PoolId,
        bucket: FeeBucket,
        amount: Amount,
    ) -> ChronosResult<TreasuryAccount> {
        let route = self
            .routes
            .get_mut(&(pool, bucket))
            .ok_or_else(|| ChronosError::invalid("fee route not configured"))?;
        route.distribute(amount)?;
        let treasury = self
            .treasuries
            .get_mut(&(route.recipient, route.asset))
            .ok_or_else(|| ChronosError::invalid("treasury not configured"))?;
        treasury.mark_distributed(amount)?;
        Ok(*treasury)
    }

    pub fn treasury(&self, owner: AccountId, asset: AssetId) -> Option<TreasuryAccount> {
        self.treasuries.get(&(owner, asset)).copied()
    }

    pub fn total_pending_for_asset(&self, asset: AssetId) -> Amount {
        self.treasuries
            .values()
            .filter(|treasury| treasury.asset == asset)
            .map(|treasury| treasury.pending_distribution)
            .sum()
    }
}
