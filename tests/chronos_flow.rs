use chronos_dtl::{
    Amount, Bps, ChronosLedger, ClosePositionRequest, DepositLiquidityRequest, Epoch, LedgerConfig,
    LockMode, LockRequest, OpenPositionRequest, PositionState, RateModel,
};

fn fixture() -> (
    ChronosLedger,
    chronos_dtl::AccountId,
    chronos_dtl::AccountId,
    chronos_dtl::AssetId,
    chronos_dtl::PoolId,
) {
    let mut ledger = ChronosLedger::new(LedgerConfig::default()).unwrap();
    let treasury = ledger.create_account("treasury").unwrap();
    let borrower = ledger.create_account("borrower").unwrap();
    let asset = ledger.register_asset("cUSD", 6, treasury).unwrap();
    ledger
        .deposit(treasury, asset, Amount::new(12_000_000_000))
        .unwrap();
    ledger
        .deposit(borrower, asset, Amount::new(4_000_000_000))
        .unwrap();
    let pool = ledger
        .create_pool(
            asset,
            treasury,
            "receivables",
            RateModel {
                base_bps: Bps::from_raw_unchecked(50),
                utilization_slope_bps: Bps::from_raw_unchecked(250),
                penalty_bps: Bps::from_raw_unchecked(150),
                max_bps: Bps::from_raw_unchecked(2_000),
                compounding: chronos_dtl::CompoundingMode::EpochCompound,
            },
        )
        .unwrap();
    ledger
        .deposit_liquidity(DepositLiquidityRequest {
            provider: treasury,
            pool,
            amount: Amount::new(8_000_000_000),
        })
        .unwrap();
    (ledger, treasury, borrower, asset, pool)
}

#[test]
fn deposits_liquidity_and_opens_position() {
    let (mut ledger, _treasury, borrower, asset, pool) = fixture();
    let position = ledger
        .open_position(OpenPositionRequest {
            borrower,
            pool,
            principal: Amount::new(1_000_000_000),
            collateral: Amount::new(1_500_000_000),
            maturity_epoch: Epoch::new(8),
        })
        .unwrap();

    let account = ledger.account_snapshot(borrower).unwrap();
    let line = account
        .balances
        .iter()
        .find(|line| line.asset == asset)
        .unwrap();
    assert_eq!(line.held, Amount::new(1_500_000_000));
    assert_eq!(
        ledger.positions().get(position).unwrap().state,
        PositionState::Active
    );
    assert_eq!(
        ledger.pool_snapshot(pool).unwrap().principal_outstanding,
        Amount::new(1_000_000_000)
    );
}

#[test]
fn normal_settlement_collects_accrued_interest() {
    let (mut ledger, _treasury, borrower, _asset, pool) = fixture();
    let position = ledger
        .open_position(OpenPositionRequest {
            borrower,
            pool,
            principal: Amount::new(900_000_000),
            collateral: Amount::new(1_300_000_000),
            maturity_epoch: Epoch::new(10),
        })
        .unwrap();

    ledger.advance_epochs(3).unwrap();
    let quote = ledger.quote_position(position).unwrap();
    assert!(quote.breakdown.interest > Amount::ZERO);

    let receipt = ledger
        .settle_position(ClosePositionRequest {
            payer: borrower,
            position,
            max_total_due: Amount::new(2_000_000_000),
        })
        .unwrap();

    assert!(receipt.paid > Amount::new(900_000_000));
    assert_eq!(
        ledger.positions().get(position).unwrap().state,
        PositionState::Closed
    );
    assert_eq!(
        ledger.pool_snapshot(pool).unwrap().principal_outstanding,
        Amount::ZERO
    );
}

#[test]
fn temporal_lock_can_be_released_after_epoch_window() {
    let (mut ledger, _treasury, borrower, _asset, pool) = fixture();
    let position = ledger
        .open_position(OpenPositionRequest {
            borrower,
            pool,
            principal: Amount::new(600_000_000),
            collateral: Amount::new(900_000_000),
            maturity_epoch: Epoch::new(12),
        })
        .unwrap();

    ledger.advance_epochs(2).unwrap();
    let lock = ledger
        .lock_position(LockRequest::new(
            position,
            borrower,
            Epoch::new(4),
            LockMode::Rollover,
        ))
        .unwrap();

    assert_eq!(
        ledger.positions().get(position).unwrap().state,
        PositionState::Locked
    );
    ledger.advance_epochs(2).unwrap();
    ledger.release_lock(lock).unwrap();
    assert_eq!(
        ledger.positions().get(position).unwrap().state,
        PositionState::Active
    );
}

#[test]
fn expired_position_sweeps_collateral_after_grace_window() {
    let (mut ledger, _treasury, borrower, _asset, pool) = fixture();
    let position = ledger
        .open_position(OpenPositionRequest {
            borrower,
            pool,
            principal: Amount::new(500_000_000),
            collateral: Amount::new(800_000_000),
            maturity_epoch: Epoch::new(2),
        })
        .unwrap();

    ledger.advance_epochs(6).unwrap();
    let receipt = ledger.expire_position(position).unwrap();
    assert_eq!(receipt.collateral_absorbed, Amount::new(800_000_000));
    assert_eq!(
        ledger.positions().get(position).unwrap().state,
        PositionState::Expired
    );
    assert_eq!(
        ledger.pool_snapshot(pool).unwrap().principal_outstanding,
        Amount::ZERO
    );
}

#[test]
fn scenario_catalog_builds_a_representative_ledger() {
    let templates = chronos_dtl::ScenarioBuilder::catalog();
    assert!(templates.len() >= 30);
    let ledger = chronos_dtl::ScenarioBuilder::new(templates[0].clone())
        .build()
        .unwrap();
    let snapshot = ledger.snapshot().unwrap();
    assert_eq!(snapshot.accounts, 2);
    assert_eq!(snapshot.pools.len(), 1);
    assert_eq!(snapshot.positions, 1);
}
