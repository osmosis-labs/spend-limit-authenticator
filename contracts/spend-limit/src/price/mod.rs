mod error;
pub use error::PriceError;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, Decimal, Deps, DepsMut, StdError, Storage, Timestamp, Uint64};
use cw_storage_plus::{Item, Map};
use osmosis_std::shim::Timestamp as ProtoTimestamp;
use osmosis_std::types::osmosis::{
    poolmanager::v1beta1::SwapAmountInRoute, twap::v1beta1::TwapQuerier,
};
use time::OffsetDateTime;

pub trait PriceSource {
    fn get_and_cache_price(
        &self,
        deps: DepsMut,
        block_time: Timestamp,
        denom: &str,
    ) -> Result<Option<PriceInfo>, PriceError>;
}

#[cw_serde]
pub struct PriceInfo {
    /// Price of the asset
    pub price: Decimal,

    /// Timestamp when the price was last updated
    pub last_updated_time: Timestamp,

    pub swap_routes: Vec<SwapAmountInRoute>,
}

#[cw_serde]
pub struct PriceInfoStoreConfig {
    /// Denom that the price is quoted in
    qoute_denom: String,

    /// Duration in nanoseconds that the price is considered stale.
    /// If the current time is greater than the last_updated_time + staleness_threshold,
    /// the price needs to be updated.
    staleness_threshold: Uint64,

    /// Twap duration in nanoseconds
    twap_duration: Uint64,
}

pub struct PriceInfoStore<'a> {
    config: Item<'a, PriceInfoStoreConfig>,
    price_infos: Map<'a, &'a str, PriceInfo>,
}

impl<'a> PriceInfoStore<'a> {
    pub const fn new(price_info_store_config_key: &'a str, price_infos_key: &'a str) -> Self {
        Self {
            config: Item::new(price_info_store_config_key),
            price_infos: Map::new(price_infos_key),
        }
    }

    pub fn get_config(&self, storage: &dyn Storage) -> Result<PriceInfoStoreConfig, StdError> {
        self.config.load(storage)
    }

    pub fn update_config<A>(
        &self,
        deps: DepsMut,
        action: A,
    ) -> Result<PriceInfoStoreConfig, StdError>
    where
        A: FnOnce(PriceInfoStoreConfig) -> Result<PriceInfoStoreConfig, StdError>,
    {
        self.config.update(deps.storage, action)
    }

    pub fn track_denom(
        &self,
        deps: DepsMut,
        denom: &str,
        block_time: Timestamp,
        swap_routes: Vec<SwapAmountInRoute>,
    ) -> Result<(), PriceError> {
        let price_info = self.fetch_twap_price(deps.as_ref(), denom, block_time, swap_routes)?;
        self.price_infos
            .save(deps.storage, denom, &price_info)
            .map_err(PriceError::StdError)
    }

    // TODO: get conf from params to avoid storage read
    fn fetch_twap_price(
        &self,
        deps: Deps,
        base_denom: &str,
        block_time: Timestamp,
        swap_routes: Vec<SwapAmountInRoute>,
    ) -> Result<PriceInfo, PriceError> {
        let conf = self.get_config(deps.storage)?;

        // Ensure that the swap routes end with the quote denom
        ensure!(
            valid_swap_routes(&swap_routes, &conf.qoute_denom),
            PriceError::SwapRoutesMustEndWithQuoteDenom {
                qoute_denom: conf.qoute_denom,
                swap_routes
            }
        );

        let start_time = block_time.minus_nanos(conf.twap_duration.u64());
        let start_time = to_proto_timestamp(start_time)?;

        let mut base_asset = base_denom.to_string();

        let mut price = Decimal::one();

        for route in swap_routes.iter() {
            let pool_id = route.pool_id;

            let route_price = TwapQuerier::new(&deps.querier).arithmetic_twap_to_now(
                pool_id,
                base_asset,
                conf.qoute_denom.clone(),
                Some(start_time.clone()),
            )?;

            price = price.checked_mul(route_price.arithmetic_twap.parse::<Decimal>()?)?;
            base_asset = route.token_out_denom.clone();
        }

        Ok(PriceInfo {
            price,
            last_updated_time: block_time,
            swap_routes,
        })
    }
}

impl PriceSource for PriceInfoStore<'_> {
    fn get_and_cache_price(
        &self,
        deps: DepsMut,
        block_time: Timestamp,
        denom: &str,
    ) -> Result<Option<PriceInfo>, PriceError> {
        let Some(price_info) = self.price_infos.may_load(deps.storage, denom)? else {
            return Ok(None);
        };
        let price_info =
            self.fetch_twap_price(deps.as_ref(), denom, block_time, price_info.swap_routes)?;

        self.price_infos.save(deps.storage, denom, &price_info)?;
        Ok(Some(price_info))
    }
}

fn to_proto_timestamp(timestamp: Timestamp) -> Result<ProtoTimestamp, PriceError> {
    OffsetDateTime::from_unix_timestamp_nanos(timestamp.nanos() as i128)
        .map(|t| ProtoTimestamp {
            seconds: t.second() as i64,
            nanos: t.nanosecond() as i32,
        })
        .map_err(PriceError::TimestampConversionError)
}

// TODO: check the remaining paths? only if twap does not check that
fn valid_swap_routes(swap_routes: &[SwapAmountInRoute], quote_denom: &str) -> bool {
    if let Some(last_swap_route) = swap_routes.last() {
        last_swap_route.token_out_denom == quote_denom
    } else {
        false
    }
}

// TODO:
// - [x] proper error handling
// - write test
// - documentation
// - remove price oracle contract address
// - wiring
//   - instantiate with price infos
