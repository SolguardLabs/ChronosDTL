use crate::amount::{Amount, Bps};
use crate::error::ChronosResult;
use crate::ids::Epoch;
use crate::ledger::{ChronosLedger, LedgerConfig};
use crate::rates::RateModel;
use crate::settlement::{DepositLiquidityRequest, OpenPositionRequest};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ScenarioAccount {
    pub label: String,
    pub initial_balance: Amount,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ScenarioPlan {
    pub name: String,
    pub liquidity: Amount,
    pub principal: Amount,
    pub collateral: Amount,
    pub maturity_offset: u64,
    pub advance_epochs_before_close: u64,
    pub base_bps: Bps,
    pub slope_bps: Bps,
    pub penalty_bps: Bps,
}

impl ScenarioPlan {
    pub fn rate_model(&self) -> RateModel {
        RateModel {
            base_bps: self.base_bps,
            utilization_slope_bps: self.slope_bps,
            penalty_bps: self.penalty_bps,
            max_bps: Bps::from_raw_unchecked(5_000),
            compounding: crate::rates::CompoundingMode::EpochCompound,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ScenarioTemplate {
    pub name: String,
    pub accounts: Vec<ScenarioAccount>,
    pub plan: ScenarioPlan,
}

impl ScenarioTemplate {
    pub fn conservative() -> Self {
        Self {
            name: "conservative-usdc-epoch".to_string(),
            accounts: vec![
                ScenarioAccount {
                    label: "treasury".to_string(),
                    initial_balance: Amount::new(8_000_000_000),
                },
                ScenarioAccount {
                    label: "borrower".to_string(),
                    initial_balance: Amount::new(2_000_000_000),
                },
            ],
            plan: ScenarioPlan {
                name: "thirty-day-receivable".to_string(),
                liquidity: Amount::new(5_000_000_000),
                principal: Amount::new(750_000_000),
                collateral: Amount::new(1_000_000_000),
                maturity_offset: 8,
                advance_epochs_before_close: 4,
                base_bps: Bps::from_raw_unchecked(20),
                slope_bps: Bps::from_raw_unchecked(150),
                penalty_bps: Bps::from_raw_unchecked(80),
            },
        }
    }

    pub fn merchant_rollover() -> Self {
        Self {
            name: "merchant-rollover".to_string(),
            accounts: vec![
                ScenarioAccount {
                    label: "facility-provider".to_string(),
                    initial_balance: Amount::new(12_000_000_000),
                },
                ScenarioAccount {
                    label: "merchant".to_string(),
                    initial_balance: Amount::new(3_000_000_000),
                },
            ],
            plan: ScenarioPlan {
                name: "invoice-rollover".to_string(),
                liquidity: Amount::new(9_000_000_000),
                principal: Amount::new(1_250_000_000),
                collateral: Amount::new(1_750_000_000),
                maturity_offset: 6,
                advance_epochs_before_close: 7,
                base_bps: Bps::from_raw_unchecked(35),
                slope_bps: Bps::from_raw_unchecked(260),
                penalty_bps: Bps::from_raw_unchecked(120),
            },
        }
    }

    pub fn late_settlement() -> Self {
        Self {
            name: "late-settlement".to_string(),
            accounts: vec![
                ScenarioAccount {
                    label: "pool-operator".to_string(),
                    initial_balance: Amount::new(20_000_000_000),
                },
                ScenarioAccount {
                    label: "desk-alpha".to_string(),
                    initial_balance: Amount::new(5_000_000_000),
                },
            ],
            plan: ScenarioPlan {
                name: "late-desk-close".to_string(),
                liquidity: Amount::new(15_000_000_000),
                principal: Amount::new(2_000_000_000),
                collateral: Amount::new(3_000_000_000),
                maturity_offset: 3,
                advance_epochs_before_close: 7,
                base_bps: Bps::from_raw_unchecked(45),
                slope_bps: Bps::from_raw_unchecked(330),
                penalty_bps: Bps::from_raw_unchecked(180),
            },
        }
    }
}

#[derive(Clone, Debug)]
pub struct ScenarioBuilder {
    pub template: ScenarioTemplate,
    pub config: LedgerConfig,
}

impl ScenarioBuilder {
    pub fn new(template: ScenarioTemplate) -> Self {
        Self {
            template,
            config: LedgerConfig::default(),
        }
    }

    pub fn with_config(mut self, config: LedgerConfig) -> Self {
        self.config = config;
        self
    }

    pub fn build(self) -> ChronosResult<ChronosLedger> {
        let mut ledger = ChronosLedger::new(self.config)?;
        let treasury = ledger.create_account(&self.template.accounts[0].label)?;
        let borrower = ledger.create_account(&self.template.accounts[1].label)?;
        let asset = ledger.register_asset("cUSD", 6, treasury)?;
        ledger.deposit(treasury, asset, self.template.accounts[0].initial_balance)?;
        ledger.deposit(borrower, asset, self.template.accounts[1].initial_balance)?;
        let pool = ledger.create_pool(
            asset,
            treasury,
            &self.template.plan.name,
            self.template.plan.rate_model(),
        )?;
        ledger.deposit_liquidity(DepositLiquidityRequest {
            provider: treasury,
            pool,
            amount: self.template.plan.liquidity,
        })?;
        ledger.open_position(OpenPositionRequest {
            borrower,
            pool,
            principal: self.template.plan.principal,
            collateral: self.template.plan.collateral,
            maturity_epoch: Epoch::new(self.template.plan.maturity_offset),
        })?;
        Ok(ledger)
    }

    pub fn catalog() -> Vec<ScenarioTemplate> {
        let mut templates = vec![
            ScenarioTemplate::conservative(),
            ScenarioTemplate::merchant_rollover(),
            ScenarioTemplate::late_settlement(),
        ];
        for idx in 0..36u64 {
            let liquidity = 4_000_000_000u128 + u128::from(idx) * 125_000_000;
            let principal = 400_000_000u128 + u128::from(idx % 9) * 55_000_000;
            let collateral = principal + 250_000_000 + u128::from(idx % 5) * 30_000_000;
            templates.push(ScenarioTemplate {
                name: format!("institutional-window-{idx:02}"),
                accounts: vec![
                    ScenarioAccount {
                        label: format!("provider-{idx:02}"),
                        initial_balance: Amount::new(liquidity + 3_000_000_000),
                    },
                    ScenarioAccount {
                        label: format!("counterparty-{idx:02}"),
                        initial_balance: Amount::new(collateral + 1_500_000_000),
                    },
                ],
                plan: ScenarioPlan {
                    name: format!("receivable-strip-{idx:02}"),
                    liquidity: Amount::new(liquidity),
                    principal: Amount::new(principal),
                    collateral: Amount::new(collateral),
                    maturity_offset: 4 + (idx % 11),
                    advance_epochs_before_close: 2 + (idx % 8),
                    base_bps: Bps::from_raw_unchecked(18 + (idx % 12) as u32),
                    slope_bps: Bps::from_raw_unchecked(140 + (idx % 15) as u32 * 9),
                    penalty_bps: Bps::from_raw_unchecked(70 + (idx % 10) as u32 * 11),
                },
            });
        }
        templates
    }
}
