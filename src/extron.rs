use serialport::prelude::*;
use std::io::{BufRead, BufReader, Result, Write};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ExtronDevice {
    pub device_path: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct ExtronDeviceList {
    map: std::collections::HashMap<String, ExtronDevice>,
}

impl ExtronDeviceList {
    pub fn rescan(&mut self) -> Result<()> {
        self.map.clear();
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
                            let name = device_name.trim_end().to_string();
                            self.map.insert(
                                name.clone(),
                                ExtronDevice {
                                    device_path: port.port_name,
                                    name,
                                },
                            );
                        }
                        Err(_) => {}
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub fn enumerate_extron() -> Result<Self> {
        let extron = std::collections::HashMap::new();
        let mut result = Self { map: extron };
        result.rescan()?;

        Ok(result)
    }

    pub fn new() -> Self {
        let map = std::collections::HashMap::new();
        Self { map }
    }

    pub fn find(&self, name: &str) -> Option<ExtronDevice> {
        self.map.get(name).map(|d| d.clone())
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = ExtronDevice> + '_ {
        self.map.iter().map(|(_, d)| d.clone())
    }
}

impl ExtronDevice {
    pub fn select(&self, input: &str) -> Result<()> {
        use std::io::{Error, ErrorKind};
        let settings = SerialPortSettings {
            baud_rate: 115200,
            data_bits: DataBits::Eight,
            flow_control: FlowControl::None,
            parity: Parity::None,
            stop_bits: StopBits::One,
            timeout: Duration::from_millis(100),
        };
        let mut port = serialport::open_with_settings(&self.device_path, &settings)?;
        let command = format!("{}!", input);
        port.write(command.as_bytes())?;
        //    .map(|_| ())
        //    .map_err(|e| e.into())?;

        let serial_reader = BufReader::new(port);
        let ok_pattern = format!("In{}All", input);
        for line in serial_reader.lines() {
            let l = line?;
            let result = if l.starts_with("E01") {
                Err(Error::new(ErrorKind::Other, format!("Invalid input {}", input)))
            } else if l.starts_with(&ok_pattern) {
                Ok(())
            } else  {
                Err(Error::new(ErrorKind::Other, format!("Unexpected answer {}", l)))
            };
            result?;
        }
        Ok(())
    }
}
