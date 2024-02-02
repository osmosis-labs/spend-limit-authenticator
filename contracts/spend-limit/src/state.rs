use cosmwasm_schema::cw_serde;
use cw_storage_plus::Map;
use osmosis_std::types::osmosis::poolmanager::v1beta1::SwapAmountInRoute;

use crate::spend_limit::{DeprecatedSpendLimit, SpendingStorage};

pub const USDC_DENOM: &str = "ibc/498A0751C798A0D9A389AA3691123DADA57DAA4FE165D5C75894505B876BA6E4";
pub const TRACKED_DENOMS_IN_MEMORY: &str = "TBD";

pub const DEPRECATED_SPEND_LIMITS: Map<String, DeprecatedSpendLimit> = Map::new("sls");
pub const TRACKED_DENOMS: Map<Denom, TrackedDenom> = Map::new("tds");

#[cw_serde]
pub struct TrackedDenom {
    pub denom: Denom,
    pub path: Path,
}

pub type Denom = String;
pub type Path = Vec<SwapAmountInRoute>;

pub const SPENDINGS: SpendingStorage<'_> = Map::new("spendings");
