extern crate tar;

use crate::config::state::Config;

use clap::Parser;
use std::collections::HashMap;

use std::sync::Arc;
use std::process::exit;
use colored::Colorize;
use glob::glob_with;
use glob::MatchOptions;

use std::path::Path;
use std::fs::{File, remove_file};
use std::io::Read;

use tar::Builder;
use rand::distributions::{Alphanumeric, DistString};
use openssh::Stdio;
use openssh_sftp_client::Sftp;
use std::time::Instant;
use human_bytes::human_bytes;

#[derive(Debug,Parser,Clone)]
#[clap(author, version, about, long_about = None)]
pub struct TransferConfig {
    /// A list of src => dst files to transfer
    #[clap(short, long, env = "PLUGIN_FILES")]
    pub files: Vec<String>,

    /// Whether or not to enable compression on uploads. Defaults to true.
    #[clap(long, value_parser, default_value_t= true, env = "PLUGIN_COMPRESS")]
    pub compress: bool,

    /// ZSTD compression level.
    #[clap(long, value_parser, default_value_t= 13, env = "PLUGIN_COMPRESS_LEVEL")]
    pub compress_level: i32,
}

impl TransferConfig {
    // ~64 Kb
    const BUF_SIZE : usize = 2 << 16;

    pub fn parse_files_json<'a>(&'a self) -> Result<std::collections::HashMap<String,String>, std::io::Error> {
        let files = self.files.clone();
        if files.len() == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "No files provided to transfer."));
        }

        let json : Vec<serde_json::Value> = serde_json::from_str(&files[0])?;

        let mut result : HashMap<String, String> = HashMap::new();
        for obj in json {
            if obj.is_object() {
               let sd = obj.as_object().unwrap();
               if sd.contains_key("src") && sd.contains_key("dst") {

                let src = sd.get("src").unwrap().as_str().unwrap();
                let dst = sd.get("dst").unwrap().as_str().unwrap();
                result.insert(src.to_string(), dst.to_string());
               }
            }
        }

        return Ok(result);
    }

    // Transfers the requested files to the remote server
    pub async fn transfer<'a>(&'a self, cfg: &Config) {
        let glob_options = MatchOptions {
            case_sensitive: false,
            require_literal_separator: false,
            require_literal_leading_dot: false,
        };

        let mut tasks = Vec::new();

        let hosts = cfg.hosts.to_owned();
        for host in hosts {
            // Iterate over each host and create the processing task
            // File transfers are syncronous IO, so run them in separate threads
            // todo!() => Make this so it can live outside of this loop
            let files = self
            .parse_files_json()
            .unwrap()
            .to_owned();

            let sb = Arc::new(cfg.get_sb());
            let debug = cfg.debug.to_owned();
            let compress = self.compress.to_owned();
            let compression_level = self.compress_level.to_owned();

            let task = tokio::task::spawn_blocking(move || {
                let handle = tokio::runtime::Handle::current();
                if let Ok(session) = handle.block_on(sb.to_owned().connect(&host)) {
                    if let Ok(mut child) = handle.block_on(session
                        .subsystem("sftp")
                        .stdin(Stdio::piped())
                        .stdout(Stdio::piped())
                        .spawn()) {
                            if let Ok (sftp) = handle.block_on(Sftp::new(
                                child.stdin().take().unwrap(),
                                child.stdout().take().unwrap(),
                                Default::default(),
                            )) {
                                for (src, dst) in files {
                                    let mut fpath = dst.clone();

                                    // Create dst on the remote server
                                    if debug {
                                        println!("{}: Ensuring remote directory path {} exists for {}", &host.bold().yellow(), &dst.to_string().italic().cyan(), &src.to_string().italic().cyan());
                                    }

                                    // Grab the paths to create
                                    let mut paths : Vec<String> = Vec::new();
                                    while fpath != "/" {
                                        paths.push(fpath.to_string());
                                        let path = Path::new(fpath.as_str());
                                        fpath = path.parent().unwrap().display().to_string();

                                        if fpath == "/" {
                                            break;
                                        }
                                    }

                                    // Create the paths in reverse tree order - sftp doesn't have a `mkdir -p` equivalent so we have to make them each one-by-one
                                    paths.reverse();
                                    for fp in paths {
                                        let path = Path::new(fp.as_str());
                                        #[allow(unused_must_use)] {
                                            handle.block_on(sftp.fs().create_dir(path));
                                        }
                                    }

                                    // Grab all the files matched by the glob, thenn create an archive to upload
                                    if debug {
                                        println!("{}: Creating archive to upload.", &host.bold().yellow());
                                    }
                                    let glob = glob_with(&src, glob_options).unwrap();
                                    let farcname = Alphanumeric.sample_string(&mut rand::thread_rng(), 16);
                                    let mut tarname = format!("/tmp/{}.tar", farcname);
                                    let mut archive = File::create(tarname.clone()).unwrap();
                                    let mut archive_builder = Builder::new(archive);
                                    for entry in glob {
                                        if let Ok(path) = entry {
                                            archive_builder.append_path(path).unwrap();
                                        }
                                    }

                                    // Verify that the archive is built out
                                    if let Err(done) = archive_builder.finish() {
                                        println!("{}: {}", "Unable to create local archive".bold().red(), done.to_string().bold());
                                        exit(6);
                                    }


                                    // If compression is enabled, compress to archive to zstd
                                    if compress {
                                        println!("{}: Compressing archive prior to transfer.", &host.bold().yellow());
                                        let comp_tar_name = format!("/tmp/{}.tar.zstd", farcname);
                                        let new_archive = File::create(comp_tar_name).unwrap();
                                        archive = File::open(format!("/tmp/{}.tar", farcname)).unwrap();
                                        if let Err(_comp_rst) = zstd::stream::copy_encode(archive, new_archive, compression_level) {
                                            println!("{}: Unable to compress archive. Aborting with error: {}", &host.bold().yellow(), _comp_rst);
                                            exit(7);
                                        }
                                        tarname = format!("/tmp/{}.tar.zstd", farcname);
                                    }

                                    // Create the remote archive file on the SFTP server
                                    let mut r_file = handle.block_on(sftp
                                        .options()
                                        .read(true)
                                        .create(true)
                                        .write(true)
                                        .open(format!("{}", &tarname))).unwrap();

                                    // Rewind the archive by re-opening the file
                                    archive = File::open(&tarname).unwrap();

                                    let archive_size = human_bytes(archive.metadata().unwrap().len() as f64);
                                    let now = Instant::now();
                                    {
                                        println!("{}: {} {} {}",
                                            &host.bold().yellow(),
                                            "Transferring".bold(),
                                            &src.to_string().italic(),
                                            archive_size.bold().green()
                                        );

                                        // Write the archive to the remote location
                                        let mut buffer = [0u8; TransferConfig::BUF_SIZE];
                                        let mut transfered = 0;
                                        loop {
                                            let rc = archive.read(&mut buffer).unwrap();
                                            handle.block_on(r_file.write(&buffer[..rc])).unwrap();
                                            transfered += TransferConfig::BUF_SIZE;

                                            // Log at 8Mb intervals for progress indicator
                                            if debug {
                                                if transfered % (2 << 21) == 0{
                                                    println!("{}: {} {}/{} \r",
                                                        &host.bold().yellow(),
                                                        "Transferring -".bold(),
                                                        human_bytes(transfered as f64).bold().cyan(),
                                                        archive_size.bold().green()
                                                    );
                                                }
                                            }

                                            if rc != TransferConfig::BUF_SIZE {
                                                break;
                                            }
                                        }
                                    }

                                    let elapsed = now.elapsed();
                                    println!("{}: {} {} {} {} {}{}",
                                        &host.bold().yellow(),
                                        "Completed".bold(),
                                        &src.to_string().italic(),
                                        archive_size.bold().green(),
                                        "in", elapsed.as_secs().to_string().bold().cyan(),
                                        " seconds"
                                    );

                                    // Close the remote file
                                    #[allow(unused_must_use)] {
                                        handle.block_on(r_file.close());
                                    }

                                    // Extract the archive on the remote server and delete it
                                    if let Err(_command) = handle.block_on(session.shell(format!("tar -xf {} -C {}", tarname, dst)).output()) {
                                        println!("{} {}", "Unable to extract archive on remote".bold().red(), &host.bold().yellow());
                                    }

                                    // Delete the archive on the remote
                                    if let Err(_command) = handle.block_on(session.shell(format!("rm {}", tarname)).output()) {
                                        println!("{} {}", "Unable to delete archive on remote".bold().red(), &host.bold().yellow());
                                    }

                                    // Cleanup the local disk
                                    if let Err(rmrst) = remove_file(tarname.clone()) {
                                        if debug {
                                            println!("{} {}\n\t{}",
                                                "Unable to remove local archive directory:".bold().red(),
                                                &tarname.bold().yellow(),
                                                rmrst.to_string().italic()
                                            );
                                        }
                                    }
                                }

                                // Close the sftp connection
                                handle.block_on(sftp.close()).unwrap();
                            } else {
                                // Failed to create new SFTP instance
                                println!("{}: {}.", &host, "Failed to create SFTP instance".bold().red());
                                exit(3);
                            }
                        } else {
                            // Failed to setup SFTP subsystem
                            println!("{}: {}", &host, "Failed to setup SFTP subsystem on remote.".bold().red());
                            exit(2);
                        }

                    // Close the connection, errors don't matter
                    #[allow(unused_must_use)] {
                        handle.block_on(session.close());
                    }
                } else {
                    // Failed to connect
                    println!("{} {}", "Unable to connect to Teleport target:".red().bold(), &host.to_owned().cyan().italic());
                    exit(1);
                }
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
}