use clap::{Parser, Subcommand, ValueEnum};
use inquire::{
    ui::{IndexPrefix, RenderConfig},
    Select,
};
use num_format::{Locale, ToFormattedString};
use prep_instantiate::{
    get_pool_liquidities, get_pools, get_route, get_tokens, Config, PoolInfo, Result, TokenInfo,
};
use serde::Serialize;
use spend_limit::msg::{InstantiateMsg, SwapAmountInRoute, TrackedDenom};
use std::{
    collections::{BTreeMap, HashMap},
    fmt::{self, Display, Formatter},
    path::PathBuf,
};

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
        /// File to write resulted message to
        #[arg(long)]
        write_to: PathBuf,

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
            write_to,
            blacklisted_pools,
            latest_synced_pool,
        } => {
            let conf: Config = toml::from_str(include_str!("../config.toml"))?;

            select_routes(conf, write_to, blacklisted_pools, latest_synced_pool).await;
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

struct RouteChoice<'a> {
    token_in: &'a str,
    routes: Vec<SwapAmountInRoute>,
    token_map: &'a BTreeMap<String, TokenInfo>,
    pool_infos: &'a HashMap<u64, PoolInfo>,
    liquidities: &'a HashMap<u64, f64>,
}

impl<'a> Display for RouteChoice<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let token_in_symbol = self.token_map[self.token_in].symbol.as_str();

        let hops = self.routes.len();

        write!(f, "[hops = {}] -({})>", hops, token_in_symbol)?;

        for route in self.routes.iter() {
            let token_out_symbol = self.token_map[&route.token_out_denom].symbol.as_str();
            let pool_info = self.pool_infos.get(&route.pool_id).unwrap();
            let liquidity = (self.liquidities.get(&route.pool_id).unwrap().round() as u64)
                .to_formatted_string(&Locale::en);

            write!(
                f,
                " pool:[{}#{} ~ ${}] -({})>",
                pool_info.pool_type, route.pool_id, liquidity, token_out_symbol
            )?;
        }

        Ok(())
    }
}

async fn select_routes(
    conf: Config,
    write_to: PathBuf,
    blacklisted_pools: Vec<u64>,
    latest_synced_pool: Option<u64>,
) {
    let token_map = get_token_map().await.expect("Failed to get prices");
    let pool_infos = get_pools().await.expect("Failed to get pools");
    let liquidities = get_pool_liquidities()
        .await
        .expect("Failed to get pool liquidities");

    let mut tracked_denoms = vec![];

    for denom in &conf.tracked_denoms {
        let qoute_denom = conf.price_resolution.quote_denom.to_string();
        let pool_infos = pool_infos.clone();
        let blacklisted_pools = blacklisted_pools.clone();

        let swap_routes = get_route(
            denom.to_string().as_str(),
            qoute_denom.as_str(),
            blacklisted_pools,
            latest_synced_pool,
            &pool_infos,
        )
        .await
        .expect("Failed to get route");

        let route_choices = swap_routes
            .into_iter()
            .map(|routes| RouteChoice {
                token_in: denom,
                routes,
                token_map: &token_map,
                pool_infos: &pool_infos,
                liquidities: &liquidities,
            })
            .collect::<Vec<_>>();

        // clear terminal
        // println!("{esc}[2J{esc}[1;1H", esc = 27 as char);
        let symbol = token_map[denom].symbol.as_str();
        let route_choice = Select::new(format!("`{}` route =", symbol).as_str(), route_choices)
            .with_render_config(
                RenderConfig::default().with_option_index_prefix(IndexPrefix::SpacePadded),
            )
            .prompt()
            .unwrap();

        let res = TrackedDenom {
            denom: denom.to_string(),
            swap_routes: route_choice.routes,
        };

        tracked_denoms.push(res);

        // keep saving result to file every time user selects a route
        let msg = InstantiateMsg {
            price_resolution_config: conf.price_resolution.clone(),
            tracked_denoms: tracked_denoms.clone(),
        };
        let msg = serde_json::to_string_pretty(&msg).expect("Failed to serialize msg");
        std::fs::write(write_to.clone(), msg).expect("Failed to write msg to file");
    }
}

async fn get_token_map() -> Result<BTreeMap<String, TokenInfo>> {
    let tokens = get_tokens().await?;
    let prices = tokens
        .into_iter()
        .map(|token| (token.denom.clone(), token))
        .collect::<BTreeMap<_, _>>();

    Ok(prices)
}
