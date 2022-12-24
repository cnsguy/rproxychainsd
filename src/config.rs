use anyhow::{Error as AnyError, Result};
use serde::Deserialize;
use std::net::Ipv4Addr;
use std::ops::Deref;
use thiserror::Error;
use tokio::fs::read_to_string;
use toml::from_str;

#[derive(Error, Debug)]
pub enum Error {
    #[error("expected socks4 or socks5 for proxy type")]
    UnexpectedProxyType,
    #[error("empty chain")]
    EmptyChain,
    #[error("no proxies specified")]
    NoChains,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Server {
    host: String,
    port: u16,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(try_from = "(String, String, u16)")]
#[serde(deny_unknown_fields)]
pub enum Proxy {
    Socks4(Ipv4Addr, u16),
    Socks5(Ipv4Addr, u16),
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "Vec<Proxy>")]
pub struct ChainEntries(Vec<Proxy>);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Chain {
    entries: ChainEntries,
}

#[derive(Debug, Deserialize)]
#[serde(try_from = "Vec<Chain>")]
pub struct Chains(Vec<Chain>);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    server: Server,
    chains: Chains,
}

impl Config {
    pub async fn read_file(file_name: &str) -> Result<Config> {
        let content = read_to_string(file_name).await?;
        Ok(from_str(&content)?)
    }

    pub fn server(&self) -> &Server {
        &self.server
    }

    pub fn chains(&self) -> &Chains {
        &self.chains
    }
}

impl Server {
    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Chain {
    pub fn entries(&self) -> &[Proxy] {
        &self.entries
    }
}

impl Deref for ChainEntries {
    type Target = [Proxy];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Deref for Chains {
    type Target = [Chain];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TryFrom<(String, String, u16)> for Proxy {
    type Error = AnyError;

    fn try_from(value: (String, String, u16)) -> Result<Self, Self::Error> {
        match value.0.as_str() {
            "socks4" => Ok(Self::Socks4(value.1.as_str().parse()?, value.2)),
            "socks5" => Ok(Self::Socks5(value.1.as_str().parse()?, value.2)),
            _ => Err(Error::UnexpectedProxyType)?,
        }
    }
}

impl TryFrom<Vec<Proxy>> for ChainEntries {
    type Error = AnyError;

    fn try_from(value: Vec<Proxy>) -> Result<Self, Self::Error> {
        if value.len() == 0 {
            return Err(Error::EmptyChain)?;
        }

        Ok(Self(value))
    }
}

impl TryFrom<Vec<Chain>> for Chains {
    type Error = AnyError;

    fn try_from(value: Vec<Chain>) -> Result<Self, Self::Error> {
        if value.len() == 0 {
            return Err(Error::NoChains)?;
        }

        Ok(Self(value))
    }
}
