use crate::socks::Error as SocksError;
use crate::socks5::{Socks5Command, Socks5Reply};
use anyhow::Result;
use std::net::Ipv4Addr;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[derive(Error, Debug)]
pub enum Error {
    #[error("request failed: {0}")]
    RequestFailed(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Socks4CommandType {
    Connect,
    Bind,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Socks4Command {
    Connect(Ipv4Addr, u16),
    Bind(Ipv4Addr, u16),
}

pub struct Socks4Reply {
    ip: Ipv4Addr,
    port: u16,
}

impl Socks4Command {
    pub async fn write<S>(&self, stream: &mut S) -> Result<()>
    where
        S: AsyncWrite + Unpin,
    {
        stream.write_u8(4).await?;

        let (ip, port) = match self {
            &Self::Connect(ip, port) => {
                stream.write_u8(1).await?;
                (ip, port)
            }
            &Self::Bind(ip, port) => {
                stream.write_u8(2).await?;
                (ip, port)
            }
        };

        stream.write_u16(port).await?;
        stream.write_u32(ip.into()).await?;
        stream.write_u8(0).await?;
        Ok(())
    }

    pub async fn read<S>(stream: &mut S) -> Result<Self>
    where
        S: AsyncRead + Unpin,
    {
        let command_type = stream.read_u8().await?.try_into()?;
        let port = stream.read_u16().await?;
        let ip = stream.read_u32().await?.into();

        // Ignore userid part
        while stream.read_u8().await? != 0 {}

        Ok(match command_type {
            Socks4CommandType::Connect => Self::Connect(ip, port),
            Socks4CommandType::Bind => Self::Bind(ip, port),
        })
    }
}

impl Socks4Reply {
    pub fn new(ip: Ipv4Addr, port: u16) -> Self {
        Self {
            ip,
            port,
        }
    }

    pub fn ip(&self) -> Ipv4Addr {
        self.ip
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub async fn read<S>(stream: &mut S) -> Result<Self>
    where
        S: AsyncRead + Unpin,
    {
        let version = stream.read_u8().await?;

        if version != 0 {
            return Err(SocksError::ProtocolError)?;
        }

        let result = stream.read_u8().await?;

        if result != 90 {
            return Err(Error::RequestFailed(result))?;
        }

        let port = stream.read_u16().await?;
        let ip = stream.read_u32().await?;
        Ok(Self::new(ip.into(), port))
    }

    pub async fn write<S>(&self, stream: &mut S) -> Result<()>
    where
        S: AsyncWrite + Unpin,
    {
        stream.write_u8(0).await?;
        stream.write_u8(90).await?;
        stream.write_u16(self.port()).await?;
        stream.write_u32(self.ip().into()).await?;
        Ok(())
    }
}

impl TryFrom<u8> for Socks4CommandType {
    type Error = SocksError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Socks4CommandType::Connect),
            2 => Ok(Socks4CommandType::Bind),
            _ => Err(SocksError::ProtocolError),
        }
    }
}

impl From<&Socks5Command> for Socks4Command {
    fn from(value: &Socks5Command) -> Self {
        match value {
            &Socks5Command::Connect(ip, port) => Self::Connect(ip, port),
            &Socks5Command::Bind(ip, port) => Self::Bind(ip, port),
        }
    }
}

impl From<&Socks5Reply> for Socks4Reply {
    fn from(value: &Socks5Reply) -> Self {
        Self::new(value.ip(), value.port())
    }
}
