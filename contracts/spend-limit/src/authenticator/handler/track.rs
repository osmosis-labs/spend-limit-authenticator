use cosmwasm_std::{Addr, DepsMut, Env, Response};

use osmosis_authenticators::TrackRequest;

use crate::state::PRE_EXEC_BALANCES;

use crate::authenticator::error::AuthenticatorResult;

pub fn track(
    deps: DepsMut,
    _env: Env,
    TrackRequest {
        account,
        authenticator_id,
        ..
    }: TrackRequest,
) -> AuthenticatorResult<Response> {
    update_pre_exec_balance(deps, &account, authenticator_id.as_str())?;
    Ok(Response::new().add_attribute("action", "track"))
}

fn update_pre_exec_balance(
    deps: DepsMut,
    account: &Addr,
    authenticator_id: &str,
) -> AuthenticatorResult<()> {
    // query all the balances of the account
    let balances = deps.querier.query_all_balances(account)?;

    // make sure the pre-exec balance is cleaned up
    let key = (account, authenticator_id);

    // save the updated pre_exec balance
    PRE_EXEC_BALANCES.save(deps.storage, key, &balances)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::spend_limit::{Period, SpendLimitParams};
    use cosmwasm_std::{
        testing::{mock_dependencies_with_balances, mock_env},
        to_json_binary, Addr, Binary, Coin, Uint128,
    };
    use osmosis_authenticators::TrackRequest;

    #[test]
    fn test_track_success() {
        let mut deps = mock_dependencies_with_balances(&[("addr", &[Coin::new(1000, "usdc")])]);

        let track_request = TrackRequest {
            authenticator_id: "2".to_string(),
            account: Addr::unchecked("addr"),
            fee_payer: Addr::unchecked("addr"),
            authenticator_params: Some(
                to_json_binary(&SpendLimitParams {
                    limit: Uint128::new(500_000_000),
                    reset_period: Period::Day,
                    time_limit: None,
                })
                .unwrap(),
            ),
            msg: osmosis_authenticators::Any {
                type_url: "".to_string(),
                value: Binary::default(),
            },
            msg_index: 0,
        };

        let response = track(deps.as_mut(), mock_env(), track_request).unwrap();
        assert_eq!(response, Response::new().add_attribute("action", "track"));

        // Verify that the pre_exec_balance is updated
        let key = (&Addr::unchecked("addr"), "2");
        let pre_exec_balance = PRE_EXEC_BALANCES.load(deps.as_ref().storage, key).unwrap();
        assert_eq!(pre_exec_balance, vec![Coin::new(1000, "usdc")]);
    }

    #[test]
    fn test_track_success_with_dirty_pre_exec_balance() {
        let mut deps = mock_dependencies_with_balances(&[("addr", &[Coin::new(1000, "usdc")])]);

        let key = (&Addr::unchecked("addr"), "2");

        // make sure the pre-exec balance dirty
        PRE_EXEC_BALANCES
            .save(deps.as_mut().storage, key, &vec![Coin::new(500, "usdc")])
            .unwrap();

        let track_request = TrackRequest {
            authenticator_id: "2".to_string(),
            account: Addr::unchecked("addr"),
            fee_payer: Addr::unchecked("addr"),
            authenticator_params: Some(
                to_json_binary(&SpendLimitParams {
                    limit: Uint128::new(500_000_000),
                    reset_period: Period::Day,
                    time_limit: None,
                })
                .unwrap(),
            ),
            msg: osmosis_authenticators::Any {
                type_url: "".to_string(),
                value: Binary::default(),
            },
            msg_index: 0,
        };

        let response = track(deps.as_mut(), mock_env(), track_request).unwrap();
        assert_eq!(response, Response::new().add_attribute("action", "track"));

        // Verify that the pre_exec_balance is updated
        let pre_exec_balance = PRE_EXEC_BALANCES.load(deps.as_ref().storage, key).unwrap();
        assert_eq!(pre_exec_balance, vec![Coin::new(1000, "usdc")]);
    }
}
