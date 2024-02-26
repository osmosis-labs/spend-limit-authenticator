use cosmwasm_std::{ensure, Addr, DepsMut, Env, Response};

use osmosis_authenticators::TrackRequest;

use crate::state::PRE_EXEC_BALANCES;

use crate::authenticator::error::{AuthenticatorError, AuthenticatorResult};

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
    Ok(Response::new())
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
    let no_dirty_pre_exec_balance = !PRE_EXEC_BALANCES.has(deps.storage, key);
    ensure!(
        no_dirty_pre_exec_balance,
        AuthenticatorError::dirty_pre_exec_balances(&key)
    );

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
        assert_eq!(response, Response::new());

        // Verify that the pre_exec_balance is updated
        let key = (&Addr::unchecked("addr"), "2");
        let pre_exec_balance = PRE_EXEC_BALANCES.load(deps.as_ref().storage, key).unwrap();
        assert_eq!(pre_exec_balance, vec![Coin::new(1000, "usdc")]);
    }

    #[test]
    fn test_track_failure_dirty_pre_exec_balance() {
        let mut deps = mock_dependencies_with_balances(&[("addr", &[Coin::new(1000, "usdc")])]);

        // Simulate existing pre-exec balance to trigger failure
        let key = (&Addr::unchecked("addr"), "2");
        PRE_EXEC_BALANCES
            .save(deps.as_mut().storage, key, &vec![Coin::new(500, "usdc")])
            .unwrap();

        let track_request = TrackRequest {
            authenticator_id: "2".to_string(),
            account: Addr::unchecked("addr"),
            authenticator_params: Some(
                to_json_binary(&SpendLimitParams {
                    limit: Uint128::new(500),
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

        let err = track(deps.as_mut(), mock_env(), track_request).unwrap_err();
        assert_eq!(err, AuthenticatorError::dirty_pre_exec_balances(&key));
    }
}
