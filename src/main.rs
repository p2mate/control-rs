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

use itertools::Itertools;
use serialport::prelude::*;
use serialport::Error;
use std::io::{BufRead, BufReader, Write};
use std::time::Duration;

#[derive(Debug, Clone)]
struct ExtronDevice {
    device_path: String,
    name: String,
}

fn enumerate_extron() -> Result<Vec<ExtronDevice>, Error> {
    let mut extron = Vec::new();

    let settings = SerialPortSettings {
        baud_rate: 115200,
        data_bits: DataBits::Eight,
        flow_control: FlowControl::None,
        parity: Parity::None,
        stop_bits: StopBits::One,
        timeout: Duration::from_millis(100),
    };

    for port in serialport::available_ports()? {
        match port.port_type {
            serialport::SerialPortType::UsbPort(p)
                if p.vid == 0x1ce2
                    && p.manufacturer.clone().unwrap_or("".to_string()) == "Extron" =>
            {
                match serialport::open_with_settings(&port.port_name, &settings) {
                    Ok(mut serial) => {
                        serial.clear(ClearBuffer::All)?;
                        serial.write(b"\x1bCN\x0d")?;
                        let mut serial_reader = BufReader::new(serial);
                        let mut device_name = String::new();
                        serial_reader.read_line(&mut device_name)?;
                        extron.push(ExtronDevice {
                            device_path: port.port_name,
                            name: device_name.trim_end().to_string(),
                        });
                    }
                    Err(_) => {}
                }
            }
            _ => {}
        }
    }
    Ok(extron)
}

fn main() -> Result<(), Error> {
    let settings = SerialPortSettings {
        baud_rate: 115200,
        data_bits: DataBits::Eight,
        flow_control: FlowControl::None,
        parity: Parity::None,
        stop_bits: StopBits::One,
        timeout: Duration::from_millis(1),
    };

    let devices = enumerate_extron()?;
    let devices_iter = devices.clone();
    let select_arg = clap::Arg::with_name("device")
        .short("d")
        .long("device")
        .takes_value(true)
        .value_name("NAME")
        .required(devices.len() != 1)
        .validator(move |n| {
            devices_iter
                .iter()
                .find(|d| d.name == n)
                .map(|_| ())
                .ok_or(format!("Device {} not found", n))
        })
        .help("Extron device to control");
    let args = clap::App::new("control-dsc")
        .author("Peter De Schrijver <p2@psychaos.be>")
        .version("0.1")
        .about("Control Extron scalers/switchers")
        .setting(clap::AppSettings::SubcommandRequiredElseHelp)
        .subcommand(clap::SubCommand::with_name("list").about("list available devices"))
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
                ),
        )
        .get_matches();

    match args.subcommand() {
        ("list", _) => println!(
            "{:<32}Device\n{}",
            "Name",
            devices.iter().format_with("\n", |e, f| {
                f(&format_args!("{:<32}{}", e.name, e.device_path))
            })
        ),
        ("select", Some(sub_c)) => {
            let input = sub_c.value_of("input").unwrap();
            let path = match sub_c.value_of("device") {
                Some(name) => match devices.iter().find(|d| d.name == name) {
                    Some(d) => d.device_path.clone(),
                    None => unreachable!(),
                },
                None => devices[0].device_path.clone(),
            };
            let mut port = serialport::open_with_settings(&path, &settings)?;
            let command = format!("{}!", input);
            port.write(command.as_bytes())?;
        }
        _ => unreachable!(),
    }

    Ok(())
}
