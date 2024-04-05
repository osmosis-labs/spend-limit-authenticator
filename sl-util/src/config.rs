use serde::{Deserialize, Serialize};
use spend_limit::price::PriceResolutionConfig;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    /// The price resolution config used directly in the instantiate msg
    pub price_resolution: PriceResolutionConfig,

    /// The denoms to track, used for calculating route via sqs
    pub tracked_denoms: Vec<String>,
}
