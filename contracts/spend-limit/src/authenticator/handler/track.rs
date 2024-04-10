use crate::authenticator::common::get_account_spending_fee;
use crate::state::{PRE_EXEC_BALANCES, UNTRACKED_SPENT_FEES};
use crate::ContractError;
use cosmwasm_std::{Coins, DepsMut, Env, Response};
use osmosis_authenticators::TrackRequest;

pub fn track(
    deps: DepsMut,
    _env: Env,
    TrackRequest {
        account,
        authenticator_id,
        fee_payer,
        fee_granter,
        fee,
        ..
    }: TrackRequest,
) -> Result<Response, ContractError> {
    let key = (&account, authenticator_id.as_str());

    // add new fee to untracked spent fee, if confirm execution passed, it will be cleaned up
    // if execution or confirmation failed, it will be accumulated and check at authenticate
    let mut untracked_spent_fee: Coins = UNTRACKED_SPENT_FEES
        .may_load(deps.storage, key)?
        .unwrap_or_default()
        .try_into()?;

    for fee in get_account_spending_fee(&account, &fee_payer, fee_granter.as_ref(), fee) {
        untracked_spent_fee.add(fee)?;
    }

    UNTRACKED_SPENT_FEES.save(deps.storage, key, &untracked_spent_fee.to_vec())?;

    // force update pre_exec_balance, disregard the previous value
    let balances = deps.querier.query_all_balances(account.to_string())?;
    PRE_EXEC_BALANCES.save(deps.storage, key, &balances)?;

    Ok(Response::new().add_attribute("action", "track"))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::period::Period;
    use crate::{spend_limit::SpendLimitParams, state::UNTRACKED_SPENT_FEES};
    use cosmwasm_std::{
        testing::{mock_dependencies_with_balances, mock_env},
        to_json_binary, Addr, Binary, Coin, Uint128,
    };
    use osmosis_authenticators::TrackRequest;

    #[test]
    fn test_track_success() {
        let mut deps = mock_dependencies_with_balances(&[("addr", &[Coin::new(1000, "uusdc")])]);
        let fee = vec![Coin::new(1000, "uosmo"), Coin::new(1000, "usdc")];
        let track_request = TrackRequest {
            authenticator_id: "2".to_string(),
            account: Addr::unchecked("addr"),
            fee_payer: Addr::unchecked("addr"),
            fee_granter: None,
            fee: fee.clone(),
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
        assert_eq!(pre_exec_balance, vec![Coin::new(1000, "uusdc")]);

        let untracked_spent_fee = UNTRACKED_SPENT_FEES
            .load(deps.as_ref().storage, key)
            .unwrap_or_default();
        assert_eq!(untracked_spent_fee, fee);
    }

    #[test]
    fn test_track_success_with_dirty_pre_exec_balance() {
        let mut deps = mock_dependencies_with_balances(&[("addr", &[Coin::new(1000, "uusdc")])]);

        let key = (&Addr::unchecked("addr"), "2");

        // make sure the pre-exec balance dirty
        PRE_EXEC_BALANCES
            .save(deps.as_mut().storage, key, &vec![Coin::new(500, "uusdc")])
            .unwrap();

        let fee = vec![Coin::new(1000, "uosmo"), Coin::new(1000, "usdc")];
        let track_request = TrackRequest {
            authenticator_id: "2".to_string(),
            account: Addr::unchecked("addr"),
            fee_payer: Addr::unchecked("addr"),
            fee_granter: None,
            fee: fee.clone(),
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
        assert_eq!(pre_exec_balance, vec![Coin::new(1000, "uusdc")]);

        let untracked_spent_fee = UNTRACKED_SPENT_FEES
            .load(deps.as_ref().storage, key)
            .unwrap_or_default();
        assert_eq!(untracked_spent_fee, fee);
    }

    #[test]
    fn test_track_success_with_dirty_untracked_fees() {
        let mut deps = mock_dependencies_with_balances(&[("addr", &[Coin::new(1000, "uusdc")])]);

        let key = (&Addr::unchecked("addr"), "2");

        let prev_untracked_spent_fee = vec![Coin::new(500, "uosmo")];

        UNTRACKED_SPENT_FEES
            .save(deps.as_mut().storage, key, &prev_untracked_spent_fee)
            .unwrap();

        let fee = vec![Coin::new(1000, "uosmo"), Coin::new(1000, "usdc")];

        let track_request = TrackRequest {
            authenticator_id: "2".to_string(),
            account: Addr::unchecked("addr"),
            fee_payer: Addr::unchecked("addr"),
            fee_granter: None,
            fee: fee.clone(),
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
        assert_eq!(pre_exec_balance, vec![Coin::new(1000, "uusdc")]);

        // Verify that the untracked spent fee is updated
        let untracked_spent_fee = UNTRACKED_SPENT_FEES
            .load(deps.as_ref().storage, key)
            .unwrap_or_default();

        assert_eq!(
            untracked_spent_fee,
            vec![Coin::new(1500, "uosmo"), Coin::new(1000, "usdc")]
        );
    }

    #[test]
    fn test_track_success_not_accum_fee_if_has_fee_granter() {
        let mut deps = mock_dependencies_with_balances(&[("addr", &[Coin::new(1000, "uusdc")])]);

        let key = (&Addr::unchecked("addr"), "2");

        let prev_untracked_spent_fee = vec![Coin::new(500, "uosmo")];

        UNTRACKED_SPENT_FEES
            .save(deps.as_mut().storage, key, &prev_untracked_spent_fee)
            .unwrap();

        let fee = vec![Coin::new(1000, "uosmo"), Coin::new(1000, "usdc")];

        let track_request = TrackRequest {
            authenticator_id: "2".to_string(),
            account: Addr::unchecked("addr"),
            fee_payer: Addr::unchecked("addr"),
            fee_granter: Some(Addr::unchecked("granter")),
            fee: fee.clone(),
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
        assert_eq!(pre_exec_balance, vec![Coin::new(1000, "uusdc")]);

        // Verify that the untracked spent fee is updated
        let untracked_spent_fee = UNTRACKED_SPENT_FEES
            .load(deps.as_ref().storage, key)
            .unwrap_or_default();

        assert_eq!(untracked_spent_fee, prev_untracked_spent_fee);
    }

    #[test]
    fn test_track_success_not_accum_fee_if_not_a_fee_payer() {
        let mut deps = mock_dependencies_with_balances(&[("addr", &[Coin::new(1000, "uusdc")])]);

        let key = (&Addr::unchecked("addr"), "2");

        let prev_untracked_spent_fee = vec![Coin::new(500, "uosmo")];

        UNTRACKED_SPENT_FEES
            .save(deps.as_mut().storage, key, &prev_untracked_spent_fee)
            .unwrap();

        let fee = vec![Coin::new(1000, "uosmo"), Coin::new(1000, "usdc")];

        let track_request = TrackRequest {
            authenticator_id: "2".to_string(),
            account: Addr::unchecked("addr"),
            fee_payer: Addr::unchecked("not_addr"),
            fee_granter: None,
            fee: fee.clone(),
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
        assert_eq!(pre_exec_balance, vec![Coin::new(1000, "uusdc")]);

        // Verify that the untracked spent fee is updated
        let untracked_spent_fee = UNTRACKED_SPENT_FEES
            .load(deps.as_ref().storage, key)
            .unwrap_or_default();

        assert_eq!(untracked_spent_fee, prev_untracked_spent_fee);
    }
}
