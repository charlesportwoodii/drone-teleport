mod config;

extern crate tokio;

/// A drone plugin for executing remote commands over SSH, through Teleport Machine IDs
#[tokio::main]
async fn main() {
    // Parse arguments with clap => config::Config struct
    let cfg = config::state::get_config();

    match &cfg.cmd {
        config::state::SubCommand::Connect(config) => {
            config.connect(&cfg).await;
        },
        config::state::SubCommand::Transfer(config) => {
            config.transfer(&cfg).await;
        },

    }
}