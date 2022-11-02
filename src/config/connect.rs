use clap::Parser;
use std::collections::HashMap;

use crate::config::state::Config;
use colored::Colorize;
use std::{process::exit, sync::Arc};

#[derive(Debug, Parser, Clone)]
#[clap(author, version, about, long_about = None)]
pub struct ConnectConfig {
    /// A list of environment variables
    #[clap(short, long, required = false, default_value = "", parse(try_from_str = ConnectConfig::parse_env_json), env = "PLUGIN_ENV")]
    pub env: HashMap<String, String>,

    /// The script to execute on the given targets
    #[clap(short, long, env = "PLUGIN_SCRIPT")]
    pub script: Vec<String>,
}

impl ConnectConfig {
    // Helper function to return KEY=VAL environment variables to append to each command
    pub fn build_env<'a>(&'a self) -> String {
        let mut envstr: Vec<String> = Vec::new();
        for (k, v) in &self.env {
            envstr.push(format!("export {}={}", k, v));
        }

        for (k, v) in std::env::vars() {
            if k.starts_with("PLUGIN_") {
                envstr.push(format!("export {}={}", k.replace("PLUGIN_", ""), v));
            }
        }

        return envstr.join(" && ");
    }

    // Drone submits PLUGIN_SCRIPT as a comma-separated list if settings:script is used.
    // We utilize settings:script:commands to have drone populate PLUGIN_SCRIPT as a JSON object which we can de-serialize into commands that are not altered.

    // @todo: Clap won't permit this function to be used with Vec<string> and parse it as Vec<string>
    // value_parser(parse_script_json) => Vec<String> results in:
    // thread 'main' panicked at 'Mismatch between definition and access of `script`. Could not downcast to alloc::string::String, need to downcast to alloc::vec::Vec<alloc::string::String>
    pub fn parse_script_json<'a>(&'a self) -> Result<Vec<String>, std::io::Error> {
        let script = self.script.clone();
        if script.len() == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "No commands to execute.",
            ));
        }

        let mut hash: HashMap<String, serde_json::Value> =
            serde_json::from_str(&script[0]).unwrap();

        let key = String::from("commands");
        if hash.contains_key(&key) {
            let commands = hash.remove(&key).unwrap();
            let mut ret: Vec<String> = Vec::new();

            for command in commands.as_array().unwrap().clone() {
                ret.push(serde_json::from_value(command).unwrap());
            }

            return Ok(ret);
        }

        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Missing settings:script:commands",
        ));
    }

    fn parse_env_json(
        arg: &str,
    ) -> Result<std::collections::HashMap<String, String>, std::io::Error> {
        // Parse
        if arg == "" {
            return Ok(HashMap::new());
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

    // Connects to a remote SSH target and executes the requested commands
    pub async fn connect<'a>(&'a self, cfg: &Config) {
        // Store a lists of tasks so we can execute them asyncronously
        let mut tasks = Vec::new();

        // Iterate over each host and create the processing task
        for host in cfg.hosts.to_owned() {
            let sb = Arc::new(cfg.get_sb());
            let env = Arc::new(self.build_env());
            let commands = match self.parse_script_json() {
                Ok(commands) => commands.to_owned(),
                Err(_) => {
                    println!("No commands supplied.");
                    exit(1);
                }
            };
            let debug = cfg.debug.to_owned();

            let task = tokio::spawn(async move {
                // Attempt to connect to the database via tsh
                match sb.to_owned().connect(&host).await {
                    Ok(session) => {
                        // Iterate over all of the commands and run them syncronously
                        for command in commands.iter() {
                            let command_to_run = match env.trim().is_empty() {
                                true => format!("{}", command),
                                false => format!("{}; {}", env, command),
                            };

                            match session.shell(command_to_run).output().await {
                                Ok(result) => {
                                    println!(
                                        "{}",
                                        format!(
                                            "{}: {}",
                                            &host.to_owned().yellow(),
                                            command.to_owned().green()
                                        )
                                    );
                                    println!(
                                        "{}{}",
                                        String::from_utf8(result.stdout).unwrap(),
                                        String::from_utf8(result.stderr).unwrap().red()
                                    );

                                    // If any commit exits with a non-0 exit status code, stop execution of this task.
                                    if result.status.code() != Some(0) {
                                        println!(
                                            "{}",
                                            format!("Exit: {}", result.status.code().unwrap())
                                                .red()
                                                .bold()
                                        );
                                        session.close().await.unwrap();
                                        exit(1);
                                    }
                                }
                                Err(error) => {
                                    println!(
                                        "{}",
                                        format!(
                                            "{}: {}",
                                            &host.to_owned().yellow(),
                                            command.to_owned().green()
                                        )
                                    );
                                    // If a command fail (eg command not found or similar) stop processing additional commands
                                    println!("{}\n", error.to_string().red().bold().italic());
                                    session.close().await.unwrap();
                                    exit(2);
                                }
                            };
                        }
                    }
                    // Handle tsh connection errors
                    Err(error) => {
                        println!(
                            "{} {}",
                            "Unable to connect to Teleport target:".red().bold(),
                            &host.to_owned().cyan().italic()
                        );
                        if debug {
                            println!("\t{}", error.to_string().italic());
                        }
                        exit(3);
                    }
                };
            });

            // Push the task to the list
            tasks.push(task);
        }

        // Execute all commands asyncronously
        for task in tasks {
            #[allow(unused_must_use)]
            {
                task.await;
            }
        }

        exit(0);
    }
}
