use crate::ids::{AccountId, AssetId, Epoch, LockId, PoolId, PositionId};
use thiserror::Error;

pub type ChronosResult<T> = Result<T, ChronosError>;

#[derive(Debug, Error, Clone, Eq, PartialEq)]
pub enum ChronosError {
    #[error("amount overflow")]
    AmountOverflow,
    #[error("amount must be greater than zero")]
    ZeroAmount,
    #[error("basis points out of range: {0}")]
    BpsOutOfRange(u32),
    #[error("unknown account {0}")]
    UnknownAccount(AccountId),
    #[error("unknown asset {0}")]
    UnknownAsset(AssetId),
    #[error("unknown pool {0}")]
    UnknownPool(PoolId),
    #[error("unknown position {0}")]
    UnknownPosition(PositionId),
    #[error("unknown lock {0}")]
    UnknownLock(LockId),
    #[error("asset {0} is not enabled")]
    AssetDisabled(AssetId),
    #[error("pool {0} is not accepting the requested action")]
    PoolUnavailable(PoolId),
    #[error("balance is insufficient for account {account} and asset {asset}")]
    InsufficientBalance { account: AccountId, asset: AssetId },
    #[error("pool {pool} has insufficient liquidity")]
    InsufficientLiquidity { pool: PoolId },
    #[error("risk limit rejected request: {0}")]
    RiskRejected(String),
    #[error("epoch {0} is outside the accepted range")]
    EpochOutOfRange(Epoch),
    #[error("position {0} is not in a compatible state")]
    PositionState(PositionId),
    #[error("lock {0} is not releasable at this epoch")]
    LockNotReleasable(LockId),
    #[error("request is malformed: {0}")]
    InvalidRequest(String),
    #[error("codec failure: {0}")]
    Codec(String),
    #[error("invariant failed: {0}")]
    Invariant(String),
}

impl ChronosError {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::InvalidRequest(message.into())
    }

    pub fn risk(message: impl Into<String>) -> Self {
        Self::RiskRejected(message.into())
    }

    pub fn invariant(message: impl Into<String>) -> Self {
        Self::Invariant(message.into())
    }
}
