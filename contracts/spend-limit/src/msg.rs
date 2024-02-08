use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Addr;

use crate::spend_limit::Spending;

#[cw_serde]
pub struct InstantiateMsg {
    pub price_oracle_contract_addr: String,
}

#[cw_serde]
pub enum SudoMsg {
    OnAuthenticatorAdded(osmosis_authenticators::OnAuthenticatorAddedRequest),
    Authenticate(osmosis_authenticators::AuthenticationRequest),
    Track(osmosis_authenticators::TrackRequest),
    ConfirmExecution(osmosis_authenticators::ConfirmExecutionRequest),
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(SpendingResponse)]
    Spending { account: Addr, subkey: String },

    #[returns(PriceOracleContractAddrResponse)]
    PriceOracleContractAddr {},
}

#[cw_serde]
pub struct SpendingResponse {
    pub spending: Spending,
}

#[cw_serde]
pub struct PriceOracleContractAddrResponse {
    pub price_oracle_contract_addr: Addr,
}
