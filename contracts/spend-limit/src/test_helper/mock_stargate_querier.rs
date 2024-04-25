#![cfg(test)]

use std::marker::PhantomData;

use cosmwasm_std::{
    from_json,
    testing::{MockApi, MockQuerier, MockStorage},
    to_json_binary, Binary, Coin, ContractResult, CustomQuery, Empty, OwnedDeps, Querier,
    QuerierResult, QuerierWrapper, QueryRequest, SystemError, SystemResult,
};
use osmosis_std::types::osmosis::{
    smartaccount::v1beta1::{GetAuthenticatorRequest, GetAuthenticatorResponse},
    twap::v1beta1::{ArithmeticTwapToNowRequest, ArithmeticTwapToNowResponse, TwapQuerier},
};
use serde::{de::DeserializeOwned, Serialize};

type QueryHandler = Box<dyn Fn(String, Binary) -> QuerierResult>;

pub struct MockStargateQuerier<C: DeserializeOwned = Empty> {
    mock_querier: MockQuerier<C>,
    stargate_query_handler: Option<QueryHandler>,
}

impl<C: DeserializeOwned> MockStargateQuerier<C> {
    pub fn new(balances: &[(&str, &[Coin])]) -> Self {
        MockStargateQuerier {
            mock_querier: MockQuerier::new(balances),
            stargate_query_handler: None,
        }
    }

    pub fn with_stargate_handler(mut self, handler: QueryHandler) -> MockStargateQuerier<C> {
        self.stargate_query_handler = Some(handler);
        self
    }

    pub fn update_balance(&mut self, addr: impl Into<String>, balance: Vec<Coin>) {
        self.mock_querier.update_balance(addr, balance);
    }
}

impl<C: CustomQuery + DeserializeOwned> Querier for MockStargateQuerier<C> {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        let request: QueryRequest<C> = match from_json(bin_request) {
            Ok(v) => v,
            Err(e) => {
                return SystemResult::Err(SystemError::InvalidRequest {
                    error: format!("Parsing query request: {e}"),
                    request: bin_request.into(),
                })
            }
        };

        match request {
            QueryRequest::Stargate { path, data } => {
                if let Some(handler) = &self.stargate_query_handler {
                    handler(path, data)
                } else {
                    SystemResult::Err(SystemError::UnsupportedRequest {
                        kind: "Stargate".to_string(),
                    })
                }
            }
            _ => self.mock_querier.handle_query(&request),
        }
    }
}

pub fn mock_dependencies_with_stargate_querier(
    balances: &[(&str, &[Coin])],
    stargate_query_handler: QueryHandler,
) -> OwnedDeps<MockStorage, MockApi, MockStargateQuerier, Empty> {
    OwnedDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier: MockStargateQuerier::new(balances).with_stargate_handler(stargate_query_handler),
        custom_query_type: PhantomData,
    }
}

pub fn arithmetic_twap_to_now_query_handler(
    req_mapper: Box<
        dyn Fn(ArithmeticTwapToNowRequest) -> ContractResult<ArithmeticTwapToNowResponse>,
    >,
) -> QueryHandler {
    make_query_handler(
        "/osmosis.twap.v1beta1.Query/ArithmeticTwapToNow",
        req_mapper,
    )
}

pub fn get_authenticator_query_handler(
    req_mapper: Box<dyn Fn(GetAuthenticatorRequest) -> ContractResult<GetAuthenticatorResponse>>,
) -> QueryHandler {
    make_query_handler(
        "/osmosis.smartaccount.v1beta1.Query/GetAuthenticator",
        req_mapper,
    )
}

fn make_query_handler<Req, Res>(
    target_path: &'static str,
    req_mapper: Box<dyn Fn(Req) -> ContractResult<Res>>,
) -> QueryHandler
where
    Req: TryFrom<Binary, Error = cosmwasm_std::StdError> + 'static,
    Res: Serialize + 'static,
{
    Box::new(move |path: String, data: Binary| match path.as_str() {
        p if p == target_path => {
            let request = match Req::try_from(data.clone()) {
                Ok(v) => v,
                Err(e) => {
                    return SystemResult::Err(SystemError::InvalidRequest {
                        error: e.to_string(),
                        request: data,
                    })
                }
            };

            let res = req_mapper(request);

            match res {
                ContractResult::Ok(v) => SystemResult::Ok(to_json_binary(&v).into()),
                ContractResult::Err(e) => SystemResult::Err(SystemError::InvalidRequest {
                    error: e,
                    request: data,
                }),
            }
        }
        _ => SystemResult::Err(SystemError::UnsupportedRequest { kind: path }),
    })
}

#[test]
fn test_stargate_handler() {
    let deps = mock_dependencies_with_stargate_querier(
        &[],
        arithmetic_twap_to_now_query_handler(Box::new(|req| {
            let base_asset = req.base_asset.as_str();
            let quote_asset = req.quote_asset.as_str();

            let arithmetic_twap = match (base_asset, quote_asset) {
                ("uatom", "uosmo") => "1",
                ("uion", "uosmo") => "2",
                _ => return ContractResult::Err("Price not found".to_string()),
            }
            .to_string();

            ContractResult::Ok(ArithmeticTwapToNowResponse { arithmetic_twap })
        })),
    );

    let queier_wrapper: QuerierWrapper<'_, Empty> = QuerierWrapper::new(&deps.querier);
    let twap_querier = TwapQuerier::new(&queier_wrapper);

    let atom_osmo_price = twap_querier
        .arithmetic_twap_to_now(
            1,
            "uatom".to_string(),
            "uosmo".to_string(),
            Some(osmosis_std::shim::Timestamp {
                seconds: 0,
                nanos: 0,
            }),
        )
        .unwrap()
        .arithmetic_twap;

    assert_eq!(atom_osmo_price, "1".to_string());

    let atom_ion_price = twap_querier
        .arithmetic_twap_to_now(
            1,
            "uion".to_string(),
            "uosmo".to_string(),
            Some(osmosis_std::shim::Timestamp {
                seconds: 0,
                nanos: 0,
            }),
        )
        .unwrap()
        .arithmetic_twap;

    assert_eq!(atom_ion_price, "2".to_string());
}
