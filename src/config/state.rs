use clap::Parser;
use std::sync::Arc;
use openssh::{SessionBuilder};

use crate::config::connect::ConnectConfig;
use crate::config::transfer::TransferConfig;

#[derive(clap::Subcommand,Debug,Clone)]
pub enum SubCommand {
    /// Connect to a Teleport host
    Connect(ConnectConfig),
    /// Transfer a file to a Teleport host
    Transfer(TransferConfig),
}

/// A Drone CI plugin to execute commands on a remote host through Teleport Machine ID
#[derive(Debug,Parser,Clone)]
#[clap(author, version, about, long_about = None)]
pub struct Config {
    /// Command to execute
    #[clap(subcommand)]
    pub cmd: SubCommand,

    /// Teleport MachineID / Bot Username
    #[clap(short, long, value_parser, required = true, env = "PLUGIN_USERNAME")]
    pub username: String,

    /// A list of teleport hosts to connect to
    #[clap(long, value_parser, required = true, multiple_occurrences=true, use_value_delimiter = true, env = "PLUGIN_HOSTS")]
    pub hosts: Vec<String>,

    ///  Teleport Proxy Endpoint (with port)
    #[clap(long, value_parser, required = true, env = "PLUGIN_PROXY")]
    pub proxy: String,

    /// Teleport Cluster to connect to (unused)
    #[clap(short, long, value_parser, required = false, default_value = "", env = "PROXY_CLUSTER")]
    pub cluster: String,

    /// The teleport SSH port to use
    #[clap(short, long, value_parser, default_value_t = 3022, env = "PLUGIN_PORT")]
    pub port: u16,

    /// The teleport MachineID datapath
    #[clap(short, long, value_parser, min_values = 1, required = true, env = "PLUGIN_DATA_PATH")]
    pub data_path: String,

    /// Whether to enable debug mode or not
    #[clap(long, value_parser, default_value_t = false, env = "PLUGIN_DEBUG")]
    pub debug: bool,

    /// The timeout for any single command
    #[clap(short, long, value_parser, default_value_t= 120, env = "PLUGIN_TIMEOUT")]
    pub timeout: i32,
}

impl Config {
    // Helper function to get the SessionBuilder configuration
    pub fn get_sb<'a>(&'a self) -> SessionBuilder {
        let mut sb = SessionBuilder::default();
            sb.port(self.port)
                .user(self.username.to_string())
                .config_file(format!("{}/ssh_config", self.data_path))
                .known_hosts_check(openssh::KnownHosts::Accept)
                .compression(true);

        return sb;
    }
}

// Parsing command for clap to correctly build the configuration.
pub fn get_config() -> Arc<Config> {
    // Collect the arguments, then properly mutate the configuration with the parse_script_json so we can read the data from PLUGIN_SCRIPT correctly.
    // Find a way to do this with clap instead of here so args can be immutable
    let mut argsc = Config::parse();

    if argsc.cluster == "" {
        argsc.cluster = argsc.proxy.clone();
    }

    let args : Config = argsc.clone();
    drop(argsc);
    return Arc::new(args);
}