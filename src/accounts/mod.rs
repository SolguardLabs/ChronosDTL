use crate::amount::Amount;
use crate::error::{ChronosError, ChronosResult};
use crate::ids::{AccountId, AssetId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct BalanceLine {
    pub asset: AssetId,
    pub total: Amount,
    pub held: Amount,
    pub available: Amount,
}

impl BalanceLine {
    pub fn new(asset: AssetId, total: Amount, held: Amount) -> Self {
        Self {
            asset,
            total,
            held,
            available: total.saturating_sub(held),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AccountState {
    pub id: AccountId,
    pub label: String,
    pub balances: HashMap<AssetId, Amount>,
    pub holds: HashMap<AssetId, Amount>,
    pub nonce: u64,
    pub active: bool,
}

impl AccountState {
    pub fn new(id: AccountId, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            balances: HashMap::new(),
            holds: HashMap::new(),
            nonce: 0,
            active: true,
        }
    }

    pub fn balance(&self, asset: AssetId) -> Amount {
        self.balances.get(&asset).copied().unwrap_or_default()
    }

    pub fn held(&self, asset: AssetId) -> Amount {
        self.holds.get(&asset).copied().unwrap_or_default()
    }

    pub fn available(&self, asset: AssetId) -> Amount {
        self.balance(asset).saturating_sub(self.held(asset))
    }

    pub fn line(&self, asset: AssetId) -> BalanceLine {
        BalanceLine::new(asset, self.balance(asset), self.held(asset))
    }

    pub fn credit(&mut self, asset: AssetId, amount: Amount) -> ChronosResult<()> {
        amount.non_zero()?;
        let next = self.balance(asset).checked_add(amount)?;
        self.balances.insert(asset, next);
        Ok(())
    }

    pub fn debit_available(&mut self, asset: AssetId, amount: Amount) -> ChronosResult<()> {
        amount.non_zero()?;
        if self.available(asset) < amount {
            return Err(ChronosError::InsufficientBalance {
                account: self.id,
                asset,
            });
        }
        let next = self.balance(asset).checked_sub(amount)?;
        self.balances.insert(asset, next);
        Ok(())
    }

    pub fn reserve(&mut self, asset: AssetId, amount: Amount) -> ChronosResult<()> {
        amount.non_zero()?;
        if self.available(asset) < amount {
            return Err(ChronosError::InsufficientBalance {
                account: self.id,
                asset,
            });
        }
        let next = self.held(asset).checked_add(amount)?;
        self.holds.insert(asset, next);
        Ok(())
    }

    pub fn release(&mut self, asset: AssetId, amount: Amount) -> ChronosResult<()> {
        amount.non_zero()?;
        let held = self.held(asset);
        if held < amount {
            return Err(ChronosError::InsufficientBalance {
                account: self.id,
                asset,
            });
        }
        self.holds.insert(asset, held.checked_sub(amount)?);
        Ok(())
    }

    pub fn debit_reserved(&mut self, asset: AssetId, amount: Amount) -> ChronosResult<()> {
        self.release(asset, amount)?;
        let next = self.balance(asset).checked_sub(amount)?;
        self.balances.insert(asset, next);
        Ok(())
    }

    pub fn bump_nonce(&mut self) -> u64 {
        self.nonce = self.nonce.saturating_add(1);
        self.nonce
    }

    pub fn lines(&self) -> Vec<BalanceLine> {
        let mut assets: Vec<AssetId> = self.balances.keys().copied().collect();
        for asset in self.holds.keys().copied() {
            if !assets.contains(&asset) {
                assets.push(asset);
            }
        }
        assets.sort();
        assets.into_iter().map(|asset| self.line(asset)).collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AccountSnapshot {
    pub id: AccountId,
    pub label: String,
    pub balances: Vec<BalanceLine>,
    pub nonce: u64,
    pub active: bool,
}

impl From<&AccountState> for AccountSnapshot {
    fn from(value: &AccountState) -> Self {
        Self {
            id: value.id,
            label: value.label.clone(),
            balances: value.lines(),
            nonce: value.nonce,
            active: value.active,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AccountBook {
    accounts: HashMap<AccountId, AccountState>,
}

impl AccountBook {
    pub fn create(&mut self, id: AccountId, label: impl Into<String>) -> ChronosResult<()> {
        if self.accounts.contains_key(&id) {
            return Err(ChronosError::invalid(format!(
                "account {id} already exists"
            )));
        }
        self.accounts.insert(id, AccountState::new(id, label));
        Ok(())
    }

    pub fn get(&self, id: AccountId) -> ChronosResult<&AccountState> {
        self.accounts
            .get(&id)
            .ok_or(ChronosError::UnknownAccount(id))
    }

    pub fn get_mut(&mut self, id: AccountId) -> ChronosResult<&mut AccountState> {
        self.accounts
            .get_mut(&id)
            .ok_or(ChronosError::UnknownAccount(id))
    }

    pub fn credit(
        &mut self,
        account: AccountId,
        asset: AssetId,
        amount: Amount,
    ) -> ChronosResult<()> {
        self.get_mut(account)?.credit(asset, amount)
    }

    pub fn debit_available(
        &mut self,
        account: AccountId,
        asset: AssetId,
        amount: Amount,
    ) -> ChronosResult<()> {
        self.get_mut(account)?.debit_available(asset, amount)
    }

    pub fn reserve(
        &mut self,
        account: AccountId,
        asset: AssetId,
        amount: Amount,
    ) -> ChronosResult<()> {
        self.get_mut(account)?.reserve(asset, amount)
    }

    pub fn release(
        &mut self,
        account: AccountId,
        asset: AssetId,
        amount: Amount,
    ) -> ChronosResult<()> {
        self.get_mut(account)?.release(asset, amount)
    }

    pub fn debit_reserved(
        &mut self,
        account: AccountId,
        asset: AssetId,
        amount: Amount,
    ) -> ChronosResult<()> {
        self.get_mut(account)?.debit_reserved(asset, amount)
    }

    pub fn balance(&self, account: AccountId, asset: AssetId) -> ChronosResult<Amount> {
        Ok(self.get(account)?.balance(asset))
    }

    pub fn available(&self, account: AccountId, asset: AssetId) -> ChronosResult<Amount> {
        Ok(self.get(account)?.available(asset))
    }

    pub fn snapshot(&self, account: AccountId) -> ChronosResult<AccountSnapshot> {
        Ok(AccountSnapshot::from(self.get(account)?))
    }

    pub fn iter(&self) -> impl Iterator<Item = &AccountState> {
        self.accounts.values()
    }

    pub fn len(&self) -> usize {
        self.accounts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.accounts.is_empty()
    }
}
