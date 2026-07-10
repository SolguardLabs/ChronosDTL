use crate::amount::Amount;
use crate::ids::{AccountId, AssetId, Epoch, LockId, PoolId, PositionId, TxId};
use crate::position::PositionState;
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum EventKind {
    AccountCreated,
    AssetRegistered,
    PoolCreated,
    Deposit,
    LiquidityDeposited,
    PositionOpened,
    EpochAdvanced,
    RateAccrued,
    PositionLocked,
    LockReleased,
    PositionClosed,
    PositionExpired,
    InvariantChecked,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChronosEvent {
    pub tx: TxId,
    pub epoch: Epoch,
    pub kind: EventKind,
    pub account: Option<AccountId>,
    pub asset: Option<AssetId>,
    pub pool: Option<PoolId>,
    pub position: Option<PositionId>,
    pub lock: Option<LockId>,
    pub amount: Amount,
    pub state: Option<PositionState>,
    pub memo: String,
}

impl ChronosEvent {
    pub fn new(tx: TxId, epoch: Epoch, kind: EventKind) -> Self {
        Self {
            tx,
            epoch,
            kind,
            account: None,
            asset: None,
            pool: None,
            position: None,
            lock: None,
            amount: Amount::ZERO,
            state: None,
            memo: String::new(),
        }
    }

    pub fn account(mut self, account: AccountId) -> Self {
        self.account = Some(account);
        self
    }

    pub fn asset(mut self, asset: AssetId) -> Self {
        self.asset = Some(asset);
        self
    }

    pub fn pool(mut self, pool: PoolId) -> Self {
        self.pool = Some(pool);
        self
    }

    pub fn position(mut self, position: PositionId) -> Self {
        self.position = Some(position);
        self
    }

    pub fn lock(mut self, lock: LockId) -> Self {
        self.lock = Some(lock);
        self
    }

    pub fn amount(mut self, amount: Amount) -> Self {
        self.amount = amount;
        self
    }

    pub fn state(mut self, state: PositionState) -> Self {
        self.state = Some(state);
        self
    }

    pub fn memo(mut self, memo: impl Into<String>) -> Self {
        self.memo = memo.into();
        self
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EventJournal {
    events: Vec<ChronosEvent>,
}

impl EventJournal {
    pub fn push(&mut self, event: ChronosEvent) {
        self.events.push(event);
    }

    pub fn all(&self) -> &[ChronosEvent] {
        &self.events
    }

    pub fn by_position(&self, position: PositionId) -> Vec<&ChronosEvent> {
        self.events
            .iter()
            .filter(|event| event.position == Some(position))
            .collect()
    }

    pub fn by_kind(&self, kind: EventKind) -> Vec<&ChronosEvent> {
        self.events
            .iter()
            .filter(|event| event.kind == kind)
            .collect()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}
