use serde::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};

macro_rules! id_type {
    ($name:ident, $prefix:literal) => {
        #[derive(
            Copy,
            Clone,
            Debug,
            Default,
            Eq,
            PartialEq,
            Ord,
            PartialOrd,
            Hash,
            Serialize,
            Deserialize,
        )]
        pub struct $name(u64);

        impl $name {
            pub const fn new(value: u64) -> Self {
                Self(value)
            }

            pub const fn raw(self) -> u64 {
                self.0
            }

            pub const fn is_zero(self) -> bool {
                self.0 == 0
            }

            pub fn next(self) -> Self {
                Self(self.0.saturating_add(1))
            }
        }

        impl From<u64> for $name {
            fn from(value: u64) -> Self {
                Self::new(value)
            }
        }

        impl Display for $name {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                write!(f, "{}{}", $prefix, self.0)
            }
        }
    };
}

id_type!(AccountId, "acct-");
id_type!(AssetId, "asset-");
id_type!(PoolId, "pool-");
id_type!(PositionId, "pos-");
id_type!(LockId, "lock-");
id_type!(TxId, "tx-");
id_type!(BatchId, "batch-");
id_type!(OperatorId, "op-");

#[derive(
    Copy, Clone, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
pub struct Epoch(u64);

impl Epoch {
    pub const ZERO: Self = Self(0);

    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }

    pub fn checked_add(self, epochs: u64) -> Option<Self> {
        self.0.checked_add(epochs).map(Self)
    }

    pub fn checked_sub(self, epochs: u64) -> Option<Self> {
        self.0.checked_sub(epochs).map(Self)
    }

    pub fn saturating_add(self, epochs: u64) -> Self {
        Self(self.0.saturating_add(epochs))
    }

    pub fn saturating_sub(self, epochs: u64) -> Self {
        Self(self.0.saturating_sub(epochs))
    }

    pub fn distance_to(self, later: Self) -> u64 {
        later.0.saturating_sub(self.0)
    }

    pub fn next(self) -> Self {
        self.saturating_add(1)
    }

    pub fn previous(self) -> Self {
        self.saturating_sub(1)
    }

    pub fn is_boundary_with(self, other: Self) -> bool {
        self.0 == other.0 || self.next() == other || other.next() == self
    }
}

impl From<u64> for Epoch {
    fn from(value: u64) -> Self {
        Self::new(value)
    }
}

impl Display for Epoch {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "epoch-{}", self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct IdAllocator {
    next_account: u64,
    next_asset: u64,
    next_pool: u64,
    next_position: u64,
    next_lock: u64,
    next_tx: u64,
    next_batch: u64,
    next_operator: u64,
}

impl Default for IdAllocator {
    fn default() -> Self {
        Self {
            next_account: 1,
            next_asset: 1,
            next_pool: 1,
            next_position: 1,
            next_lock: 1,
            next_tx: 1,
            next_batch: 1,
            next_operator: 1,
        }
    }
}

impl IdAllocator {
    pub fn account(&mut self) -> AccountId {
        let id = AccountId::new(self.next_account);
        self.next_account = self.next_account.saturating_add(1);
        id
    }

    pub fn asset(&mut self) -> AssetId {
        let id = AssetId::new(self.next_asset);
        self.next_asset = self.next_asset.saturating_add(1);
        id
    }

    pub fn pool(&mut self) -> PoolId {
        let id = PoolId::new(self.next_pool);
        self.next_pool = self.next_pool.saturating_add(1);
        id
    }

    pub fn position(&mut self) -> PositionId {
        let id = PositionId::new(self.next_position);
        self.next_position = self.next_position.saturating_add(1);
        id
    }

    pub fn lock(&mut self) -> LockId {
        let id = LockId::new(self.next_lock);
        self.next_lock = self.next_lock.saturating_add(1);
        id
    }

    pub fn tx(&mut self) -> TxId {
        let id = TxId::new(self.next_tx);
        self.next_tx = self.next_tx.saturating_add(1);
        id
    }

    pub fn batch(&mut self) -> BatchId {
        let id = BatchId::new(self.next_batch);
        self.next_batch = self.next_batch.saturating_add(1);
        id
    }

    pub fn operator(&mut self) -> OperatorId {
        let id = OperatorId::new(self.next_operator);
        self.next_operator = self.next_operator.saturating_add(1);
        id
    }
}
