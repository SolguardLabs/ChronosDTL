use crate::codec::{CanonicalDigest, CanonicalEnvelope, DigestDomain};
use crate::error::{ChronosError, ChronosResult};
use crate::ids::{AccountId, Epoch};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GovernancePolicy {
    pub quorum: usize,
    pub min_delay_epochs: u64,
    pub max_execution_window_epochs: u64,
}

impl GovernancePolicy {
    pub fn validate(self, governor_count: usize) -> ChronosResult<Self> {
        if self.quorum == 0 || self.quorum > governor_count {
            return Err(ChronosError::risk(
                "governance quorum must fit the active governor set",
            ));
        }
        if self.min_delay_epochs == 0 {
            return Err(ChronosError::risk(
                "governance minimum delay must be non-zero",
            ));
        }
        if self.max_execution_window_epochs == 0 {
            return Err(ChronosError::risk(
                "governance execution window must be non-zero",
            ));
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PolicyOperationSpec {
    pub protocol: String,
    pub network: String,
    pub chain_id: u64,
    pub target: String,
    pub selector: String,
    pub payload_digest: CanonicalDigest,
    pub predecessor: Option<CanonicalDigest>,
    pub salt: String,
    pub eta: Epoch,
    pub expires_at: Epoch,
}

impl PolicyOperationSpec {
    fn validate_field(value: &str, field: &str) -> ChronosResult<()> {
        if value.is_empty() || value.len() > 128 {
            return Err(ChronosError::invalid(format!(
                "governance {field} must contain 1..128 bytes"
            )));
        }
        if value.contains(['\n', '\r', '\0']) {
            return Err(ChronosError::invalid(format!(
                "governance {field} contains a reserved delimiter"
            )));
        }
        Ok(())
    }

    pub fn validate(&self) -> ChronosResult<()> {
        Self::validate_field(&self.protocol, "protocol")?;
        Self::validate_field(&self.network, "network")?;
        Self::validate_field(&self.target, "target")?;
        Self::validate_field(&self.selector, "selector")?;
        Self::validate_field(&self.salt, "salt")?;
        if self.chain_id == 0 {
            return Err(ChronosError::invalid(
                "governance chain id must be non-zero",
            ));
        }
        if self.expires_at <= self.eta {
            return Err(ChronosError::invalid("governance expiry must be after eta"));
        }
        Ok(())
    }

    pub fn digest(&self, quorum: usize) -> CanonicalDigest {
        CanonicalEnvelope::new(
            DigestDomain::Governance,
            "chronos-policy-operation-v1",
            [
                ("protocol", self.protocol.clone()),
                ("network", self.network.clone()),
                ("chain_id", self.chain_id.to_string()),
                ("target", self.target.clone()),
                ("selector", self.selector.clone()),
                ("payload_digest", self.payload_digest.hex()),
                (
                    "predecessor",
                    self.predecessor
                        .map(CanonicalDigest::hex)
                        .unwrap_or_else(|| "none".to_string()),
                ),
                ("salt", self.salt.clone()),
                ("eta", self.eta.raw().to_string()),
                ("expires_at", self.expires_at.raw().to_string()),
                ("quorum", quorum.to_string()),
            ],
        )
        .digest()
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum OperationStatus {
    Scheduled,
    Executed,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PolicyOperation {
    pub id: CanonicalDigest,
    pub spec: PolicyOperationSpec,
    pub scheduled_at: Epoch,
    pub status: OperationStatus,
    pub approvals: BTreeSet<AccountId>,
    pub executed_at: Option<Epoch>,
    pub cancelled_at: Option<Epoch>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum OperationLifecycle {
    PendingApprovals,
    Timelocked,
    BlockedPredecessor,
    Ready,
    Expired,
    Executed,
    Cancelled,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OperationDecision {
    pub id: CanonicalDigest,
    pub lifecycle: OperationLifecycle,
    pub approvals: usize,
    pub approvals_remaining: usize,
    pub predecessor_satisfied: bool,
    pub executable: bool,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExecutionReceipt {
    pub operation: CanonicalDigest,
    pub payload_digest: CanonicalDigest,
    pub executed_at: Epoch,
    pub approvals: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GovernanceRegistry {
    policy: GovernancePolicy,
    governors: BTreeSet<AccountId>,
    guardian: AccountId,
    operations: HashMap<CanonicalDigest, PolicyOperation>,
}

impl GovernanceRegistry {
    pub fn new<I>(
        policy: GovernancePolicy,
        governors: I,
        guardian: AccountId,
    ) -> ChronosResult<Self>
    where
        I: IntoIterator<Item = AccountId>,
    {
        let governors = governors.into_iter().collect::<BTreeSet<_>>();
        if governors.is_empty() {
            return Err(ChronosError::invalid(
                "governance requires at least one governor",
            ));
        }
        Ok(Self {
            policy: policy.validate(governors.len())?,
            governors,
            guardian,
            operations: HashMap::new(),
        })
    }

    pub fn policy(&self) -> GovernancePolicy {
        self.policy
    }

    pub fn guardian(&self) -> AccountId {
        self.guardian
    }

    pub fn governors(&self) -> impl Iterator<Item = AccountId> + '_ {
        self.governors.iter().copied()
    }

    pub fn schedule(
        &mut self,
        spec: PolicyOperationSpec,
        now: Epoch,
    ) -> ChronosResult<CanonicalDigest> {
        spec.validate()?;
        let minimum_eta = now
            .checked_add(self.policy.min_delay_epochs)
            .ok_or(ChronosError::EpochOutOfRange(now))?;
        if spec.eta < minimum_eta {
            return Err(ChronosError::risk(
                "governance operation does not satisfy minimum delay",
            ));
        }
        let window = spec.eta.distance_to(spec.expires_at);
        if window == 0 || window > self.policy.max_execution_window_epochs {
            return Err(ChronosError::risk(
                "governance execution window exceeds policy",
            ));
        }
        let id = spec.digest(self.policy.quorum);
        if self.operations.contains_key(&id) {
            return Err(ChronosError::invalid(
                "governance operation is already scheduled",
            ));
        }
        self.operations.insert(
            id,
            PolicyOperation {
                id,
                spec,
                scheduled_at: now,
                status: OperationStatus::Scheduled,
                approvals: BTreeSet::new(),
                executed_at: None,
                cancelled_at: None,
            },
        );
        Ok(id)
    }

    pub fn approve(
        &mut self,
        operation: CanonicalDigest,
        governor: AccountId,
    ) -> ChronosResult<usize> {
        if !self.governors.contains(&governor) {
            return Err(ChronosError::risk(
                "approval signer is not an active governor",
            ));
        }
        let record = self
            .operations
            .get_mut(&operation)
            .ok_or_else(|| ChronosError::invalid("unknown governance operation"))?;
        if record.status != OperationStatus::Scheduled {
            return Err(ChronosError::risk(
                "governance operation no longer accepts approvals",
            ));
        }
        record.approvals.insert(governor);
        Ok(record.approvals.len())
    }

    pub fn decision(
        &self,
        operation: CanonicalDigest,
        now: Epoch,
    ) -> ChronosResult<OperationDecision> {
        let record = self
            .operations
            .get(&operation)
            .ok_or_else(|| ChronosError::invalid("unknown governance operation"))?;
        let approvals = record.approvals.len();
        let approvals_remaining = self.policy.quorum.saturating_sub(approvals);
        let predecessor_satisfied = match record.spec.predecessor {
            Some(predecessor) => self
                .operations
                .get(&predecessor)
                .is_some_and(|record| record.status == OperationStatus::Executed),
            None => true,
        };
        let lifecycle = match record.status {
            OperationStatus::Executed => OperationLifecycle::Executed,
            OperationStatus::Cancelled => OperationLifecycle::Cancelled,
            OperationStatus::Scheduled if now >= record.spec.expires_at => {
                OperationLifecycle::Expired
            }
            OperationStatus::Scheduled if approvals_remaining > 0 => {
                OperationLifecycle::PendingApprovals
            }
            OperationStatus::Scheduled if now < record.spec.eta => OperationLifecycle::Timelocked,
            OperationStatus::Scheduled if !predecessor_satisfied => {
                OperationLifecycle::BlockedPredecessor
            }
            OperationStatus::Scheduled => OperationLifecycle::Ready,
        };
        Ok(OperationDecision {
            id: operation,
            lifecycle,
            approvals,
            approvals_remaining,
            predecessor_satisfied,
            executable: lifecycle == OperationLifecycle::Ready,
        })
    }

    pub fn execute(
        &mut self,
        operation: CanonicalDigest,
        now: Epoch,
    ) -> ChronosResult<ExecutionReceipt> {
        let decision = self.decision(operation, now)?;
        if !decision.executable {
            return Err(ChronosError::risk("governance operation is not executable"));
        }
        let record = self
            .operations
            .get_mut(&operation)
            .ok_or_else(|| ChronosError::invalid("unknown governance operation"))?;
        record.status = OperationStatus::Executed;
        record.executed_at = Some(now);
        Ok(ExecutionReceipt {
            operation,
            payload_digest: record.spec.payload_digest,
            executed_at: now,
            approvals: record.approvals.len(),
        })
    }

    pub fn cancel(
        &mut self,
        operation: CanonicalDigest,
        caller: AccountId,
        now: Epoch,
    ) -> ChronosResult<()> {
        if caller != self.guardian {
            return Err(ChronosError::risk(
                "only the governance guardian can cancel an operation",
            ));
        }
        let record = self
            .operations
            .get_mut(&operation)
            .ok_or_else(|| ChronosError::invalid("unknown governance operation"))?;
        if record.status != OperationStatus::Scheduled {
            return Err(ChronosError::risk(
                "only a scheduled governance operation can be cancelled",
            ));
        }
        record.status = OperationStatus::Cancelled;
        record.cancelled_at = Some(now);
        Ok(())
    }

    pub fn operation(&self, id: CanonicalDigest) -> ChronosResult<&PolicyOperation> {
        self.operations
            .get(&id)
            .ok_or_else(|| ChronosError::invalid("unknown governance operation"))
    }

    pub fn len(&self) -> usize {
        self.operations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }
}
