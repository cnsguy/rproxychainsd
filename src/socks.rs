use anyhow::Result;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt};

#[derive(Error, Debug)]
pub enum Error {
    #[error("unsupported version")]
    UnsupportedVersion,
    #[error("protocol error")]
    ProtocolError,
    #[error("unsupported command")]
    UnsupportedCommand,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum SocksVersion {
    Socks4,
    Socks5,
}

pub async fn read_version<S>(stream: &mut S) -> Result<SocksVersion>
where
    S: AsyncRead + Unpin,
{
    match stream.read_u8().await? {
        4 => Ok(SocksVersion::Socks4),
        5 => Ok(SocksVersion::Socks5),
        _ => Err(Error::UnsupportedVersion)?,
    }
}
