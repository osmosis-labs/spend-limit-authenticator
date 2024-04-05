# Spend Limit Utility Program

Utility program for the spend limit contract.

This is mainly used to generate message for the spend limit contract which can be complicated to setup.

To use this program, build this program by running

```bash
cargo build --release
```

and find the binary in `target/release/sl-util`.

or just replace `sl-util` with `cargo run` within the crate directory while following the instructions below.

It finds best routes for each tracked denom through [sqs](https://github.com/osmosis-labs/sqs) to construct the instantiate message alongside with other necessary information defined in config file (toml format).

You can generate `config.toml` starter through the following command:

```bash
sl-util config example > config.toml
```

edit `config.toml` to fit your needs.

You can use `sl-util token list` to list all tokens available through [imperator's api](https://api-osmosis.imperator.co/swagger/) in the format that is copy-pasteable to `config.toml`.

```bash
sl-util token list
```

`sl-util message generate <TARGET_FILE>` this generates instantiate message which will be written to `<TARGET_FILE>`.

```bash
sl-util message generate instantiate-msg.json --config config.toml
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
