use clap::Parser;
use std::collections::HashMap;
use std::sync::Arc;
use openssh::{SessionBuilder};

/// A Drone CI plugin to execute commands on a remote host through Teleport Machine ID
#[derive(Debug,Parser,Clone)]
#[clap(author, version, about, long_about = None)]
pub struct Config {
    /// Teleport MachineID / Bot Username
    #[clap(short, long, value_parser, required = true, env = "PLUGIN_USERNAME")]
    pub username: String,

    /// A list of teleport hosts to connect to
    #[clap(short, long, value_parser, required = true, multiple_occurrences=true, use_value_delimiter = true, env = "PLUGIN_HOSTS")]
    pub hosts: Vec<String>,

    #[clap(long, value_parser, required = true, env = "PLUGIN_PROXY")]
    pub proxy: String,

    #[clap(short, long, value_parser, required = false, default_value = "", env = "PROXY_CLUSTER")]
    pub cluster: String,

    /// The teleport SSH port to use
    #[clap(short, long, value_parser, default_value_t = 3022, env = "PLUGIN_PORT")]
    pub port: u16,

    /// The teleport MachineID datapath
    #[clap(short, long, value_parser, min_values = 1, required = true, env = "PLUGIN_DATA_PATH")]
    pub data_path: String,

    /// The timeout for any single command
    #[clap(short, long, value_parser, default_value_t= 120, env = "PLUGIN_TIMEOUT")]
    pub timeout: i32,

    /// A list of environment variables
    #[clap(short, long, required = false, default_value = "", parse(try_from_str = parse_env_json), env = "PLUGIN_ENV")]
    pub env: HashMap<String,String>,

    /// The script to execute on the given targets
    #[clap(short, long, env = "PLUGIN_SCRIPT")]
    pub script: Vec<String>,

    /// Whether to enable debug mode or not
    #[clap(long, value_parser, default_value_t = false, env = "PLUGIN_DEBUG")]
    pub debug: bool
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

    // Helper function to return KEY=VAL environment variables to append to each command
    pub fn build_env<'a>(&'a self) -> String {
        let mut envstr: Vec<String> = Vec::new();
        for (k, v) in &self.env {
            envstr.push(format!("{}={}", k, v));
        }
        return envstr.join(" ");
    }
}

// Drone submits PLUGIN_SCRIPT as a comma-separated list if settings:script is used.
// We utilize settings:script:commands to have drone populate PLUGIN_SCRIPT as a JSON object which we can de-serialize into commands that are not altered.

// @todo: Clap won't permit this function to be used with Vec<string> and parse it as Vec<string>
// value_parser(parse_script_json) => Vec<String> results in:
// thread 'main' panicked at 'Mismatch between definition and access of `script`. Could not downcast to alloc::string::String, need to downcast to alloc::vec::Vec<alloc::string::String>
fn parse_script_json(arg: &str) -> Result<Vec<String>, std::io::Error> {
    let mut hash: HashMap<String, serde_json::Value> = serde_json::from_str(arg).unwrap();

    let key = String::from("commands");
    if hash.contains_key(&key) {
        let commands = hash.remove(&key).unwrap();
        let mut ret : Vec<String> = Vec::new();

        for command in commands.as_array().unwrap().clone() {
           ret.push(serde_json::from_value(command).unwrap());
        }

        return Ok(ret);
    }

    return Err(std::io::Error::new(std::io::ErrorKind::Other, "Missing settings:script:commands"));
}

fn parse_env_json(arg: &str) -> Result<std::collections::HashMap<String,String>, std::io::Error> {
    // Parse
    if arg == "" {
        return Ok(HashMap::new())
    }
    let v: HashMap<String, serde_json::Value> = serde_json::from_str(arg).unwrap();

    // Iterate over the serde_json::Value HashMap and convert properties into simple-to-use strings instead of raw values
    let mut n = HashMap::new();
    for (key, value) in v.into_iter() {
        let parsed_val: String;
        if value.is_string() {
            parsed_val = serde_json::from_value(value).unwrap();
        } else if value.is_boolean() || value.is_number() {
            parsed_val = serde_json::to_string(&value).unwrap();
        } else if value.is_null() {
            parsed_val = String::from("");
        } else {
            // Ignore objects and arrays
            parsed_val = String::from("");
        }

        n.insert(key.to_string(), parsed_val);
    }

    return Ok(n);
}

// Parsing command for clap to correctly build the configuration.
pub fn get_config() -> Arc<Config> {
    // Collect the arguments, then properly mutate the configuration with the parse_script_json so we can read the data from PLUGIN_SCRIPT correctly.
    // Find a way to do this with clap instead of here so args can be immutable
    let mut argsc = Config::parse();
    let script =  parse_script_json(&argsc.script[0]);
    argsc.script = script.unwrap();

    if argsc.cluster == "" {
        argsc.cluster = argsc.proxy.clone();
    }

    // Dereference argsc to prevent use
    let args : Config = argsc.clone();
    drop(argsc);
    return Arc::new(args);
}