use cosmwasm_std::{DepsMut, Env, Response, Timestamp};
use osmosis_authenticators::AuthenticationRequest;

use crate::ContractError;

use super::validate_and_parse_params;

pub fn authenticate(
    _deps: DepsMut,
    env: Env,
    auth_request: AuthenticationRequest,
) -> Result<Response, ContractError> {
    let time_limit = validate_and_parse_params(auth_request.authenticator_params)?.time_limit;

    if let Some(time_limit) = time_limit {
        let start = time_limit.start.unwrap_or(Timestamp::from_nanos(0));
        let end = time_limit.end;

        let current = env.block.time;

        if !(start <= current && current <= end) {
            return Err(ContractError::NotWithinTimeLimit {
                current: env.block.time,
                start: time_limit.start,
                end: time_limit.end,
            });
        }
    }

    Ok(Response::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spend_limit::{Period, SpendLimitParams, TimeLimit};
    use cosmwasm_std::{
        testing::{mock_dependencies, mock_env},
        to_json_binary, Addr, Binary, Timestamp,
    };
    use osmosis_authenticators::{Any, SignModeTxData, SignatureData, TxData};
    use rstest::rstest;

    #[rstest]
    #[case::no_time_limit(0, None, true)]
    #[case::no_time_limit(1_771_797_419_879_305_533, None, true)]
    #[case::no_time_limit(u64::MAX, None, true)]
    #[case::within_time_limit(1_771_797_419_879_305_533, Some((Some(current), current + 1)), true)]
    #[case::within_time_limit(1_771_797_419_879_305_533, Some((Some(current), current)), true)]
    #[case::within_time_limit(1_771_797_419_879_305_533, Some((None, current)), true)]
    #[case::not_within_time_limit(1_771_797_419_879_305_533, Some((Some(current), current - 1)), false)]
    #[case::not_within_time_limit(1_771_797_419_879_305_533, Some((Some(current + 1), current)), false)]
    #[case::not_within_time_limit(1_771_797_419_879_305_533, Some((None, current - 1)), false)]
    fn test_authenticate(
        #[case] current: u64,
        #[case] time_limit: Option<(Option<u64>, u64)>,
        #[case] expected: bool,
    ) {
        let mut deps = mock_dependencies();

        let time_limit = time_limit.map(|(start, end)| TimeLimit {
            start: start.map(Timestamp::from_nanos),
            end: Timestamp::from_nanos(end),
        });

        let request = AuthenticationRequest {
            authenticator_id: "2".to_string(),
            account: Addr::unchecked("addr"),
            authenticator_params: Some(
                to_json_binary(&SpendLimitParams {
                    limit: 1000u128.into(),
                    reset_period: Period::Day,
                    time_limit: time_limit.clone(),
                })
                .unwrap(),
            ),
            msg: Any {
                type_url: "".to_string(),
                value: Binary::default(),
            },
            msg_index: 0,
            signature: Binary::default(),
            sign_mode_tx_data: SignModeTxData {
                sign_mode_direct: Binary::default(),
                sign_mode_textual: None,
            },
            tx_data: TxData {
                chain_id: "osmosis-1".to_string(),
                account_number: 0,
                sequence: 0,
                timeout_height: 0,
                msgs: vec![],
                memo: "".to_string(),
            },
            signature_data: SignatureData {
                signers: vec![],
                signatures: vec![],
            },
            simulate: false,
        };

        let mut env = mock_env();

        env.block.time = Timestamp::from_nanos(current);

        let response = authenticate(deps.as_mut(), env.clone(), request);

        if expected {
            response.expect("expected authenticated");
        } else {
            let TimeLimit { start, end } = time_limit.unwrap();
            assert_eq!(
                response.unwrap_err(),
                ContractError::NotWithinTimeLimit {
                    current: env.block.time,
                    start,
                    end,
                }
            );
        }
    }
}
