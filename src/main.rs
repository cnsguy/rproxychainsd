mod config;
mod server;
mod session;
mod socks;
mod socks4;
mod socks5;

use crate::config::Config;
use crate::server::Server;
use anyhow::Result;
use std::env;
use tokio::main;

async fn run(config: Config) -> Result<()> {
    let server = Server::new(config.into());
    server.run().await?;
    Ok(())
}

#[main(flavor = "current_thread")]
async fn main() {
    let config_file = env::var("CONFIG");
    let config_file = config_file.as_ref().map(|s| s.as_str()).unwrap_or("config.toml");

    let config = match Config::read_file(config_file).await {
        Err(error) => {
            eprintln!("[error] Could not read config from '{}': {}", config_file, error);
            return;
        }
        Ok(config) => config,
    };

    if let Err(error) = run(config).await {
        eprintln!("[error] Fatal error: {}", error);
    }
}
