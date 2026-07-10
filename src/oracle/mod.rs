use crate::amount::{Amount, Bps};
use crate::error::{ChronosError, ChronosResult};
use crate::ids::{AssetId, Epoch, OperatorId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PriceSource {
    InternalTwap,
    ExternalMedian,
    ManualCommittee,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EpochPrice {
    pub asset: AssetId,
    pub epoch: Epoch,
    pub price_e8: u128,
    pub source: PriceSource,
    pub publisher: Option<OperatorId>,
    pub confidence_bps: Bps,
}

impl EpochPrice {
    pub fn new(
        asset: AssetId,
        epoch: Epoch,
        price_e8: u128,
        source: PriceSource,
    ) -> ChronosResult<Self> {
        if price_e8 == 0 {
            return Err(ChronosError::invalid("price must be non-zero"));
        }
        Ok(Self {
            asset,
            epoch,
            price_e8,
            source,
            publisher: None,
            confidence_bps: Bps::from_raw_unchecked(9_900),
        })
    }

    pub fn value_of(self, amount: Amount) -> ChronosResult<Amount> {
        amount
            .raw()
            .checked_mul(self.price_e8)
            .and_then(|value| value.checked_div(100_000_000))
            .map(Amount::new)
            .ok_or(ChronosError::AmountOverflow)
    }

    pub fn with_publisher(mut self, publisher: OperatorId) -> Self {
        self.publisher = Some(publisher);
        self
    }

    pub fn with_confidence(mut self, confidence_bps: Bps) -> Self {
        self.confidence_bps = confidence_bps;
        self
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PriceBand {
    pub asset: AssetId,
    pub lower_e8: u128,
    pub upper_e8: u128,
    pub max_staleness_epochs: u64,
}

impl PriceBand {
    pub fn new(asset: AssetId, lower_e8: u128, upper_e8: u128) -> ChronosResult<Self> {
        if lower_e8 == 0 || upper_e8 < lower_e8 {
            return Err(ChronosError::invalid("invalid price band"));
        }
        Ok(Self {
            asset,
            lower_e8,
            upper_e8,
            max_staleness_epochs: 3,
        })
    }

    pub fn accepts(self, price: EpochPrice, now: Epoch) -> bool {
        price.asset == self.asset
            && price.price_e8 >= self.lower_e8
            && price.price_e8 <= self.upper_e8
            && price.epoch.distance_to(now) <= self.max_staleness_epochs
    }

    pub fn midpoint_e8(self) -> u128 {
        self.lower_e8 + (self.upper_e8 - self.lower_e8) / 2
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PriceQuote {
    pub base_asset: AssetId,
    pub quote_asset: AssetId,
    pub base_amount: Amount,
    pub quote_amount: Amount,
    pub base_price_e8: u128,
    pub quote_price_e8: u128,
    pub epoch: Epoch,
}

impl PriceQuote {
    pub fn implied_rate_e8(self) -> ChronosResult<u128> {
        if self.base_amount.is_zero() {
            return Err(ChronosError::AmountOverflow);
        }
        self.quote_amount
            .raw()
            .checked_mul(100_000_000)
            .and_then(|value| value.checked_div(self.base_amount.raw()))
            .ok_or(ChronosError::AmountOverflow)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OracleBook {
    prices: HashMap<AssetId, Vec<EpochPrice>>,
    bands: HashMap<AssetId, PriceBand>,
}

impl OracleBook {
    pub fn set_band(&mut self, band: PriceBand) {
        self.bands.insert(band.asset, band);
    }

    pub fn publish(&mut self, price: EpochPrice, now: Epoch) -> ChronosResult<()> {
        if let Some(band) = self.bands.get(&price.asset).copied()
            && !band.accepts(price, now)
        {
            return Err(ChronosError::risk("price outside configured band"));
        }
        let feed = self.prices.entry(price.asset).or_default();
        feed.push(price);
        feed.sort_by_key(|price| price.epoch);
        Ok(())
    }

    pub fn latest(&self, asset: AssetId) -> ChronosResult<EpochPrice> {
        self.prices
            .get(&asset)
            .and_then(|feed| feed.last().copied())
            .ok_or_else(|| ChronosError::invalid("missing price"))
    }

    pub fn at_or_before(&self, asset: AssetId, epoch: Epoch) -> ChronosResult<EpochPrice> {
        self.prices
            .get(&asset)
            .and_then(|feed| {
                feed.iter()
                    .rev()
                    .copied()
                    .find(|price| price.epoch <= epoch)
            })
            .ok_or_else(|| ChronosError::invalid("missing price for epoch"))
    }

    pub fn quote(
        &self,
        base_asset: AssetId,
        quote_asset: AssetId,
        base_amount: Amount,
        epoch: Epoch,
    ) -> ChronosResult<PriceQuote> {
        let base_price = self.at_or_before(base_asset, epoch)?;
        let quote_price = self.at_or_before(quote_asset, epoch)?;
        let value_e8 = base_amount
            .raw()
            .checked_mul(base_price.price_e8)
            .ok_or(ChronosError::AmountOverflow)?;
        let quote_amount = value_e8
            .checked_div(quote_price.price_e8)
            .map(Amount::new)
            .ok_or(ChronosError::AmountOverflow)?;
        Ok(PriceQuote {
            base_asset,
            quote_asset,
            base_amount,
            quote_amount,
            base_price_e8: base_price.price_e8,
            quote_price_e8: quote_price.price_e8,
            epoch,
        })
    }
}
