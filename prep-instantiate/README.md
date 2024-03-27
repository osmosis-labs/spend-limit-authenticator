# Prepararing Instantiate Message

This is program that prepares the instantiate message for the spend limit contract.

It finds best routes for each tracked denom through [sqs](https://github.com/osmosis-labs/sqs) to construct the instantiate message alongside with other necessary information defined in [config.toml](./config.toml).

To run the program, edit [config.toml](./config.toml) to fits your needs.

As a tool to help coming up with comprehensive tracked denoms in [config.toml](./config.toml), you can use `cargo run list-tokens` (use `-h` to see more options).
This will list all tokens avaialble through [imparator's api](https://api-osmosis.imperator.co/swagger/) in the format that is copy-pasteable to [config.toml](./config.toml).

```bash
cargo run list-tokens
```

`cargo run gen-msg` this generates instantiate message which will be written to stdout, so it can be redirected to a file

```bash
cargo run gen-msg > instantiate-msg.json
```

or use it directly in the `osmosisd tx wasm instantiate` command.

```bash
osmosisd tx wasm instantiate $CODE_ID $(cargo run gen-msg) --label "spend-limit" --no-admin --gas-prices 0.25uosmo --gas auto --gas-adjustment 1.5 --from $ACCOUNT
```

Because the data comes from mainnet and your testing environment may have non up-to-date data, you may want to use `--latest-synced-pool` flag to filter out routes that are not available in your testing environment.

```bash
cargo run gen-msg --latest-synced-pool 1499
```

All resulted routes will have no cw pool or unsynced pool that will make instantiation failed because twap will fail to calculate. But there are cases where other pool fails to calculate twap as well, the error message will tell you which once you tried to instantiate.

You can remove routes that contains those pool by

```bash
cargo run gen-msg --latest-synced-pool 1499 --rejected-pool-ids 1260,1261
```
