mod config;

extern crate tokio;
use std::sync::Arc;

use colored::Colorize;

/// A drone plugin for executing remote commands over SSH, through Teleport Machine IDs
#[tokio::main]
async fn main() {
    // Parse arguments with clap => config::Config struct
    let cfg = config::get_config();

    // Store a lists of tasks so we can execute them asyncronously
    let mut tasks = Vec::new();

    // Iterate over each host and create the processing task
    for host in cfg.hosts.to_owned() {
        let sb = Arc::new(cfg.get_sb());
        let env = Arc::new(cfg.build_env());
        let commands = cfg.script.to_owned();
        let debug = cfg.debug.to_owned();

        #[allow(dead_code)]
        let task = tokio::spawn(async move {
            // Attempt to connect to the database via tsh
            match sb.to_owned().connect(&host).await {
                Ok(session) => {
                    // Iterate over all of the commands and run them syncronously
                    for command in commands.iter() {
                        println!("{}", format!("{}: {}", &host.to_owned().yellow(), command.to_owned().green()));
                        match session.shell(format!("{} {}", env, command)).output().await {
                                Ok(result) => {
                                    println!(
                                        "{}{}",
                                        String::from_utf8(result.stdout).unwrap(),
                                        String::from_utf8(result.stderr).unwrap().red()
                                    );

                                // If any commit exits with a non-0 exit status code, stop execution of this task.
                                if result.status.code() != Some(0) {
                                    if debug {
                                        println!("\tExit: {}", result.status.code().unwrap());
                                    }
                                    break;
                                }
                            },
                            Err(error) => {
                                // If a command fail (eg command not found or similar) stop processing additional commands
                                println!("{}\n", error.to_string().red().bold().italic());
                                break;
                            }
                        };
                    }
                },
                // Handle tsh connection errors
                Err(error) => {
                    println!("{} {}", "Unable to connect to Teleport target:".red().bold(), &host.to_owned().cyan().italic());
                    if debug {
                        println!("\t{}", error.to_string().italic());
                    }
                }
            };
        });

        // Push the task to the list
        tasks.push(task);
    }

    // Execute all commands asyncronously
    for task in tasks {
        #[allow(unused_must_use)] {
            task.await;
        }
    }

}
