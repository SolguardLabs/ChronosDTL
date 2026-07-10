use crate::amount::Amount;
use crate::error::{ChronosError, ChronosResult};
use crate::ids::{AccountId, AssetId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AssetStatus {
    Enabled,
    DepositsOnly,
    Disabled,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AssetConfig {
    pub id: AssetId,
    pub symbol: String,
    pub decimals: u8,
    pub min_deposit: Amount,
    pub min_collateral: Amount,
    pub debt_ceiling: Amount,
    pub late_fee_recipient: AccountId,
    pub status: AssetStatus,
}

impl AssetConfig {
    pub fn new(
        id: AssetId,
        symbol: impl Into<String>,
        decimals: u8,
        late_fee_recipient: AccountId,
    ) -> Self {
        Self {
            id,
            symbol: symbol.into(),
            decimals,
            min_deposit: Amount::new(1),
            min_collateral: Amount::new(1),
            debt_ceiling: Amount::new(u128::MAX / 4),
            late_fee_recipient,
            status: AssetStatus::Enabled,
        }
    }

    pub fn with_min_deposit(mut self, amount: Amount) -> Self {
        self.min_deposit = amount;
        self
    }

    pub fn with_min_collateral(mut self, amount: Amount) -> Self {
        self.min_collateral = amount;
        self
    }

    pub fn with_debt_ceiling(mut self, amount: Amount) -> Self {
        self.debt_ceiling = amount;
        self
    }

    pub fn with_status(mut self, status: AssetStatus) -> Self {
        self.status = status;
        self
    }

    pub fn validate_deposit(&self, amount: Amount) -> ChronosResult<()> {
        amount.non_zero()?;
        if amount < self.min_deposit {
            return Err(ChronosError::invalid(format!(
                "deposit below minimum for {}",
                self.symbol
            )));
        }
        match self.status {
            AssetStatus::Enabled | AssetStatus::DepositsOnly => Ok(()),
            AssetStatus::Disabled => Err(ChronosError::AssetDisabled(self.id)),
        }
    }

    pub fn validate_borrow(&self, amount: Amount) -> ChronosResult<()> {
        amount.non_zero()?;
        if amount > self.debt_ceiling {
            return Err(ChronosError::risk("asset debt ceiling exceeded"));
        }
        if self.status != AssetStatus::Enabled {
            return Err(ChronosError::AssetDisabled(self.id));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AssetBook {
    assets: HashMap<AssetId, AssetConfig>,
    by_symbol: HashMap<String, AssetId>,
}

impl AssetBook {
    pub fn insert(&mut self, config: AssetConfig) -> ChronosResult<()> {
        if config.symbol.trim().is_empty() {
            return Err(ChronosError::invalid("asset symbol is empty"));
        }
        if config.decimals > 18 {
            return Err(ChronosError::invalid("asset decimals exceed 18"));
        }
        self.by_symbol.insert(config.symbol.clone(), config.id);
        self.assets.insert(config.id, config);
        Ok(())
    }

    pub fn get(&self, id: AssetId) -> ChronosResult<&AssetConfig> {
        self.assets.get(&id).ok_or(ChronosError::UnknownAsset(id))
    }

    pub fn get_mut(&mut self, id: AssetId) -> ChronosResult<&mut AssetConfig> {
        self.assets
            .get_mut(&id)
            .ok_or(ChronosError::UnknownAsset(id))
    }

    pub fn id_for_symbol(&self, symbol: &str) -> ChronosResult<AssetId> {
        self.by_symbol
            .get(symbol)
            .copied()
            .ok_or_else(|| ChronosError::invalid(format!("unknown symbol {symbol}")))
    }

    pub fn enable(&mut self, id: AssetId) -> ChronosResult<()> {
        self.get_mut(id)?.status = AssetStatus::Enabled;
        Ok(())
    }

    pub fn disable(&mut self, id: AssetId) -> ChronosResult<()> {
        self.get_mut(id)?.status = AssetStatus::Disabled;
        Ok(())
    }

    pub fn iter(&self) -> impl Iterator<Item = &AssetConfig> {
        self.assets.values()
    }

    pub fn len(&self) -> usize {
        self.assets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.assets.is_empty()
    }
}
