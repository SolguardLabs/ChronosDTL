use crate::amount::{Amount, BPS_DENOMINATOR, Bps};
use crate::error::{ChronosError, ChronosResult};
use crate::ids::{AccountId, Epoch, PoolId, PositionId};
use serde::{Deserialize, Serialize};

pub const MAX_STRESS_HORIZON_EPOCHS: u64 = 365;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TemporalStressPolicy {
    pub collateral_haircut_bps: Bps,
    pub liquidation_cost_bps: Bps,
    pub rate_shock_bps_per_epoch: Bps,
    pub concentration_addon_bps: Bps,
    pub target_coverage_bps: Bps,
    pub horizon_epochs: u64,
    pub operational_buffer: Amount,
}

impl TemporalStressPolicy {
    pub fn validate(self) -> ChronosResult<Self> {
        let collateral_deduction = self
            .collateral_haircut_bps
            .raw()
            .checked_add(self.liquidation_cost_bps.raw())
            .ok_or(ChronosError::AmountOverflow)?;
        if collateral_deduction > 10_000 {
            return Err(ChronosError::risk(
                "collateral haircut and liquidation cost exceed full collateral value",
            ));
        }
        if self.target_coverage_bps.raw() < 10_000 || self.target_coverage_bps.raw() > 30_000 {
            return Err(ChronosError::risk(
                "target coverage must be between 10000 and 30000 bps",
            ));
        }
        if self.horizon_epochs == 0 || self.horizon_epochs > MAX_STRESS_HORIZON_EPOCHS {
            return Err(ChronosError::risk(
                "stress horizon must be between 1 and 365 epochs",
            ));
        }
        Ok(self)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TemporalStressPosition {
    pub position: PositionId,
    pub borrower: AccountId,
    pub pool: PoolId,
    pub principal: Amount,
    pub quoted_interest: Amount,
    pub quoted_penalty: Amount,
    pub collateral: Amount,
    pub maturity_epoch: Epoch,
}

impl TemporalStressPosition {
    pub fn claim(self) -> ChronosResult<Amount> {
        self.principal
            .checked_add(self.quoted_interest)?
            .checked_add(self.quoted_penalty)
    }

    pub fn validate(self) -> ChronosResult<Self> {
        self.principal.non_zero()?;
        self.collateral.non_zero()?;
        Ok(self)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PoolStressInput {
    pub pool: PoolId,
    pub available_liquidity: Amount,
    pub reserve_balance: Amount,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PoolStressReport {
    pub pool: PoolId,
    pub position_count: usize,
    pub gross_claim: Amount,
    pub eligible_collateral: Amount,
    pub projected_interest: Amount,
    pub concentration_addon: Amount,
    pub stressed_obligation: Amount,
    pub eligible_resources: Amount,
    pub required_coverage: Amount,
    pub surplus: Amount,
    pub shortfall: Amount,
    pub coverage_bps: Bps,
    pub largest_borrower_share_bps: Bps,
    pub hhi_bps: Bps,
    pub weighted_maturity_milli_epochs: u128,
    pub policy_satisfied: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TemporalStressReport {
    pub generated_epoch: Epoch,
    pub policy: TemporalStressPolicy,
    pub pools: Vec<PoolStressReport>,
    pub total_gross_claim: Amount,
    pub total_eligible_resources: Amount,
    pub total_required_coverage: Amount,
    pub total_surplus: Amount,
    pub total_shortfall: Amount,
    pub policy_satisfied: bool,
}

fn mul_ratio_floor(value: Amount, numerator: u128, denominator: u128) -> ChronosResult<Amount> {
    value
        .raw()
        .checked_mul(numerator)
        .and_then(|product| product.checked_div(denominator))
        .map(Amount::new)
        .ok_or(ChronosError::AmountOverflow)
}

fn mul_ratio_ceil(value: Amount, numerator: u128, denominator: u128) -> ChronosResult<Amount> {
    if denominator == 0 {
        return Err(ChronosError::AmountOverflow);
    }
    let product = value
        .raw()
        .checked_mul(numerator)
        .ok_or(ChronosError::AmountOverflow)?;
    let adjusted = product
        .checked_add(denominator - 1)
        .ok_or(ChronosError::AmountOverflow)?;
    Ok(Amount::new(adjusted / denominator))
}

fn ratio_bps(numerator: Amount, denominator: Amount) -> ChronosResult<Bps> {
    if denominator.is_zero() {
        return Ok(if numerator.is_zero() {
            Bps::from_raw_unchecked(10_000)
        } else {
            Bps::from_raw_unchecked(30_000)
        });
    }
    let raw = numerator
        .raw()
        .checked_mul(BPS_DENOMINATOR)
        .and_then(|value| value.checked_div(denominator.raw()))
        .ok_or(ChronosError::AmountOverflow)?;
    Bps::new(raw.min(100_000) as u32)
}

#[derive(Clone, Debug)]
pub struct TemporalStressEngine {
    policy: TemporalStressPolicy,
}

impl TemporalStressEngine {
    pub fn new(policy: TemporalStressPolicy) -> ChronosResult<Self> {
        Ok(Self {
            policy: policy.validate()?,
        })
    }

    pub fn policy(&self) -> TemporalStressPolicy {
        self.policy
    }

    pub fn evaluate(
        &self,
        generated_epoch: Epoch,
        pool_inputs: &[PoolStressInput],
        positions: &[TemporalStressPosition],
    ) -> ChronosResult<TemporalStressReport> {
        if pool_inputs.is_empty() {
            return Err(ChronosError::invalid(
                "at least one pool stress input is required",
            ));
        }
        let mut reports = Vec::with_capacity(pool_inputs.len());
        for input in pool_inputs {
            if reports
                .iter()
                .any(|report: &PoolStressReport| report.pool == input.pool)
            {
                return Err(ChronosError::invalid("duplicate pool stress input"));
            }
            reports.push(self.evaluate_pool(generated_epoch, *input, positions)?);
        }
        for position in positions {
            position.validate()?;
            if !pool_inputs.iter().any(|input| input.pool == position.pool) {
                return Err(ChronosError::invalid(
                    "stress position references a pool without liquidity input",
                ));
            }
        }
        reports.sort_by_key(|report| report.pool);

        let total_gross_claim = reports.iter().try_fold(Amount::ZERO, |sum, report| {
            sum.checked_add(report.gross_claim)
        })?;
        let total_eligible_resources = reports.iter().try_fold(Amount::ZERO, |sum, report| {
            sum.checked_add(report.eligible_resources)
        })?;
        let total_required_coverage = reports.iter().try_fold(Amount::ZERO, |sum, report| {
            sum.checked_add(report.required_coverage)
        })?;
        let total_surplus = total_eligible_resources.saturating_sub(total_required_coverage);
        let total_shortfall = total_required_coverage.saturating_sub(total_eligible_resources);
        let policy_satisfied =
            total_shortfall.is_zero() && reports.iter().all(|report| report.policy_satisfied);
        Ok(TemporalStressReport {
            generated_epoch,
            policy: self.policy,
            pools: reports,
            total_gross_claim,
            total_eligible_resources,
            total_required_coverage,
            total_surplus,
            total_shortfall,
            policy_satisfied,
        })
    }

    fn evaluate_pool(
        &self,
        generated_epoch: Epoch,
        input: PoolStressInput,
        positions: &[TemporalStressPosition],
    ) -> ChronosResult<PoolStressReport> {
        let pool_positions = positions
            .iter()
            .copied()
            .filter(|position| position.pool == input.pool)
            .collect::<Vec<_>>();
        for position in &pool_positions {
            position.validate()?;
        }

        let gross_claim = pool_positions
            .iter()
            .try_fold(Amount::ZERO, |sum, position| {
                sum.checked_add(position.claim()?)
            })?;
        let collateral = pool_positions
            .iter()
            .try_fold(Amount::ZERO, |sum, position| {
                sum.checked_add(position.collateral)
            })?;
        let collateral_deduction = self
            .policy
            .collateral_haircut_bps
            .raw()
            .checked_add(self.policy.liquidation_cost_bps.raw())
            .ok_or(ChronosError::AmountOverflow)?;
        let eligible_collateral = mul_ratio_floor(
            collateral,
            u128::from(10_000u32 - collateral_deduction),
            BPS_DENOMINATOR,
        )?;
        let shock_rate = u128::from(self.policy.rate_shock_bps_per_epoch.raw())
            .checked_mul(u128::from(self.policy.horizon_epochs))
            .ok_or(ChronosError::AmountOverflow)?;
        let projected_interest = mul_ratio_ceil(gross_claim, shock_rate, BPS_DENOMINATOR)?;

        let mut borrower_claims = std::collections::BTreeMap::<AccountId, Amount>::new();
        for position in &pool_positions {
            let claim = position.claim()?;
            borrower_claims
                .entry(position.borrower)
                .and_modify(|value| *value += claim)
                .or_insert(claim);
        }
        let largest_claim = borrower_claims
            .values()
            .copied()
            .max()
            .unwrap_or(Amount::ZERO);
        let largest_borrower_share_bps = if gross_claim.is_zero() {
            Bps::ZERO
        } else {
            ratio_bps(largest_claim, gross_claim)?
        };
        let concentration_addon = largest_claim.ceil_bps(self.policy.concentration_addon_bps)?;

        let stressed_obligation = gross_claim
            .checked_add(projected_interest)?
            .checked_add(concentration_addon)?;
        let required_coverage = mul_ratio_ceil(
            stressed_obligation,
            u128::from(self.policy.target_coverage_bps.raw()),
            BPS_DENOMINATOR,
        )?
        .checked_add(self.policy.operational_buffer)?;
        let eligible_resources = input
            .available_liquidity
            .checked_add(input.reserve_balance)?
            .checked_add(eligible_collateral)?;
        let surplus = eligible_resources.saturating_sub(required_coverage);
        let shortfall = required_coverage.saturating_sub(eligible_resources);
        let coverage_bps = ratio_bps(eligible_resources, required_coverage)?;

        let hhi_raw = if gross_claim.is_zero() {
            0u128
        } else {
            borrower_claims.values().try_fold(0u128, |sum, claim| {
                let share = claim
                    .raw()
                    .checked_mul(BPS_DENOMINATOR)
                    .and_then(|value| value.checked_div(gross_claim.raw()))
                    .ok_or(ChronosError::AmountOverflow)?;
                let component = share
                    .checked_mul(share)
                    .and_then(|value| value.checked_div(BPS_DENOMINATOR))
                    .ok_or(ChronosError::AmountOverflow)?;
                sum.checked_add(component)
                    .ok_or(ChronosError::AmountOverflow)
            })?
        };
        let hhi_bps = Bps::new(hhi_raw.min(10_000) as u32)?;

        let weighted_maturity_milli_epochs = if gross_claim.is_zero() {
            0
        } else {
            let numerator = pool_positions.iter().try_fold(0u128, |sum, position| {
                let distance = generated_epoch.distance_to(position.maturity_epoch);
                let weighted = position
                    .claim()?
                    .raw()
                    .checked_mul(u128::from(distance))
                    .and_then(|value| value.checked_mul(1_000))
                    .ok_or(ChronosError::AmountOverflow)?;
                sum.checked_add(weighted)
                    .ok_or(ChronosError::AmountOverflow)
            })?;
            numerator / gross_claim.raw()
        };

        Ok(PoolStressReport {
            pool: input.pool,
            position_count: pool_positions.len(),
            gross_claim,
            eligible_collateral,
            projected_interest,
            concentration_addon,
            stressed_obligation,
            eligible_resources,
            required_coverage,
            surplus,
            shortfall,
            coverage_bps,
            largest_borrower_share_bps,
            hhi_bps,
            weighted_maturity_milli_epochs,
            policy_satisfied: shortfall.is_zero(),
        })
    }
}
