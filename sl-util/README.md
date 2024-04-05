# Spend Limit Utility Program

Utility program for the spend limit contract.

This is mainly used to generate message for the spend limit contract which can be complicated to setup.

It finds best routes for each tracked denom through [sqs](https://github.com/osmosis-labs/sqs) to construct the instantiate message alongside with other necessary information defined in [config.toml](./config.toml).

To run the program, edit [config.toml](./config.toml) to fits your needs.

As a tool to help coming up with comprehensive tracked denoms in [config.toml](./config.toml), you can use `sl-util list-tokens` (use `-h` to see more options).
This will list all tokens avaialble through [imparator's api](https://api-osmosis.imperator.co/swagger/) in the format that is copy-pasteable to [config.toml](./config.toml).

```bash
sl-util token list
```

`sl-util message generate <TARGET_FILE>` this generates instantiate message which will be written to `<TARGET_FILE>`.

```bash
sl-util message generate instantiate-msg.json
```

So that we can use the msg with `osmosisd tx wasm instantiate` command.

```bash
osmosisd tx wasm instantiate $CODE_ID "$(cat instantiate-msg.json)" --label "spend-limit" --no-admin --gas-prices 0.25uosmo --gas auto --gas-adjustment 1.5 --from $ACCOUNT
```

or elsewhere appropriate.

Because this program use mainnet data, if you are testing it with any non-mainnet environment, the data may not be up-to-date, you may want to use `--latest-synced-pool` flag to filter out routes that are not available in your testing environment.

```bash
sl-util message generate instantiate-msg.json --latest-synced-pool 1499
```

All resulted routes will have no cw pool or unsynced pool that will make instantiation failed because twap will fail to calculate. But there are cases where other pool fails to calculate twap as well, the error message will tell you which once you tried to instantiate.

You can remove routes that contains those pool by

```bash
sl-util message generate instantiate-msg.json --latest-synced-pool 1499 --blacklisted-pools 1260,1275,1066
```

For more options, please seek help from `-h` flag.
