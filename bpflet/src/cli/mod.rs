pub(crate) mod args;
mod load;
mod image;
mod system;
mod table;
mod unload;
mod get;
mod list;
mod helper;

use args::Commands;
use bpflet_api::{
    config::Config,
    constants::directories::{CFGPATH_BPFLET_CONFIG, RTPATH_BPFLET_SOCKET},
};
use log::warn;
use std::fs;
use tokio::net::UnixStream;
use tonic::transport::{Channel, Endpoint, Uri};
use tower::service_fn;

impl Commands {
    pub(crate) async fn execute(&self) -> Result<(), anyhow::Error> {
        let config = if let Ok(c) = fs::read_to_string(CFGPATH_BPFLET_CONFIG) {
            c.parse().unwrap_or_else(|_| {
                warn!("Unable to parse config file, using defaults");
                Config::default()
            })
        } else {
            warn!("Unable to read config file, using defaults");
            Config::default()
        };

        match self {
            Commands::Load(l) => l.execute().await,
            Commands::Unload(args) => unload::execute_unload(args).await,
            Commands::Get(args) => get::execute_get(args).await,
            Commands::List(args) => list::execute_list(args).await,
            Commands::Image(i) => i.execute().await,
            Commands::System(s) => s.execute(&config).await,
        }
    }
}

fn select_channel() -> Option<Channel> {
    let path = RTPATH_BPFLET_SOCKET.to_string();

    let address = Endpoint::try_from(format!("unix:/{path}"));
    if let Err(e) = address {
        warn!("Failed to parse unix endpoint: {e:?}");
        return None;
    };
    let address = address.unwrap();
    let channel = address
        .connect_with_connector_lazy(service_fn(move |_: Uri| UnixStream::connect(path.clone())));
    Some(channel)
}
