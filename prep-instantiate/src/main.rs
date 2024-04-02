use clap::{Parser, Subcommand, ValueEnum};
use futures::StreamExt;
use prep_instantiate::{get_pools, get_route, get_tokens, Config, Result, TokenInfo};
use serde::Serialize;
use spend_limit::msg::{InstantiateMsg, TrackedDenom};
use std::collections::BTreeMap;
use tokio::task::JoinHandle;

/// Prepare instantiate msg for spend-limit contract
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Generate instantiate msg for spend-limit contract
    GenMsg {
        /// Number of concurrent requests to make to get route
        #[arg(long, default_value_t = 20)]
        concurrency: usize,

        /// Filtering out route that contains pool that is blacklisted.
        /// There are some pools that are not cw pool yet failed to calculate twap.
        #[arg(long, value_delimiter = ',')]
        blacklisted_pools: Vec<u64>,

        /// Filtering out tracked denoms that its route contains newer pool
        /// than latest pool that gets synced from mainnet.
        /// This is only used for setting up test environment.
        #[arg(long)]
        latest_synced_pool: Option<u64>,
    },

    /// List tokens in the format that is easiliy copy-pastable to config.toml
    ListTokens {
        /// Sort tokens by
        #[arg(long)]
        sort_by: SortBy,

        /// Include all infos for each token
        #[arg(long, short, default_value_t = false)]
        verbose: bool,
    },
}

#[derive(ValueEnum, Debug, Default, Serialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
enum SortBy {
    #[default]
    Volume24h,
    Liquidity,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        Commands::GenMsg {
            concurrency,
            blacklisted_pools,
            latest_synced_pool,
        } => {
            let conf: Config = toml::from_str(include_str!("../config.toml"))?;

            let tracked_denoms = get_tracked_denom_infos(
                conf.tracked_denoms.clone(),
                &conf.price_resolution.quote_denom,
                concurrency,
                blacklisted_pools,
                latest_synced_pool,
            )
            .await;

            let msg = InstantiateMsg {
                price_resolution_config: conf.price_resolution,
                tracked_denoms,
            };

            // write instantiate msg to stdout
            let msg_str = serde_json::to_string(&msg)?;
            println!("{}", msg_str);
        }
        Commands::ListTokens { sort_by, verbose } => {
            get_tokens_sorted_by_24h_volume(sort_by)
                .await
                .iter()
                .for_each(|token| {
                    print!("\"{}\", # {} - {}", token.denom, token.symbol, token.name);
                    if verbose {
                        print!(
                            " (volume_24h = {}, liquidity = {})",
                            token.volume_24h, token.liquidity
                        );
                    }
                    println!();
                });
        }
    }

    Ok(())
}

async fn get_tokens_sorted_by_24h_volume(sort_by: SortBy) -> Vec<TokenInfo> {
    let mut tokens = get_tokens().await.expect("Failed to get tokens");

    match sort_by {
        SortBy::Volume24h => tokens.sort_by(|a, b| b.volume_24h.total_cmp(&a.volume_24h)),
        SortBy::Liquidity => tokens.sort_by(|a, b| b.liquidity.total_cmp(&a.liquidity)),
    }

    tokens
}

async fn get_tracked_denom_infos(
    denoms: Vec<String>,
    qoute_denom: &str,
    concurrency: usize,
    blacklisted_pools: Vec<u64>,
    latest_synced_pool: Option<u64>,
) -> Vec<TrackedDenom> {
    let token_map = get_token_map().await.expect("Failed to get prices");
    let pool_infos = get_pools().await.expect("Failed to get pools");

    futures::stream::iter(denoms.into_iter().map(|denom| {
        let qoute_denom = qoute_denom.to_string();
        let pool_infos = pool_infos.clone();
        let blacklisted_pools = blacklisted_pools.clone();
        let handle: JoinHandle<TrackedDenom> = tokio::spawn(async move {
            let swap_routes = get_route(
    denom.to_string().as_str(),
                qoute_denom.as_str(),
                blacklisted_pools,
                latest_synced_pool,
                &pool_infos,
            )
            .await
            .expect("Failed to get route");

            TrackedDenom {
                denom: denom.to_string(),
                swap_routes,
            }
        });

        handle
    }))
    .buffer_unordered(concurrency)
    .filter_map(|handle| async {
        let res = handle.expect("Failed to join handle");

        if res.swap_routes.is_empty() {
            eprintln!(
                "⚠️ Can't automatically resolve twap-able route for denom: `{}` [`{}`], please manually set the route or remove it from the config",
                res.denom, token_map[&res.denom].symbol
            );

            return None;
        }

        Some(res)
    })
    .collect::<Vec<_>>()
    .await
}

async fn get_token_map() -> Result<BTreeMap<String, TokenInfo>> {
    let tokens = get_tokens().await?;
    let prices = tokens
        .into_iter()
        .map(|token| (token.denom.clone(), token))
        .collect::<BTreeMap<_, _>>();

    Ok(prices)
}
