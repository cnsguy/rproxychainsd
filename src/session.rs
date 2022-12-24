use crate::config::{Config, Proxy};
use crate::socks::{read_version, SocksVersion};
use crate::socks4::{Socks4Command, Socks4Reply};
use crate::socks5::{
    read_socks5_auth_reply, read_socks5_auth_request, write_socks5_auth, write_socks5_auth_reply,
    Socks5Command, Socks5Reply,
};
use anyhow::Result;
use rand::seq::SliceRandom;
use rand::thread_rng;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::select;
use tokio::task::spawn;

pub struct Session {
    config: Arc<Config>,
    ip: SocketAddr,
}

async fn connect_to_proxy(proxy: &Proxy) -> Result<TcpStream> {
    match proxy {
        &Proxy::Socks4(ip, port) => Ok(TcpStream::connect((ip, port)).await?),
        &Proxy::Socks5(ip, port) => Ok(TcpStream::connect((ip, port)).await?),
    }
}

async fn write_chain_common<S>(stream: &mut S, chain: &[Proxy]) -> Result<TcpStream>
where
    S: AsyncWrite + Unpin,
{
    assert!(chain.len() > 0);
    let mut iter = chain.iter().peekable();
    let first = iter.peek().unwrap();
    let connection = connect_to_proxy(first).await?;

    loop {
        let proxy = iter.next().unwrap();
        let next = match iter.peek() {
            Some(next) => next,
            None => break,
        };

        match proxy {
            Proxy::Socks4(..) => match next {
                &&Proxy::Socks4(ip, port) | &&Proxy::Socks5(ip, port) => {
                    let command = Socks4Command::Connect(ip, port);
                    command.write(stream).await?;
                }
            },
            Proxy::Socks5(..) => match next {
                &&Proxy::Socks4(ip, port) | &&Proxy::Socks5(ip, port) => {
                    write_socks5_auth(stream).await?;
                    let command = Socks5Command::Connect(ip, port);
                    command.write(stream).await?;
                }
            },
        }
    }

    Ok(connection)
}

async fn read_chain_common<S>(stream: &mut S, chain: &[Proxy]) -> Result<(Ipv4Addr, u16)>
where
    S: AsyncRead + Unpin,
{
    assert!(chain.len() > 0);
    let mut iter = chain.iter().peekable();

    loop {
        let proxy = iter.next().unwrap();

        match proxy {
            Proxy::Socks4(..) => {
                let reply = Socks4Reply::read(stream).await?;

                if !iter.peek().is_some() {
                    return Ok((reply.ip(), reply.port()));
                }
            }
            Proxy::Socks5(..) => {
                read_socks5_auth_reply(stream).await?;
                let reply = Socks5Reply::read(stream).await?;

                if !iter.peek().is_some() {
                    return Ok((reply.ip(), reply.port()));
                }
            }
        }
    }
}

impl Session {
    pub fn new(config: Arc<Config>, ip: SocketAddr) -> Self {
        Self {
            config,
            ip,
        }
    }

    fn make_chain(&self) -> Vec<Proxy> {
        let mut final_chain = Vec::new();
        let mut iter = self.config.chains().iter().peekable();

        while let Some(chain) = iter.next() {
            let proxy = chain.entries().choose(&mut thread_rng()).unwrap();
            final_chain.push(proxy.clone());
        }

        final_chain
    }

    async fn handle_socks4(&mut self, client_stream: &mut TcpStream) -> Result<TcpStream> {
        let command = Socks4Command::read(client_stream).await?;
        let chain = self.make_chain();
        let mut buf = vec![];
        let mut proxy_stream = write_chain_common(&mut buf, &chain).await?;
        let last_proxy = chain.last().unwrap();

        match last_proxy {
            Proxy::Socks5(..) => {
                write_socks5_auth(&mut buf).await?;
                let command = Socks5Command::from(&command);
                command.write(&mut buf).await?;
            }
            Proxy::Socks4(..) => {
                command.write(&mut buf).await?;
            }
        }

        proxy_stream.write(&buf).await?;
        let (ip, port) = read_chain_common(&mut proxy_stream, &chain).await?;
        let mut buf = vec![];
        let reply = Socks4Reply::new(ip, port);
        reply.write(&mut buf).await?;
        client_stream.write(&buf).await?;
        Ok(proxy_stream)
    }

    async fn handle_socks5(&mut self, client_stream: &mut TcpStream) -> Result<TcpStream> {
        read_socks5_auth_request(client_stream).await?;
        write_socks5_auth_reply(client_stream).await?;
        let command = Socks5Command::read(client_stream).await?;
        let chain = self.make_chain();
        let mut buf = vec![];
        let mut proxy_stream = write_chain_common(&mut buf, &chain).await?;
        let last_proxy = chain.last().unwrap();

        match last_proxy {
            Proxy::Socks5(..) => {
                write_socks5_auth(&mut buf).await?;
                command.write(&mut buf).await?
            }
            Proxy::Socks4(..) => {
                let command = Socks4Command::from(&command);
                command.write(&mut buf).await?;
            }
        }

        proxy_stream.write(&buf).await?;
        let (ip, port) = read_chain_common(&mut proxy_stream, &chain).await?;
        let mut buf = vec![];
        let reply = Socks5Reply::new(ip, port);
        reply.write(&mut buf).await?;
        client_stream.write(&buf).await?;
        Ok(proxy_stream)
    }

    async fn run(&mut self, mut client_stream: TcpStream) -> Result<()> {
        let mut proxy_stream = match read_version(&mut client_stream).await? {
            SocksVersion::Socks4 => self.handle_socks4(&mut client_stream).await?,
            SocksVersion::Socks5 => self.handle_socks5(&mut client_stream).await?,
        };

        let (mut client_read, mut client_write) = client_stream.split();
        let (mut proxy_read, mut proxy_write) = proxy_stream.split();
        let mut proxy_buf = [0u8; 512];
        let mut client_buf = [0u8; 512];

        loop {
            select! {
                num = proxy_read.read(&mut proxy_buf) => {
                    let num = num?;

                    if num == 0 {
                        return Ok(());
                    }

                    client_write.write(&proxy_buf[..num]).await?;
                }

                num = client_read.read(&mut client_buf) => {
                    let num = num?;

                    if num == 0 {
                        return Ok(());
                    }

                    proxy_write.write(&client_buf[..num]).await?;
                }
            }
        }
    }

    pub fn spawn_task(mut self, client_stream: TcpStream) {
        spawn(async move {
            if let Err(error) = self.run(client_stream).await {
                eprintln!("[error] [{}] Error: {}", self.ip, error);
            }

            eprintln!("[info] [{}] Disconnected.", self.ip);
        });
    }
}
