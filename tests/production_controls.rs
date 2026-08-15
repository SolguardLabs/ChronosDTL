use chronos_dtl::{
    AccountId, Amount, Bps, CanonicalDigest, CanonicalEnvelope, DigestDomain, Epoch,
    GovernancePolicy, GovernanceRegistry, OperationLifecycle, PolicyOperationSpec, PoolId,
    PoolStressInput, PositionId, TemporalStressEngine, TemporalStressPolicy,
    TemporalStressPosition,
};

fn stress_policy() -> TemporalStressPolicy {
    TemporalStressPolicy {
        collateral_haircut_bps: Bps::from_raw_unchecked(1_000),
        liquidation_cost_bps: Bps::from_raw_unchecked(500),
        rate_shock_bps_per_epoch: Bps::from_raw_unchecked(100),
        concentration_addon_bps: Bps::from_raw_unchecked(1_000),
        target_coverage_bps: Bps::from_raw_unchecked(12_000),
        horizon_epochs: 3,
        operational_buffer: Amount::new(10_000_000),
    }
}

fn stress_positions() -> Vec<TemporalStressPosition> {
    vec![
        TemporalStressPosition {
            position: PositionId::new(1),
            borrower: AccountId::new(1),
            pool: PoolId::new(1),
            principal: Amount::new(100_000_000),
            quoted_interest: Amount::new(5_000_000),
            quoted_penalty: Amount::new(1_000_000),
            collateral: Amount::new(150_000_000),
            maturity_epoch: Epoch::new(10),
        },
        TemporalStressPosition {
            position: PositionId::new(2),
            borrower: AccountId::new(2),
            pool: PoolId::new(1),
            principal: Amount::new(50_000_000),
            quoted_interest: Amount::new(2_000_000),
            quoted_penalty: Amount::ZERO,
            collateral: Amount::new(80_000_000),
            maturity_epoch: Epoch::new(5),
        },
    ]
}

#[test]
fn temporal_stress_prices_haircut_rate_duration_and_concentration() {
    let engine = TemporalStressEngine::new(stress_policy()).unwrap();
    let report = engine
        .evaluate(
            Epoch::new(2),
            &[PoolStressInput {
                pool: PoolId::new(1),
                available_liquidity: Amount::new(200_000_000),
                reserve_balance: Amount::new(20_000_000),
            }],
            &stress_positions(),
        )
        .unwrap();
    let pool = report.pools[0];

    assert_eq!(pool.gross_claim, Amount::new(158_000_000));
    assert_eq!(pool.eligible_collateral, Amount::new(195_500_000));
    assert_eq!(pool.projected_interest, Amount::new(4_740_000));
    assert_eq!(pool.concentration_addon, Amount::new(10_600_000));
    assert_eq!(pool.stressed_obligation, Amount::new(173_340_000));
    assert_eq!(pool.required_coverage, Amount::new(218_008_000));
    assert_eq!(pool.eligible_resources, Amount::new(415_500_000));
    assert_eq!(pool.surplus, Amount::new(197_492_000));
    assert_eq!(pool.shortfall, Amount::ZERO);
    assert_eq!(pool.coverage_bps, Bps::from_raw_unchecked(19_058));
    assert_eq!(
        pool.largest_borrower_share_bps,
        Bps::from_raw_unchecked(6_708)
    );
    assert_eq!(pool.hhi_bps, Bps::from_raw_unchecked(5_582));
    assert_eq!(pool.weighted_maturity_milli_epochs, 6_354);
    assert!(report.policy_satisfied);
}

#[test]
fn temporal_stress_keeps_pool_shortfalls_separate() {
    let engine = TemporalStressEngine::new(stress_policy()).unwrap();
    let report = engine
        .evaluate(
            Epoch::new(2),
            &[
                PoolStressInput {
                    pool: PoolId::new(1),
                    available_liquidity: Amount::new(5_000_000),
                    reserve_balance: Amount::ZERO,
                },
                PoolStressInput {
                    pool: PoolId::new(2),
                    available_liquidity: Amount::new(1_000_000_000),
                    reserve_balance: Amount::ZERO,
                },
            ],
            &stress_positions(),
        )
        .unwrap();

    assert!(!report.pools[0].policy_satisfied);
    assert!(report.pools[1].policy_satisfied);
    assert!(!report.policy_satisfied);
}

#[test]
fn temporal_stress_rejects_unmapped_positions_and_invalid_policy() {
    let invalid = TemporalStressPolicy {
        collateral_haircut_bps: Bps::from_raw_unchecked(8_000),
        liquidation_cost_bps: Bps::from_raw_unchecked(3_000),
        ..stress_policy()
    };
    assert!(TemporalStressEngine::new(invalid).is_err());

    let engine = TemporalStressEngine::new(stress_policy()).unwrap();
    assert!(
        engine
            .evaluate(
                Epoch::ZERO,
                &[PoolStressInput {
                    pool: PoolId::new(2),
                    available_liquidity: Amount::new(1),
                    reserve_balance: Amount::ZERO,
                }],
                &stress_positions(),
            )
            .is_err()
    );
}

