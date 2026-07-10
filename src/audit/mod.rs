use crate::amount::{Amount, Bps};
use crate::error::{ChronosError, ChronosResult};
use crate::ids::{AccountId, AssetId, Epoch, PoolId, PositionId};
use crate::ledger::LedgerSnapshot;
use crate::pools::PoolSnapshot;
use crate::position::PositionState;
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AuditSeverity {
    Info,
    Warning,
    High,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuditFinding {
    pub severity: AuditSeverity,
    pub code: String,
    pub subject: String,
    pub message: String,
}

impl AuditFinding {
    pub fn info(
        code: impl Into<String>,
        subject: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: AuditSeverity::Info,
            code: code.into(),
            subject: subject.into(),
            message: message.into(),
        }
    }

    pub fn warning(
        code: impl Into<String>,
        subject: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: AuditSeverity::Warning,
            code: code.into(),
            subject: subject.into(),
            message: message.into(),
        }
    }

    pub fn high(
        code: impl Into<String>,
        subject: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: AuditSeverity::High,
            code: code.into(),
            subject: subject.into(),
            message: message.into(),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConservationReport {
    pub asset: AssetId,
    pub account_total: Amount,
    pub pool_liquidity: Amount,
    pub pool_debt: Amount,
    pub pool_reserves: Amount,
    pub balanced: bool,
}

impl ConservationReport {
    pub fn observed_supply(self) -> ChronosResult<Amount> {
        self.account_total
            .checked_add(self.pool_liquidity)?
            .checked_add(self.pool_reserves)
    }

    pub fn economic_assets(self) -> ChronosResult<Amount> {
        self.pool_liquidity
            .checked_add(self.pool_debt)?
            .checked_add(self.pool_reserves)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExposureReport {
    pub pool: PoolId,
    pub asset: AssetId,
    pub utilization_bps: Bps,
    pub principal_outstanding: Amount,
    pub collateral_locked: Amount,
    pub late_positions: usize,
}

impl ExposureReport {
    pub fn from_pool(pool: PoolSnapshot, late_positions: usize) -> Self {
        Self {
            pool: pool.id,
            asset: pool.asset,
            utilization_bps: pool.utilization_bps,
            principal_outstanding: pool.principal_outstanding,
            collateral_locked: pool.collateral_locked,
            late_positions,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PositionAuditView {
    pub position: PositionId,
    pub borrower: AccountId,
    pub pool: PoolId,
    pub state: PositionState,
    pub effective_maturity_epoch: Epoch,
    pub principal: Amount,
    pub pending_interest: Amount,
    pub pending_penalty: Amount,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuditReport {
    pub snapshot: LedgerSnapshot,
    pub exposures: Vec<ExposureReport>,
    pub findings: Vec<AuditFinding>,
}

impl AuditReport {
    pub fn new(snapshot: LedgerSnapshot) -> Self {
        Self {
            snapshot,
            exposures: Vec::new(),
            findings: Vec::new(),
        }
    }

    pub fn with_exposure(mut self, exposure: ExposureReport) -> Self {
        self.exposures.push(exposure);
        self
    }

    pub fn push(&mut self, finding: AuditFinding) {
        self.findings.push(finding);
    }

    pub fn has_high(&self) -> bool {
        self.findings
            .iter()
            .any(|finding| finding.severity == AuditSeverity::High)
    }

    pub fn validate_pool_bounds(&mut self) -> ChronosResult<()> {
        let mut queued = Vec::new();
        for pool in &self.snapshot.pools {
            if pool.utilization_bps > Bps::from_raw_unchecked(10_000) {
                queued.push(AuditFinding::high(
                    "pool.utilization",
                    pool.id.to_string(),
                    "utilization above nominal full allocation",
                ));
            }
            if pool.principal_outstanding.is_zero() && !pool.collateral_locked.is_zero() {
                queued.push(AuditFinding::warning(
                    "pool.collateral",
                    pool.id.to_string(),
                    "collateral remains locked without principal",
                ));
            }
        }
        self.findings.extend(queued);
        if self.snapshot.assets == 0 {
            return Err(ChronosError::invariant("audit snapshot has no assets"));
        }
        Ok(())
    }
}
