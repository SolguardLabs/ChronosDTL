pub mod accounts;
pub mod amount;
pub mod analytics;
pub mod asset;
pub mod audit;
pub mod batch;
pub mod capital;
pub mod codec;
pub mod debt;
pub mod error;
pub mod events;
pub mod expiry;
pub mod governance;
pub mod ids;
pub mod ledger;
pub mod locks;
pub mod maintenance;
pub mod operators;
pub mod oracle;
pub mod pools;
pub mod portfolio;
pub mod position;
pub mod rates;
pub mod risk;
pub mod routes;
pub mod scenario;
pub mod schedule;
pub mod settlement;
pub mod time;
pub mod treasury;

pub use accounts::{AccountBook, AccountSnapshot, AccountState, BalanceLine};
pub use amount::{AccrualIndex, Amount, Bps, INDEX_SCALE};
pub use analytics::{
    AnalyticsReport, DelinquencyBucket, EpochMetric, MetricKind, MetricWindow, PoolMetricSeries,
};
pub use asset::{AssetBook, AssetConfig, AssetStatus};
pub use audit::{AuditFinding, AuditReport, ConservationReport, ExposureReport};
pub use batch::{BatchBook, BatchInstruction, BatchManifest, BatchStatus, BatchSummary};
pub use capital::{
    MAX_STRESS_HORIZON_EPOCHS, PoolStressInput, PoolStressReport, TemporalStressEngine,
    TemporalStressPolicy, TemporalStressPosition, TemporalStressReport,
};
pub use codec::{CanonicalDigest, CanonicalEnvelope, DigestDomain};
pub use debt::{DebtBreakdown, DebtCalculator, DebtQuote, DebtQuoteInput};
pub use error::{ChronosError, ChronosResult};
pub use events::{ChronosEvent, EventJournal, EventKind};
pub use expiry::{ExpiryDecision, ExpiryEngine, ExpiryPolicy, ExpiryReceipt};
pub use governance::{
    ExecutionReceipt, GovernancePolicy, GovernanceRegistry, OperationDecision, OperationLifecycle,
    OperationStatus, PolicyOperation, PolicyOperationSpec,
};
pub use ids::{AccountId, AssetId, BatchId, Epoch, LockId, OperatorId, PoolId, PositionId, TxId};
pub use ledger::{ChronosLedger, LedgerConfig, LedgerSnapshot};
pub use locks::{
    LockBook, LockMode, LockRecord, LockRequest, LockSnapshot, LockStatus, ReleaseDecision,
};
pub use maintenance::{MaintenanceAction, MaintenancePlan, MaintenanceQueue, MaintenanceStatus};
pub use operators::{OperatorPermission, OperatorProfile, OperatorRegistry, ServiceWindow};
pub use oracle::{EpochPrice, OracleBook, PriceBand, PriceQuote, PriceSource};
pub use pools::{PoolBook, PoolConfig, PoolSnapshot, PoolState, PoolStatus};
pub use portfolio::{AccountExposure, PoolExposureRollup, PortfolioReport, PortfolioView};
pub use position::{
    AccrualCheckpoint, PositionBook, PositionRecord, PositionState, PositionTerms, StateTransition,
};
pub use rates::{AccrualSample, AccrualState, CompoundingMode, RateBook, RateModel, RateTrack};
pub use risk::{RiskDecision, RiskEngine, RiskLimits, RiskSignal};
pub use routes::{RouteBook, RouteQuote, RouteStatus, SettlementLane};
pub use scenario::{ScenarioAccount, ScenarioBuilder, ScenarioPlan, ScenarioTemplate};
pub use schedule::{RepaymentSchedule, ScheduleBuilder, ScheduleLine, ScheduleStatus};
pub use settlement::{
    ClosePositionRequest, DepositLiquidityRequest, OpenPositionRequest, SettlementEngine,
    SettlementReceipt,
};
pub use time::{BoundaryMoment, EpochClock, EpochPolicy, EpochWindow};
pub use treasury::{FeeBucket, FeeRoute, FeeRouter, TreasuryAccount};

pub fn crate_fingerprint() -> CanonicalDigest {
    let envelope = CanonicalEnvelope::new(
        DigestDomain::Library,
        "chronos-dtl",
        [
            ("crate", "chronos_dtl"),
            ("domain", "temporal-settlement"),
            ("version", env!("CARGO_PKG_VERSION")),
        ],
    );
    envelope.digest()
}
