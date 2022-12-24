use crate::socks::Error as SocksError;
use crate::socks4::{Socks4Command, Socks4Reply};
use anyhow::Result;
use std::net::Ipv4Addr;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[derive(Error, Debug)]
pub enum Error {
    #[error("no authentication rejected")]
    AuthRejected,
    #[error("request failed: {0}")]
    RequestFailed(u8),
    #[error("unsupported auth method")]
    UnsupportedAuthMethod,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Socks5CommandType {
    Connect,
    Bind,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Socks5Command {
    Connect(Ipv4Addr, u16),
    Bind(Ipv4Addr, u16),
}

pub struct Socks5Reply {
    ip: Ipv4Addr,
    port: u16,
}

pub async fn read_socks5_auth_request<S>(stream: &mut S) -> Result<()>
where
    S: AsyncRead + Unpin,
{
    let num_methods = stream.read_u8().await? as usize;
    let mut methods = Vec::with_capacity(num_methods);

    for _ in 0..num_methods {
        methods.push(stream.read_u8().await?);
    }

    if methods.len() != num_methods {
        return Err(SocksError::ProtocolError)?;
    }

    if !methods.iter().any(|&x| x == 0) {
        return Err(Error::UnsupportedAuthMethod)?;
    }

    Ok(())
}

pub async fn write_socks5_auth_reply<S>(stream: &mut S) -> Result<()>
where
    S: AsyncWrite + Unpin,
{
    stream.write_u8(5).await?;
    stream.write_u8(0.into()).await?;
    stream.flush().await?;
    Ok(())
}

pub async fn write_socks5_auth<S>(stream: &mut S) -> Result<()>
where
    S: AsyncWrite + Unpin,
{
    stream.write_u8(5).await?;
    stream.write_u8(1).await?;
    stream.write_u8(0).await?;
    Ok(())
}

pub async fn read_socks5_auth_reply<S>(stream: &mut S) -> Result<()>
where
    S: AsyncRead + Unpin,
{
    let ver = stream.read_u8().await?;

    if ver != 5 {
        return Err(SocksError::ProtocolError)?;
    }

    let reply = stream.read_u8().await?;

    if reply != 0 {
        return Err(Error::AuthRejected)?;
    }

    Ok(())
}

impl Socks5Command {
    pub async fn write<S>(&self, stream: &mut S) -> Result<()>
    where
        S: AsyncWrite + Unpin,
    {
        stream.write_u8(5).await?;

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

        stream.write_u8(0).await?;
        stream.write_u8(1).await?;
        stream.write_u32(ip.into()).await?;
        stream.write_u16(port).await?;
        Ok(())
    }

    pub async fn read<S>(stream: &mut S) -> Result<Self>
    where
        S: AsyncRead + Unpin,
    {
        let version = stream.read_u8().await?;

        if version != 5 {
            return Err(SocksError::ProtocolError)?;
        }

        let command_type = stream.read_u8().await?.try_into()?;
        let reserved = stream.read_u8().await?;

        if reserved != 0 {
            return Err(SocksError::ProtocolError)?;
        }

        let address_type = stream.read_u8().await?;

        if address_type != 1 {
            return Err(SocksError::UnsupportedCommand)?;
        }

        let ip = stream.read_u32().await?.into();
        let port = stream.read_u16().await?;

        Ok(match command_type {
            Socks5CommandType::Connect => Self::Connect(ip, port),
            Socks5CommandType::Bind => Self::Bind(ip, port),
        })
    }
}

impl Socks5Reply {
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
        let ver = stream.read_u8().await?;

        if ver != 5 {
            return Err(SocksError::ProtocolError)?;
        }

        let reply = stream.read_u8().await?;

        if reply != 0 {
            return Err(Error::RequestFailed(reply))?;
        }

        let reserved = stream.read_u8().await?;

        if reserved != 0 {
            return Err(SocksError::ProtocolError)?;
        }

        let address_type = stream.read_u8().await?;

        if address_type != 1 {
            return Err(SocksError::UnsupportedCommand)?;
        }

        let ip = stream.read_u32().await?.into();
        let port = stream.read_u16().await?;
        Ok(Self::new(ip, port))
    }

    pub async fn write<S>(&self, stream: &mut S) -> Result<()>
    where
        S: AsyncWrite + Unpin,
    {
        stream.write_u8(5).await?;
        stream.write_u8(0).await?; // XXX TODO send error on fail instead of just dc
        stream.write_u8(0).await?;
        stream.write_u8(1).await?;
        stream.write_u32(self.ip().into()).await?;
        stream.write_u16(self.port()).await?;
        Ok(())
    }
}

impl TryFrom<u8> for Socks5CommandType {
    type Error = SocksError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Connect),
            2 => Ok(Self::Bind),
            _ => Err(SocksError::ProtocolError)?,
        }
    }
}

impl From<&Socks4Command> for Socks5Command {
    fn from(value: &Socks4Command) -> Self {
        match value {
            &Socks4Command::Connect(ip, port) => Self::Connect(ip, port),
            &Socks4Command::Bind(ip, port) => Self::Bind(ip, port),
        }
    }
}

impl From<&Socks4Reply> for Socks5Reply {
    fn from(value: &Socks4Reply) -> Self {
        Self::new(value.ip(), value.port())
    }
}
