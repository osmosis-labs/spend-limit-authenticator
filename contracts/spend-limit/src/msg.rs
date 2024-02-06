use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Addr;

use crate::spend_limit::DeprecatedSpendLimit;

#[cw_serde]
pub struct InstantiateMsg {
    pub price_oracle_contract_addr: String,
}

#[cw_serde]
pub enum SudoMsg {
    Authenticate(osmosis_authenticators::AuthenticationRequest),
    Track(osmosis_authenticators::TrackRequest),
    ConfirmExecution(osmosis_authenticators::ConfirmExecutionRequest),
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(SpendLimitDataResponse)]
    GetSpendLimitData { account: Addr },

    #[returns(PriceOracleContractAddrResponse)]
    PriceOracleContractAddr {},
}

#[cw_serde]
pub struct SpendLimitDataResponse {
    pub spend_limit_data: DeprecatedSpendLimit,
}

#[cw_serde]
pub struct PriceOracleContractAddrResponse {
    pub price_oracle_contract_addr: Addr,
}
