use crate::accounts::{AccountBook, AccountSnapshot};
use crate::amount::Amount;
use crate::asset::{AssetBook, AssetConfig};
use crate::debt::{DebtCalculator, DebtQuote, DebtQuoteInput};
use crate::error::{ChronosError, ChronosResult};
use crate::events::{ChronosEvent, EventJournal, EventKind};
use crate::expiry::{ExpiryEngine, ExpiryPolicy, ExpiryReceipt};
use crate::ids::{AccountId, AssetId, Epoch, IdAllocator, LockId, PoolId, PositionId, TxId};
use crate::locks::{LockBook, LockRecord, LockRequest, LockSnapshot, LockStatus};
use crate::pools::{PoolBook, PoolConfig, PoolSnapshot};
use crate::position::{AccrualCheckpoint, PositionBook, PositionRecord, PositionTerms};
use crate::rates::{RateBook, RateModel};
use crate::risk::{RiskEngine, RiskLimits};
use crate::settlement::{
    ClosePositionRequest, DepositLiquidityRequest, OpenPositionRequest, SettlementEngine,
    SettlementReceipt,
};
use crate::time::{EpochClock, EpochPolicy};
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct LedgerConfig {
    pub epoch_policy: EpochPolicy,
    pub debt_calculator: DebtCalculator,
    pub risk_limits: RiskLimits,
    pub expiry_policy: ExpiryPolicy,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LedgerSnapshot {
    pub epoch: Epoch,
    pub accounts: usize,
    pub assets: usize,
    pub pools: Vec<PoolSnapshot>,
    pub positions: usize,
    pub locks: usize,
    pub events: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChronosLedger {
    pub config: LedgerConfig,
    ids: IdAllocator,
    clock: EpochClock,
    assets: AssetBook,
    accounts: AccountBook,
    pools: PoolBook,
    rates: RateBook,
    positions: PositionBook,
    locks: LockBook,
    risk: RiskEngine,
    expiry: ExpiryEngine,
    journal: EventJournal,
}

impl ChronosLedger {
    pub fn new(config: LedgerConfig) -> ChronosResult<Self> {
        let clock = EpochClock::new(config.epoch_policy)?;
        Ok(Self {
            config,
            ids: IdAllocator::default(),
            clock,
            assets: AssetBook::default(),
            accounts: AccountBook::default(),
            pools: PoolBook::default(),
            rates: RateBook::default(),
            positions: PositionBook::default(),
            locks: LockBook::default(),
            risk: RiskEngine::new(config.risk_limits),
            expiry: ExpiryEngine::new(config.expiry_policy),
            journal: EventJournal::default(),
        })
    }

    pub fn standard() -> ChronosResult<Self> {
        Self::new(LedgerConfig::default())
    }

    pub fn clock(&self) -> &EpochClock {
        &self.clock
    }

    pub fn current_epoch(&self) -> Epoch {
        self.clock.current_epoch()
    }

    pub fn journal(&self) -> &EventJournal {
        &self.journal
    }

    pub fn accounts(&self) -> &AccountBook {
        &self.accounts
    }

    pub fn pools(&self) -> &PoolBook {
        &self.pools
    }

    pub fn positions(&self) -> &PositionBook {
        &self.positions
    }

    pub fn locks(&self) -> &LockBook {
        &self.locks
    }

    fn next_tx(&mut self) -> TxId {
        self.ids.tx()
    }

    fn emit(&mut self, event: ChronosEvent) {
        self.journal.push(event);
    }

    pub fn create_account(&mut self, label: impl Into<String>) -> ChronosResult<AccountId> {
        let id = self.ids.account();
        self.accounts.create(id, label.into())?;
        let tx = self.next_tx();
        self.emit(
            ChronosEvent::new(tx, self.current_epoch(), EventKind::AccountCreated).account(id),
        );
        Ok(id)
    }

    pub fn register_asset(
        &mut self,
        symbol: impl Into<String>,
        decimals: u8,
        late_fee_recipient: AccountId,
    ) -> ChronosResult<AssetId> {
        self.accounts.get(late_fee_recipient)?;
        let id = self.ids.asset();
        let config = AssetConfig::new(id, symbol, decimals, late_fee_recipient);
        self.register_asset_config(config)?;
        Ok(id)
    }

    pub fn register_asset_config(&mut self, config: AssetConfig) -> ChronosResult<()> {
        self.accounts.get(config.late_fee_recipient)?;
        let id = config.id;
        self.assets.insert(config)?;
        let tx = self.next_tx();
        self.emit(
            ChronosEvent::new(tx, self.current_epoch(), EventKind::AssetRegistered).asset(id),
        );
        Ok(())
    }

    pub fn create_pool(
        &mut self,
        asset: AssetId,
        controller: AccountId,
        name: impl Into<String>,
        rate_model: RateModel,
    ) -> ChronosResult<PoolId> {
        self.assets.get(asset)?;
        self.accounts.get(controller)?;
        let id = self.ids.pool();
        let config = PoolConfig::new(id, asset, controller, name);
        self.pools.insert(config)?;
        self.rates.insert(id, self.current_epoch(), rate_model)?;
        let tx = self.next_tx();
        self.emit(
            ChronosEvent::new(tx, self.current_epoch(), EventKind::PoolCreated)
                .pool(id)
                .asset(asset),
        );
        Ok(id)
    }

    pub fn deposit(
        &mut self,
        account: AccountId,
        asset: AssetId,
        amount: Amount,
    ) -> ChronosResult<TxId> {
        self.assets.get(asset)?.validate_deposit(amount)?;
        self.accounts.credit(account, asset, amount)?;
        let tx = self.next_tx();
        self.emit(
            ChronosEvent::new(tx, self.current_epoch(), EventKind::Deposit)
                .account(account)
                .asset(asset)
                .amount(amount),
        );
        Ok(tx)
    }

    pub fn deposit_liquidity(&mut self, request: DepositLiquidityRequest) -> ChronosResult<TxId> {
        let asset = self.pools.get(request.pool)?.asset();
        self.assets.get(asset)?.validate_deposit(request.amount)?;
        self.accounts
            .debit_available(request.provider, asset, request.amount)?;
        self.pools
            .get_mut(request.pool)?
            .deposit_liquidity(request.amount)?;
        let tx = self.next_tx();
        self.emit(
            ChronosEvent::new(tx, self.current_epoch(), EventKind::LiquidityDeposited)
                .account(request.provider)
                .asset(asset)
                .pool(request.pool)
                .amount(request.amount),
        );
        Ok(tx)
    }

    pub fn open_position(&mut self, request: OpenPositionRequest) -> ChronosResult<PositionId> {
        let pool = self.pools.get(request.pool)?;
        let asset = pool.asset();
        self.assets.get(asset)?.validate_borrow(request.principal)?;
        self.accounts.get(request.borrower)?;
        let terms = PositionTerms {
            principal: request.principal,
            collateral: request.collateral,
            maturity_epoch: request.maturity_epoch,
            min_close_amount: request.principal,
            max_close_fee_bps: self.config.debt_calculator.close_fee_bps,
        };
        let open_count = self.positions.open_positions_for(request.borrower).len();
        self.risk
            .evaluate_open(terms, pool, open_count)?
            .into_result()?;
        if self.accounts.available(request.borrower, asset)? < request.collateral {
            return Err(ChronosError::InsufficientBalance {
                account: request.borrower,
                asset,
            });
        }
        let accrual = self.rates.current(request.pool)?;
        let id = self.ids.position();
        let checkpoint = AccrualCheckpoint::from_state(accrual);
        let position = PositionRecord::new(
            id,
            request.borrower,
            request.pool,
            asset,
            self.current_epoch(),
            terms,
            checkpoint,
        )?;
        self.accounts
            .reserve(request.borrower, asset, request.collateral)?;
        self.pools
            .get_mut(request.pool)?
            .borrow(request.principal, request.collateral)?;
        self.accounts
            .credit(request.borrower, asset, request.principal)?;
        self.positions.insert(position)?;
        let tx = self.next_tx();
        self.emit(
            ChronosEvent::new(tx, self.current_epoch(), EventKind::PositionOpened)
                .account(request.borrower)
                .asset(asset)
                .pool(request.pool)
                .position(id)
                .amount(request.principal),
        );
        Ok(id)
    }

    pub fn advance_epochs(&mut self, epochs: u64) -> ChronosResult<Vec<Epoch>> {
        let crossed = self.clock.advance_epochs(epochs)?;
        for epoch in crossed.iter().copied() {
            let pool_ids: Vec<PoolId> = self.pools.iter().map(|pool| pool.config.id).collect();
            for pool_id in pool_ids {
                let utilization = self.pools.get(pool_id)?.utilization_bps()?;
                let samples = self.rates.advance_pool(pool_id, epoch, utilization)?;
                for sample in samples {
                    let tx = self.next_tx();
                    self.emit(
                        ChronosEvent::new(tx, epoch, EventKind::RateAccrued)
                            .pool(pool_id)
                            .memo(format!(
                                "rate={} utilization={}",
                                sample.rate_bps, sample.utilization_bps
                            )),
                    );
                }
            }
            let tx = self.next_tx();
            self.emit(ChronosEvent::new(tx, epoch, EventKind::EpochAdvanced));
        }
        Ok(crossed)
    }

    pub fn quote_position(&self, position: PositionId) -> ChronosResult<DebtQuote> {
        let position = self.positions.get(position)?;
        let accrual = self.rates.current(position.pool)?;
        self.config.debt_calculator.quote(
            position,
            DebtQuoteInput {
                now: self.current_epoch(),
                policy: self.config.epoch_policy,
                accrual,
            },
        )
    }

    pub fn lock_position(&mut self, request: LockRequest) -> ChronosResult<LockId> {
        let now = self.current_epoch();
        let position_view = self.positions.get(request.position)?;
        self.risk
            .evaluate_lock(position_view, &request, now)
            .into_result()?;
        let quote = self.quote_position(request.position)?;
        let previous_state = position_view.state;
        let previous_maturity = position_view.effective_maturity_epoch;
        let previous_checkpoint = position_view.checkpoint;
        let previous_version = position_view.state_version;
        let pool = position_view.pool;
        let snapshot = LockSnapshot::from_quote(
            previous_state,
            previous_maturity,
            previous_checkpoint,
            previous_version,
            quote,
        );
        let lock_id = self.ids.lock();
        let lock = LockRecord {
            id: lock_id,
            position: request.position,
            owner: request.owner,
            mode: request.mode,
            operator: request.operator,
            created_epoch: now,
            release_epoch: request.release_epoch,
            status: LockStatus::Active,
            snapshot,
            reference: request.reference,
        };
        let current_accrual = self.rates.current(pool)?;
        let next_checkpoint = AccrualCheckpoint::from_state(current_accrual);
        let materialize = !now.is_boundary_with(previous_maturity);
        if materialize {
            let position = self.positions.get(request.position)?;
            let (interest, penalty, checkpoint) =
                self.config.debt_calculator.quote_materialized_delta(
                    position,
                    DebtQuoteInput {
                        now,
                        policy: self.config.epoch_policy,
                        accrual: current_accrual,
                    },
                )?;
            self.positions
                .get_mut(request.position)?
                .materialize(interest, penalty, checkpoint)?;
        }
        self.positions.get_mut(request.position)?.attach_lock(
            lock_id,
            request.release_epoch,
            next_checkpoint,
            now,
        );
        self.locks.insert(lock)?;
        let tx = self.next_tx();
        self.emit(
            ChronosEvent::new(tx, now, EventKind::PositionLocked)
                .account(request.owner)
                .pool(pool)
                .position(request.position)
                .lock(lock_id)
                .state(previous_state),
        );
        Ok(lock_id)
    }

    pub fn release_lock(&mut self, lock: LockId) -> ChronosResult<TxId> {
        let now = self.current_epoch();
        let position = self.locks.get(lock)?.position;
        self.locks.get_mut(lock)?.release(now)?;
        self.positions.get_mut(position)?.release_lock(now);
        let tx = self.next_tx();
        self.emit(
            ChronosEvent::new(tx, now, EventKind::LockReleased)
                .lock(lock)
                .position(position),
        );
        Ok(tx)
    }

    pub fn settle_position(
        &mut self,
        request: ClosePositionRequest,
    ) -> ChronosResult<SettlementReceipt> {
        let now = self.current_epoch();
        let quote = self.quote_position(request.position)?;
        if !SettlementEngine::within_limit(request, quote) {
            return Err(ChronosError::risk("settlement exceeds caller limit"));
        }
        let position_view = self.positions.get(request.position)?;
        self.risk
            .evaluate_close(position_view, quote)
            .into_result()?;
        let borrower = position_view.borrower;
        let pool = position_view.pool;
        let asset = position_view.asset;
        let state_before = position_view.state;
        let collateral = position_view.collateral();
        let active_lock = position_view.active_lock;
        let total_due = quote.total_due()?;
        self.accounts
            .debit_available(request.payer, asset, total_due)?;
        let interest_component = quote
            .breakdown
            .interest
            .checked_add(quote.breakdown.close_fee)?;
        self.pools.get_mut(pool)?.repay(
            quote.breakdown.principal,
            interest_component,
            quote.breakdown.penalty,
            collateral,
        )?;
        self.accounts.release(borrower, asset, collateral)?;
        if let Some(lock_id) = active_lock
            && let Ok(lock) = self.locks.get_mut(lock_id)
        {
            lock.status = LockStatus::Released;
        }
        self.positions.get_mut(request.position)?.close(now);
        let tx = self.next_tx();
        self.emit(
            ChronosEvent::new(tx, now, EventKind::PositionClosed)
                .account(request.payer)
                .asset(asset)
                .pool(pool)
                .position(request.position)
                .amount(total_due)
                .state(state_before),
        );
        Ok(SettlementReceipt {
            tx,
            position: request.position,
            payer: request.payer,
            pool,
            asset,
            state_before,
            quote,
            paid: total_due,
            collateral_released: collateral,
            released_lock: active_lock,
        })
    }

    pub fn expire_position(&mut self, position: PositionId) -> ChronosResult<ExpiryReceipt> {
        let now = self.current_epoch();
        let quote = self.quote_position(position)?;
        let position_view = self.positions.get(position)?;
        let decision = self
            .expiry
            .decision(position_view, now, self.config.epoch_policy)?;
        if decision != crate::expiry::ExpiryDecision::SweepCollateral {
            return Err(ChronosError::PositionState(position));
        }
        let borrower = position_view.borrower;
        let pool = position_view.pool;
        let asset = position_view.asset;
        let collateral = position_view.collateral();
        let collateral_absorbed = self.expiry.collateral_absorption(position_view)?;
        self.accounts
            .debit_reserved(borrower, asset, collateral_absorbed)?;
        let remaining_collateral = collateral.saturating_sub(collateral_absorbed);
        if !remaining_collateral.is_zero() {
            self.accounts
                .release(borrower, asset, remaining_collateral)?;
        }
        self.pools.get_mut(pool)?.charge_off(
            quote.breakdown.principal,
            quote.breakdown.penalty,
            collateral_absorbed,
        )?;
        self.positions.get_mut(position)?.expire(now);
        let tx = self.next_tx();
        self.emit(
            ChronosEvent::new(tx, now, EventKind::PositionExpired)
                .account(borrower)
                .asset(asset)
                .pool(pool)
                .position(position)
                .amount(collateral_absorbed),
        );
        Ok(ExpiryReceipt {
            tx,
            position,
            borrower,
            pool,
            asset,
            decided_at_epoch: now,
            decision,
            quote,
            collateral_absorbed,
        })
    }

    pub fn account_snapshot(&self, account: AccountId) -> ChronosResult<AccountSnapshot> {
        self.accounts.snapshot(account)
    }

    pub fn pool_snapshot(&self, pool: PoolId) -> ChronosResult<PoolSnapshot> {
        self.pools.snapshot(pool)
    }

    pub fn account_balance(&self, account: AccountId, asset: AssetId) -> ChronosResult<Amount> {
        self.accounts.balance(account, asset)
    }

    pub fn snapshot(&self) -> ChronosResult<LedgerSnapshot> {
        let pools = self
            .pools
            .iter()
            .map(|pool| pool.snapshot())
            .collect::<ChronosResult<Vec<_>>>()?;
        Ok(LedgerSnapshot {
            epoch: self.current_epoch(),
            accounts: self.accounts.len(),
            assets: self.assets.len(),
            pools,
            positions: self.positions.len(),
            locks: self.locks.len(),
            events: self.journal.len(),
        })
    }
}
