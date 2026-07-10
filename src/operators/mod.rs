use crate::amount::{Amount, Bps};
use crate::error::{ChronosError, ChronosResult};
use crate::ids::{AccountId, Epoch, OperatorId, PoolId};
use crate::time::EpochWindow;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum OperatorPermission {
    Quote,
    Lock,
    Release,
    Expire,
    Sweep,
    ConfigureRates,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ServiceWindow {
    pub pool: PoolId,
    pub window: EpochWindow,
    pub max_notional: Amount,
    pub fee_bps: Bps,
}

impl ServiceWindow {
    pub fn new(
        pool: PoolId,
        start: Epoch,
        end: Epoch,
        max_notional: Amount,
    ) -> ChronosResult<Self> {
        Ok(Self {
            pool,
            window: EpochWindow::new(start, end)?,
            max_notional,
            fee_bps: Bps::from_raw_unchecked(25),
        })
    }

    pub fn contains(&self, epoch: Epoch, amount: Amount) -> bool {
        self.window.contains(epoch) && amount <= self.max_notional
    }

    pub fn quote_fee(&self, amount: Amount) -> ChronosResult<Amount> {
        amount.mul_bps(self.fee_bps)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OperatorProfile {
    pub id: OperatorId,
    pub controller: AccountId,
    pub label: String,
    pub permissions: BTreeSet<OperatorPermission>,
    pub service_windows: Vec<ServiceWindow>,
    pub active: bool,
    pub reputation_score: u32,
    pub settled_notional: Amount,
    pub failed_notional: Amount,
}

impl OperatorProfile {
    pub fn new(id: OperatorId, controller: AccountId, label: impl Into<String>) -> Self {
        let mut permissions = BTreeSet::new();
        permissions.insert(OperatorPermission::Quote);
        permissions.insert(OperatorPermission::Lock);
        permissions.insert(OperatorPermission::Release);
        Self {
            id,
            controller,
            label: label.into(),
            permissions,
            service_windows: Vec::new(),
            active: true,
            reputation_score: 10_000,
            settled_notional: Amount::ZERO,
            failed_notional: Amount::ZERO,
        }
    }

    pub fn grant(&mut self, permission: OperatorPermission) {
        self.permissions.insert(permission);
    }

    pub fn revoke(&mut self, permission: OperatorPermission) {
        self.permissions.remove(&permission);
    }

    pub fn has(&self, permission: OperatorPermission) -> bool {
        self.active && self.permissions.contains(&permission)
    }

    pub fn add_window(&mut self, window: ServiceWindow) {
        self.service_windows.push(window);
        self.service_windows
            .sort_by_key(|window| window.window.start);
    }

    pub fn can_service(
        &self,
        permission: OperatorPermission,
        pool: PoolId,
        epoch: Epoch,
        amount: Amount,
    ) -> bool {
        self.has(permission)
            && self
                .service_windows
                .iter()
                .any(|window| window.pool == pool && window.contains(epoch, amount))
    }

    pub fn record_success(&mut self, amount: Amount) -> ChronosResult<()> {
        self.settled_notional = self.settled_notional.checked_add(amount)?;
        self.reputation_score = self.reputation_score.saturating_add(3).min(20_000);
        Ok(())
    }

    pub fn record_failure(&mut self, amount: Amount) -> ChronosResult<()> {
        self.failed_notional = self.failed_notional.checked_add(amount)?;
        self.reputation_score = self.reputation_score.saturating_sub(250);
        Ok(())
    }

    pub fn failure_ratio_bps(&self) -> ChronosResult<Bps> {
        let total = self.settled_notional.checked_add(self.failed_notional)?;
        if total.is_zero() {
            return Ok(Bps::ZERO);
        }
        let raw = self
            .failed_notional
            .raw()
            .checked_mul(10_000)
            .and_then(|value| value.checked_div(total.raw()))
            .ok_or(ChronosError::AmountOverflow)?;
        Bps::new(raw as u32)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OperatorRegistry {
    operators: HashMap<OperatorId, OperatorProfile>,
    by_controller: HashMap<AccountId, Vec<OperatorId>>,
}

impl OperatorRegistry {
    pub fn insert(&mut self, profile: OperatorProfile) -> ChronosResult<()> {
        if self.operators.contains_key(&profile.id) {
            return Err(ChronosError::invalid(format!(
                "operator {} already exists",
                profile.id
            )));
        }
        self.by_controller
            .entry(profile.controller)
            .or_default()
            .push(profile.id);
        self.operators.insert(profile.id, profile);
        Ok(())
    }

    pub fn get(&self, id: OperatorId) -> ChronosResult<&OperatorProfile> {
        self.operators
            .get(&id)
            .ok_or_else(|| ChronosError::invalid(format!("unknown operator {id}")))
    }

    pub fn get_mut(&mut self, id: OperatorId) -> ChronosResult<&mut OperatorProfile> {
        self.operators
            .get_mut(&id)
            .ok_or_else(|| ChronosError::invalid(format!("unknown operator {id}")))
    }

    pub fn for_controller(&self, controller: AccountId) -> Vec<&OperatorProfile> {
        self.by_controller
            .get(&controller)
            .into_iter()
            .flat_map(|ids| ids.iter())
            .filter_map(|id| self.operators.get(id))
            .collect()
    }

    pub fn authorize(
        &self,
        operator: OperatorId,
        permission: OperatorPermission,
        pool: PoolId,
        epoch: Epoch,
        amount: Amount,
    ) -> ChronosResult<()> {
        let profile = self.get(operator)?;
        if profile.can_service(permission, pool, epoch, amount) {
            Ok(())
        } else {
            Err(ChronosError::risk(
                "operator is not authorized for requested service",
            ))
        }
    }

    pub fn active_count(&self) -> usize {
        self.operators
            .values()
            .filter(|profile| profile.active)
            .count()
    }

    pub fn iter(&self) -> impl Iterator<Item = &OperatorProfile> {
        self.operators.values()
    }
}