fn governance() -> GovernanceRegistry {
    GovernanceRegistry::new(
        GovernancePolicy {
            quorum: 2,
            min_delay_epochs: 2,
            max_execution_window_epochs: 10,
        },
        [AccountId::new(1), AccountId::new(2), AccountId::new(3)],
        AccountId::new(9),
    )
    .unwrap()
}

fn payload(label: &str) -> CanonicalDigest {
    CanonicalEnvelope::new(DigestDomain::Governance, "test-payload", [("label", label)]).digest()
}

fn operation(
    payload_digest: CanonicalDigest,
    predecessor: Option<CanonicalDigest>,
    salt: &str,
    eta: u64,
    expires_at: u64,
) -> PolicyOperationSpec {
    PolicyOperationSpec {
        protocol: "ChronosDTL".to_string(),
        network: "testnet".to_string(),
        chain_id: 84_532,
        target: "risk-registry".to_string(),
        selector: "set-temporal-policy".to_string(),
        payload_digest,
        predecessor,
        salt: salt.to_string(),
        eta: Epoch::new(eta),
        expires_at: Epoch::new(expires_at),
    }
}

#[test]
fn governance_requires_quorum_and_timelock_before_execution() {
    let mut registry = governance();
    let id = registry
        .schedule(
            operation(payload("primary"), None, "q3", 12, 18),
            Epoch::new(10),
        )
        .unwrap();

    registry.approve(id, AccountId::new(1)).unwrap();
    assert_eq!(
        registry.decision(id, Epoch::new(12)).unwrap().lifecycle,
        OperationLifecycle::PendingApprovals
    );
    registry.approve(id, AccountId::new(2)).unwrap();
    assert_eq!(
        registry.decision(id, Epoch::new(11)).unwrap().lifecycle,
        OperationLifecycle::Timelocked
    );
    assert_eq!(
        registry.decision(id, Epoch::new(12)).unwrap().lifecycle,
        OperationLifecycle::Ready
    );
    let receipt = registry.execute(id, Epoch::new(12)).unwrap();
    assert_eq!(receipt.operation, id);
    assert_eq!(receipt.approvals, 2);
    assert_eq!(
        registry.decision(id, Epoch::new(13)).unwrap().lifecycle,
        OperationLifecycle::Executed
    );
}

#[test]
fn governance_digest_binds_payload_window_and_salt() {
    let mut registry = governance();
    let first = registry
        .schedule(operation(payload("a"), None, "q3", 12, 18), Epoch::new(10))
        .unwrap();
    let second = registry
        .schedule(operation(payload("b"), None, "q3", 12, 18), Epoch::new(10))
        .unwrap();
    let third = registry
        .schedule(operation(payload("a"), None, "q4", 12, 18), Epoch::new(10))
        .unwrap();

    assert_ne!(first, second);
    assert_ne!(first, third);
}

#[test]
fn governance_enforces_predecessor_and_expiry() {
    let mut registry = governance();
    let predecessor = registry
        .schedule(
            operation(payload("first"), None, "first", 12, 18),
            Epoch::new(10),
        )
        .unwrap();
    let dependent = registry
        .schedule(
            operation(payload("second"), Some(predecessor), "second", 12, 18),
            Epoch::new(10),
        )
        .unwrap();
    for governor in [AccountId::new(1), AccountId::new(2)] {
        registry.approve(predecessor, governor).unwrap();
        registry.approve(dependent, governor).unwrap();
    }
    assert_eq!(
        registry
            .decision(dependent, Epoch::new(12))
            .unwrap()
            .lifecycle,
        OperationLifecycle::BlockedPredecessor
    );
    registry.execute(predecessor, Epoch::new(12)).unwrap();
    assert_eq!(
        registry
            .decision(dependent, Epoch::new(12))
            .unwrap()
            .lifecycle,
        OperationLifecycle::Ready
    );
    assert_eq!(
        registry
            .decision(dependent, Epoch::new(18))
            .unwrap()
            .lifecycle,
        OperationLifecycle::Expired
    );
}

#[test]
fn governance_guardian_can_cancel_but_cannot_bypass_execution_rules() {
    let mut registry = governance();
    let id = registry
        .schedule(
            operation(payload("cancel"), None, "cancel", 12, 18),
            Epoch::new(10),
        )
        .unwrap();
    assert!(
        registry
            .cancel(id, AccountId::new(1), Epoch::new(11))
            .is_err()
    );
    registry
        .cancel(id, AccountId::new(9), Epoch::new(11))
        .unwrap();
    assert_eq!(
        registry.decision(id, Epoch::new(12)).unwrap().lifecycle,
        OperationLifecycle::Cancelled
    );
    assert!(registry.execute(id, Epoch::new(12)).is_err());
}
