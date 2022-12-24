use crate::config::Config;
use crate::session::Session;
use anyhow::Result;
use std::sync::Arc;
use tokio::net::TcpListener;

pub struct Server {
    config: Arc<Config>,
}

impl Server {
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            config,
        }
    }

    pub async fn run(&self) -> Result<()> {
        let server_config = self.config.server();
        let host = server_config.host().clone();
        let port = server_config.port();
        println!("[info] Trying to bind to {}:{}", host, port);
        let server = TcpListener::bind((host, port)).await?;
        println!("[info] Server running");

        loop {
            let (client_stream, client_addr) = server.accept().await?;
            println!("[info] [{}] Accepted", client_addr);
            let session = Session::new(self.config.clone(), client_addr);
            session.spawn_task(client_stream);
        }
    }
}
