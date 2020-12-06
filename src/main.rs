/*
 *  Copyright 2020 Peter De Schrijver
 *
 *  Licensed under the Apache License, Version 2.0 (the "License");
 *  you may not use this file except in compliance with the License.
 *  You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 *  Unless required by applicable law or agreed to in writing, software
 *  distributed under the License is distributed on an "AS IS" BASIS,
 *  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 *  See the License for the specific language governing permissions and
 *  limitations under the License.
 */

#[macro_use]
extern crate log;

mod client;
mod extron;
mod server;

use anyhow::Result;
use extron::ExtronDeviceList;
use itertools::Itertools;
pub mod extron_capnp {
    include!(concat!(env!("OUT_DIR"), "/extron_capnp.rs"));
}

fn get_ip_endpoint_arg(value_name: &str) -> clap::Arg {
    clap::Arg::with_name("address")
        .takes_value(true)
        .value_name(value_name)

        .validator(|x| {
            use std::net::ToSocketAddrs;
            let mut addrs = x.to_socket_addrs().unwrap_or(Vec::new().into_iter());
            addrs
                .next()
                .map(|_| ())
                .ok_or(format!("'{}' does not contain a valid address", x))
        })
}

fn main() -> Result<()> {
    let devices = ExtronDeviceList::enumerate_extron().unwrap_or(ExtronDeviceList::new());
    let program_name: String = std::env::current_exe()
        .unwrap_or("control-dsc".into())
        .file_name()
        .map_or("control-dsc".into(), |v| v.to_string_lossy().to_string());

    let select_arg = clap::Arg::with_name("device")
        .short("d")
        .long("device")
        .takes_value(true)
        .value_name("NAME")
        .required(devices.len() != 1)
        .help("Extron device to control");

    let remote_arg = get_ip_endpoint_arg("SERVER ADDRESS")
        .short("r")
        .long("remote");

    let args = clap::App::new(format!("{}", program_name))
        .author("Peter De Schrijver <p2@psychaos.be>")
        .version("0.2")
        .about("Control Extron scalers/switchers")
        .setting(clap::AppSettings::SubcommandRequiredElseHelp)
        .subcommand(
            clap::SubCommand::with_name("list")
                .about("list available devices")
                .arg(remote_arg.clone().help("Remote server to connect to")),
        )
        .subcommand(
            clap::SubCommand::with_name("select")
                .about("select input")
                .arg(select_arg)
                .arg(
                    clap::Arg::with_name("input")
                        .index(1)
                        .takes_value(true)
                        .value_name("INPUT")
                        .help("input port")
                        .required(true),
                )
                .arg(
                    remote_arg
                        .clone()
                        .requires("device")
                        .help("Remote server to connect to"),
                ),
        )
        .subcommand(
            clap::SubCommand::with_name("server")
                .about("run as server")
                .arg(
                    get_ip_endpoint_arg("LISTEN ADDRESS")
                        .index(1)
                        .help("Adress:Port to listen to")
                        .default_value("0.0.0.0:14000")
                        .required(true),
                )
                .arg(
                    clap::Arg::with_name("debug output")
                        .takes_value(true)
                        .value_name("DEBUG LOG DIRECTORY")
                        .long("debug"),
                )
                .arg(clap::Arg::with_name("no-daemonize").long("no-daemonize")),
        )
        .subcommand(
            clap::SubCommand::with_name("rescan")
                .about("force rescan on server")
                .arg(
                    remote_arg
                        .clone()
                        .index(1)
                        .help("Adress:Port to connect to")
                        .required(true),
                ),
        )
        .subcommand(
            clap::SubCommand::with_name("stop_server")
                .about("halt server")
                .arg(
                    remote_arg
                        .clone()
                        .index(1)
                        .help("Adress:Port to connect to")
                        .required(true),
                ),
        )
        .get_matches();

    match args.subcommand() {
        ("list", Some(sub_c)) => {
            if let Some(addr) = sub_c.value_of("address") {
                let remote = client::Client::new(&addr.to_string())?;
                remote.list()?;
            } else {
                println!(
                    "{:<32}Device\n{}",
                    "Name",
                    devices.iter().format_with("\n", |e, f| {
                        f(&format_args!("{:<32}{}", e.name, e.device_path))
                    })
                )
            }
        }

        ("select", Some(sub_c)) => {
            let input = sub_c.value_of("input").unwrap();
            let device = sub_c.value_of("device");
            if let Some(addr) = sub_c.value_of("address") {
                let remote = client::Client::new(&addr.to_string())?;
                remote.select(device.unwrap(), input)?;
            } else {
                match device {
                    Some(name) => match devices.find(name) {
                        Some(d) => d.select(input)?,
                        None => println!("Device {} not found.", name),
                    },
                    None => devices.iter().next().unwrap().select(input)?,
                };
            }
        }
        ("server", Some(sub_c)) => {
            use daemonize::{Daemonize, Group, User};
            use flexi_logger::{LogTarget, Logger};
            use std::convert::TryFrom;

            let addrs = sub_c.value_of("address").unwrap();
            if sub_c.is_present("no-daemonize") {
                if let Some(n) = sub_c.value_of("debug output") {
                    use flexi_logger::Duplicate;
                    Box::new(
                        Logger::with_str("debug")
                            .log_to_file()
                            .directory(n)
                            .suppress_timestamp()
                            .append()
                            .duplicate_to_stdout(Duplicate::Debug),
                    )
                    .start()?
                } else {
                    Box::new(Logger::with_str("debug").log_target(LogTarget::StdOut)).start()?
                }
            } else {
                use flexi_logger::writers::{SyslogConnector, SyslogFacility, SyslogWriter};
                use flexi_logger::{Duplicate, LevelFilter};
                let syslog_connector = SyslogConnector::try_datagram("/dev/log")?;
                let syslog_write = SyslogWriter::try_new(
                    SyslogFacility::UserLevel,
                    None,
                    LevelFilter::Info,
                    program_name.clone(),
                    syslog_connector,
                )?;
                if let Some(n) = sub_c.value_of("debug output") {
                    Box::new(
                        Logger::with_str("debug")
                            .directory(n)
                            .suppress_timestamp()
                            .append()
                            .log_target(LogTarget::FileAndWriter(syslog_write))
                            .duplicate_to_stdout(Duplicate::Debug),
                    )
                    .start()?
                } else {
                    Box::new(Logger::with_str("info").log_target(LogTarget::Writer(syslog_write)))
                        .start()?
                }
            };

            let pipe = pipefile::pipe()?;
            if !sub_c.is_present("no-daemonize") {
                Daemonize::new()
                    .user(User::try_from("daemon")?)
                    .group(Group::try_from("dialout")?)
                    .umask(0o000)
                    .stderr(pipe.write_end)
                    .start()?;
            } else {
                use nix::unistd::dup2;
                use std::os::unix::io::AsRawFd;

                dup2(pipe.write_end.as_raw_fd(), 2)?;
            }

            let pipe_read = pipe.read_end.try_clone()?;
            std::thread::spawn(move || {
                use std::io::BufRead;
                let mut reader = std::io::BufReader::new(&pipe_read);
                loop {
                    let mut message = String::new();
                    match reader.read_line(&mut message) {
                        Ok(_) => debug!("{}", message.trim_end()),
                        Err(_) => {}
                    }
                }
            });

            match server::do_daemon(&addrs) {
                Ok(()) => {}
                Err(e) => error!("{}", e.to_string()),
            }
        }
        ("rescan", Some(sub_c)) => {
            let remote = client::Client::new(&sub_c.value_of("address").unwrap().to_string())?;
            remote.rescan()?;
            remote.list()?;
        }
        ("stop_server", Some(sub_c)) => {
            let remote = client::Client::new(&sub_c.value_of("address").unwrap().to_string())?;
            remote.stop()?;
        }
        _ => unreachable!(),
    }

    Ok(())
}
