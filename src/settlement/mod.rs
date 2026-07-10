use crate::amount::Amount;
use crate::debt::DebtQuote;
use crate::ids::{AccountId, AssetId, Epoch, LockId, PoolId, PositionId, TxId};
use crate::position::PositionState;
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DepositLiquidityRequest {
    pub provider: AccountId,
    pub pool: PoolId,
    pub amount: Amount,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OpenPositionRequest {
    pub borrower: AccountId,
    pub pool: PoolId,
    pub principal: Amount,
    pub collateral: Amount,
    pub maturity_epoch: Epoch,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ClosePositionRequest {
    pub payer: AccountId,
    pub position: PositionId,
    pub max_total_due: Amount,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SettlementReceipt {
    pub tx: TxId,
    pub position: PositionId,
    pub payer: AccountId,
    pub pool: PoolId,
    pub asset: AssetId,
    pub state_before: PositionState,
    pub quote: DebtQuote,
    pub paid: Amount,
    pub collateral_released: Amount,
    pub released_lock: Option<LockId>,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SettlementEngine;

impl SettlementEngine {
    pub fn within_limit(request: ClosePositionRequest, quote: DebtQuote) -> bool {
        quote
            .total_due()
            .map(|total| total <= request.max_total_due)
            .unwrap_or(false)
    }
}
