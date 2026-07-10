use crate::amount::{Amount, Bps};
use crate::error::{ChronosError, ChronosResult};
use crate::ids::{AssetId, Epoch, PoolId};
use crate::pools::PoolSnapshot;
use crate::portfolio::{PoolExposureRollup, PortfolioReport};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum MetricKind {
    Liquidity,
    Principal,
    Collateral,
    Interest,
    Penalty,
    Utilization,
    Reserve,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EpochMetric {
    pub epoch: Epoch,
    pub pool: PoolId,
    pub asset: AssetId,
    pub kind: MetricKind,
    pub amount: Amount,
    pub bps: Bps,
}

impl EpochMetric {
    pub fn amount(
        epoch: Epoch,
        pool: PoolId,
        asset: AssetId,
        kind: MetricKind,
        amount: Amount,
    ) -> Self {
        Self {
            epoch,
            pool,
            asset,
            kind,
            amount,
            bps: Bps::ZERO,
        }
    }

    pub fn ratio(epoch: Epoch, pool: PoolId, asset: AssetId, kind: MetricKind, bps: Bps) -> Self {
        Self {
            epoch,
            pool,
            asset,
            kind,
            amount: Amount::ZERO,
            bps,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct PoolMetricSeries {
    pub pool: PoolId,
    pub asset: AssetId,
    pub metrics: Vec<EpochMetric>,
}

impl PoolMetricSeries {
    pub fn new(pool: PoolId, asset: AssetId) -> Self {
        Self {
            pool,
            asset,
            metrics: Vec::new(),
        }
    }

    pub fn push_snapshot(&mut self, epoch: Epoch, snapshot: PoolSnapshot) {
        self.metrics.push(EpochMetric::amount(
            epoch,
            snapshot.id,
            snapshot.asset,
            MetricKind::Liquidity,
            snapshot.liquidity_available,
        ));
        self.metrics.push(EpochMetric::amount(
            epoch,
            snapshot.id,
            snapshot.asset,
            MetricKind::Principal,
            snapshot.principal_outstanding,
        ));
        self.metrics.push(EpochMetric::amount(
            epoch,
            snapshot.id,
            snapshot.asset,
            MetricKind::Collateral,
            snapshot.collateral_locked,
        ));
        self.metrics.push(EpochMetric::amount(
            epoch,
            snapshot.id,
            snapshot.asset,
            MetricKind::Interest,
            snapshot.interest_collected,
        ));
        self.metrics.push(EpochMetric::amount(
            epoch,
            snapshot.id,
            snapshot.asset,
            MetricKind::Penalty,
            snapshot.penalty_collected,
        ));
        self.metrics.push(EpochMetric::amount(
            epoch,
            snapshot.id,
            snapshot.asset,
            MetricKind::Reserve,
            snapshot.reserve_balance,
        ));
        self.metrics.push(EpochMetric::ratio(
            epoch,
            snapshot.id,
            snapshot.asset,
            MetricKind::Utilization,
            snapshot.utilization_bps,
        ));
    }

    pub fn latest(&self, kind: MetricKind) -> Option<EpochMetric> {
        self.metrics
            .iter()
            .rev()
            .copied()
            .find(|metric| metric.kind == kind)
    }

    pub fn amount_delta(&self, kind: MetricKind, from: Epoch, to: Epoch) -> ChronosResult<Amount> {
        let start = self
            .metrics
            .iter()
            .find(|metric| metric.kind == kind && metric.epoch >= from)
            .copied()
            .ok_or_else(|| ChronosError::invalid("metric start not found"))?;
        let end = self
            .metrics
            .iter()
            .rev()
            .find(|metric| metric.kind == kind && metric.epoch <= to)
            .copied()
            .ok_or_else(|| ChronosError::invalid("metric end not found"))?;
        Ok(end.amount.saturating_sub(start.amount))
    }

    pub fn by_epoch(&self) -> BTreeMap<Epoch, Vec<EpochMetric>> {
        let mut map = BTreeMap::new();
        for metric in &self.metrics {
            map.entry(metric.epoch)
                .or_insert_with(Vec::new)
                .push(*metric);
        }
        map
    }

    pub fn window(&self, start: Epoch, end: Epoch) -> ChronosResult<MetricWindow> {
        MetricWindow::new(
            self.pool,
            self.asset,
            start,
            end,
            self.metrics.iter().copied(),
        )
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct MetricWindow {
    pub pool: PoolId,
    pub asset: AssetId,
    pub start: Epoch,
    pub end: Epoch,
    pub metrics: Vec<EpochMetric>,
}

impl MetricWindow {
    pub fn new<I>(
        pool: PoolId,
        asset: AssetId,
        start: Epoch,
        end: Epoch,
        metrics: I,
    ) -> ChronosResult<Self>
    where
        I: IntoIterator<Item = EpochMetric>,
    {
        if end < start {
            return Err(ChronosError::EpochOutOfRange(end));
        }
        let mut metrics = metrics
            .into_iter()
            .filter(|metric| metric.epoch >= start && metric.epoch <= end)
            .collect::<Vec<_>>();
        metrics.sort_by(|left, right| {
            left.epoch
                .cmp(&right.epoch)
                .then(left.kind.cmp(&right.kind))
        });
        Ok(Self {
            pool,
            asset,
            start,
            end,
            metrics,
        })
    }

    pub fn epochs(&self) -> Vec<Epoch> {
        let mut epochs = self
            .metrics
            .iter()
            .map(|metric| metric.epoch)
            .collect::<Vec<_>>();
        epochs.sort();
        epochs.dedup();
        epochs
    }

    pub fn sum_amount(&self, kind: MetricKind) -> Amount {
        self.metrics
            .iter()
            .filter(|metric| metric.kind == kind)
            .map(|metric| metric.amount)
            .sum()
    }

    pub fn last_ratio(&self, kind: MetricKind) -> Option<Bps> {
        self.metrics
            .iter()
            .rev()
            .find(|metric| metric.kind == kind)
            .map(|metric| metric.bps)
    }

    pub fn is_empty(&self) -> bool {
        self.metrics.is_empty()
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DelinquencyBucket {
    pub start_epoch_offset: u64,
    pub end_epoch_offset: u64,
    pub principal: Amount,
    pub positions: usize,
}

impl DelinquencyBucket {
    pub fn new(start_epoch_offset: u64, end_epoch_offset: u64) -> Self {
        Self {
            start_epoch_offset,
            end_epoch_offset,
            principal: Amount::ZERO,
            positions: 0,
        }
    }

    pub fn accepts(self, late_by_epochs: u64) -> bool {
        self.start_epoch_offset <= late_by_epochs && late_by_epochs <= self.end_epoch_offset
    }

    pub fn add(&mut self, principal: Amount) {
        self.principal += principal;
        self.positions = self.positions.saturating_add(1);
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AnalyticsReport {
    pub generated_epoch: Epoch,
    pub pool_series: Vec<PoolMetricSeries>,
    pub delinquency: Vec<DelinquencyBucket>,
    pub total_principal: Amount,
    pub total_pending_charges: Amount,
}

impl AnalyticsReport {
    pub fn new(generated_epoch: Epoch) -> Self {
        Self {
            generated_epoch,
            pool_series: Vec::new(),
            delinquency: vec![
                DelinquencyBucket::new(1, 1),
                DelinquencyBucket::new(2, 3),
                DelinquencyBucket::new(4, 7),
                DelinquencyBucket::new(8, u64::MAX),
            ],
            total_principal: Amount::ZERO,
            total_pending_charges: Amount::ZERO,
        }
    }

    pub fn push_pool_snapshot(&mut self, epoch: Epoch, snapshot: PoolSnapshot) {
        let series = self
            .pool_series
            .iter_mut()
            .find(|series| series.pool == snapshot.id)
            .map(|series| {
                series.push_snapshot(epoch, snapshot);
            });
        if series.is_none() {
            let mut created = PoolMetricSeries::new(snapshot.id, snapshot.asset);
            created.push_snapshot(epoch, snapshot);
            self.pool_series.push(created);
        }
    }

    pub fn ingest_portfolio(&mut self, portfolio: &PortfolioReport) -> ChronosResult<()> {
        self.total_principal = portfolio.total_principal();
        self.total_pending_charges = portfolio
            .pool_rollups
            .iter()
            .try_fold(Amount::ZERO, |acc, rollup| {
                acc.checked_add(rollup.total_pending()?)
            })?;
        for view in &portfolio.views {
            if view.is_late_at(self.generated_epoch) {
                let late_by = view
                    .effective_maturity_epoch
                    .distance_to(self.generated_epoch);
                if let Some(bucket) = self
                    .delinquency
                    .iter_mut()
                    .find(|bucket| bucket.accepts(late_by))
                {
                    bucket.add(view.principal);
                }
            }
        }
        Ok(())
    }

    pub fn rollup_for_pool(&self, pool: PoolId) -> Option<&PoolMetricSeries> {
        self.pool_series.iter().find(|series| series.pool == pool)
    }

    pub fn principal_at_risk(&self) -> Amount {
        self.delinquency.iter().map(|bucket| bucket.principal).sum()
    }

    pub fn delinquency_ratio_bps(&self) -> ChronosResult<Bps> {
        if self.total_principal.is_zero() {
            return Ok(Bps::ZERO);
        }
        let raw = self
            .principal_at_risk()
            .raw()
            .checked_mul(10_000)
            .and_then(|value| value.checked_div(self.total_principal.raw()))
            .ok_or(ChronosError::AmountOverflow)?;
        Bps::new(raw as u32)
    }

    pub fn utilization_extremes(&self) -> Option<(Bps, Bps)> {
        let mut values = self
            .pool_series
            .iter()
            .flat_map(|series| series.metrics.iter())
            .filter(|metric| metric.kind == MetricKind::Utilization)
            .map(|metric| metric.bps);
        let first = values.next()?;
        let mut min = first;
        let mut max = first;
        for value in values {
            if value < min {
                min = value;
            }
            if value > max {
                max = value;
            }
        }
        Some((min, max))
    }

    pub fn merge(mut self, other: AnalyticsReport) -> ChronosResult<Self> {
        for series in other.pool_series {
            match self
                .pool_series
                .iter_mut()
                .find(|local| local.pool == series.pool)
            {
                Some(local) => local.metrics.extend(series.metrics),
                None => self.pool_series.push(series),
            }
        }
        self.total_principal = self.total_principal.checked_add(other.total_principal)?;
        self.total_pending_charges = self
            .total_pending_charges
            .checked_add(other.total_pending_charges)?;
        for other_bucket in other.delinquency {
            if let Some(bucket) = self.delinquency.iter_mut().find(|bucket| {
                bucket.start_epoch_offset == other_bucket.start_epoch_offset
                    && bucket.end_epoch_offset == other_bucket.end_epoch_offset
            }) {
                bucket.principal = bucket.principal.checked_add(other_bucket.principal)?;
                bucket.positions = bucket.positions.saturating_add(other_bucket.positions);
            } else {
                self.delinquency.push(other_bucket);
            }
        }
        Ok(self)
    }
}

pub fn rollups_to_metrics(epoch: Epoch, rollups: &[PoolExposureRollup]) -> Vec<PoolMetricSeries> {
    rollups
        .iter()
        .map(|rollup| {
            let mut series = PoolMetricSeries::new(rollup.pool, rollup.asset);
            series.metrics.push(EpochMetric::amount(
                epoch,
                rollup.pool,
                rollup.asset,
                MetricKind::Principal,
                rollup.principal,
            ));
            series.metrics.push(EpochMetric::amount(
                epoch,
                rollup.pool,
                rollup.asset,
                MetricKind::Collateral,
                rollup.collateral,
            ));
            series.metrics.push(EpochMetric::amount(
                epoch,
                rollup.pool,
                rollup.asset,
                MetricKind::Interest,
                rollup.pending_interest,
            ));
            series.metrics.push(EpochMetric::amount(
                epoch,
                rollup.pool,
                rollup.asset,
                MetricKind::Penalty,
                rollup.pending_penalty,
            ));
            series.metrics.push(EpochMetric::ratio(
                epoch,
                rollup.pool,
                rollup.asset,
                MetricKind::Utilization,
                rollup.utilization_bps,
            ));
            series
        })
        .collect()
}
