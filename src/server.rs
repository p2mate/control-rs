use crate::extron::{ExtronDevice, ExtronDeviceList};
use crate::extron_capnp::control_extron;
use capnp::capability::Promise;
use capnp_rpc::{rpc_twoparty_capnp, twoparty, RpcSystem};
use std::io::Result;
use std::net;

#[derive(Clone)]
struct ControlExtronImpl {
    tx_channel: tokio::sync::mpsc::Sender<ServerRequest>,
    stop: tokio::sync::mpsc::Sender<bool>,
}

async fn do_list_devices(
    tx_request: tokio::sync::mpsc::Sender<ServerRequest>,
    results: &mut control_extron::ListDevicesResults,
) -> Result<()> {
    use crate::extron_capnp::control_extron::extron_device;
    use std::io::{Error, ErrorKind};

    let (tx, mut rx) = tokio::sync::mpsc::channel(5);
    let request = ServerRequest {
        reply_channel: tx,
        cmd: ServerCmd::ListDevices,
    };
    tx_request
        .send(request)
        .await
        .map_err(|_| Error::new(ErrorKind::Other, "Internal error"))?;
    match rx.recv().await {
        None => Ok(()),
        Some(v) => {
            if let ServerReply::ListDevices(devices) = v {
                let reply = results.get().init_reply(devices.len() as u32);
                for (i, extron_device) in devices.iter().enumerate() {
                    let mut builder = capnp::message::Builder::new_default();
                    let mut device = builder.init_root::<extron_device::Builder>();
                    device.set_name(&extron_device.name);
                    device.set_path(&extron_device.device_path);
                    reply
                        .set_with_caveats(i as u32, device.into_reader())
                        .map_err(|_| Error::new(ErrorKind::Other, "Internal error"))?;
                }
                Ok(())
            } else {
                Err(Error::new(std::io::ErrorKind::Other, "Internal error")).into()
            }
        }
    }
}

impl control_extron::Server for ControlExtronImpl {
    fn list_devices(
        &mut self,
        _params: control_extron::ListDevicesParams,
        mut results: control_extron::ListDevicesResults,
    ) -> Promise<(), ::capnp::Error> {
        let tx_channel = self.tx_channel.clone();
        Promise::from_future(async move {
            do_list_devices(tx_channel, &mut results)
                .await
                .map_err(|e| e.into())
        })
    }

    fn rescan(
        &mut self,
        _params: control_extron::RescanParams,
        mut _results: control_extron::RescanResults,
    ) -> Promise<(), ::capnp::Error> {
        let tx_channel = self.tx_channel.clone();
        Promise::from_future(async move {
            use std::io::{Error, ErrorKind};
            let (tx, mut rx) = tokio::sync::mpsc::channel(5);
            let request = ServerRequest {
                reply_channel: tx,
                cmd: ServerCmd::Rescan,
            };

            tx_channel
                .send(request)
                .await
                .map_err(|_| Error::new(ErrorKind::Other, "Internal error"))?;

            rx.recv()
                .await
                .ok_or(Error::new(ErrorKind::Other, "Internal error"))?;

            Ok(())
        })
    }

    fn select_input(
        &mut self,
        params: control_extron::SelectInputParams,
        mut _results: control_extron::SelectInputResults,
    ) -> Promise<(), ::capnp::Error> {
        let tx_channel = self.tx_channel.clone();
        let name = params.get().unwrap().get_name().unwrap().to_string();
        let input = params.get().unwrap().get_input().unwrap().to_string();
        Promise::from_future(async move {
            use std::io::{Error, ErrorKind};

            let (tx, mut rx) = tokio::sync::mpsc::channel(5);
            let request = ServerRequest {
                reply_channel: tx,
                cmd: ServerCmd::Select(ServerCmdSelect { name, input }),
            };
            tx_channel
                .send(request)
                .await
                .map_err(|_| Error::new(ErrorKind::Other, "Internal error"))?;
            let reply = rx
                .recv()
                .await
                .ok_or(Error::new(ErrorKind::Other, "Internal error"))?;
            let result = if let ServerReply::Select(r) = reply {
                r
            } else {
                Err(Error::new(ErrorKind::Other, "Internal error"))
            };
            result?;

            Ok(())
        })
    }

    fn stop_server(
        &mut self,
        _params: control_extron::StopServerParams,
        mut _results: control_extron::StopServerResults,
    ) -> Promise<(), ::capnp::Error> {
        let stop = self.stop.clone();
        Promise::from_future(async move {
            use std::io::{Error, ErrorKind};
            stop.send(true)
                .await
                .map_err(|_| Error::new(ErrorKind::Other, "Stop failed"))?;
            Ok(())
        })
    }
}

#[derive(Clone, Debug)]
struct ServerCmdSelect {
    name: String,
    input: String,
}

#[derive(Clone, Debug)]
enum ServerCmd {
    Rescan,
    ListDevices,
    Select(ServerCmdSelect),
}
#[derive(Clone, Debug)]
struct ServerRequest {
    cmd: ServerCmd,
    reply_channel: tokio::sync::mpsc::Sender<ServerReply>,
}
enum ServerReply {
    RescanReply,
    ListDevices(Vec<ExtronDevice>),
    Select(Result<()>),
}

async fn cmd_loop(cmd_rx: &mut tokio::sync::mpsc::Receiver<ServerRequest>) -> Result<()> {
    use std::io::{Error, ErrorKind};

    let join = tokio::task::spawn_blocking(move || ExtronDeviceList::enumerate_extron());
    let devices = join.await?;
    let mut device_list = devices?;

    while let Some(request) = cmd_rx.recv().await {
        match request.cmd {
            ServerCmd::Rescan => {
                let result: Result<ExtronDeviceList> =
                    tokio::task::spawn_blocking(move || ExtronDeviceList::enumerate_extron())
                        .await?;
                match result {
                    Ok(d) => {
                        device_list = d;
                    }
                    Err(e) => {
                        device_list = ExtronDeviceList::new();
                        info!("Rescan failed: {}", e.to_string())
                    }
                }
                request
                    .reply_channel
                    .send(ServerReply::RescanReply)
                    .await
                    .map_err(|_| Error::new(ErrorKind::Other, "Internal error"))?;
            }
            ServerCmd::ListDevices => {
                request
                    .reply_channel
                    .send(ServerReply::ListDevices(
                        device_list.iter().collect::<Vec<_>>(),
                    ))
                    .await
                    .map_err(|_| Error::new(ErrorKind::Other, "Internal error"))?;
            }
            ServerCmd::Select(s) => {
                request
                    .reply_channel
                    .send(ServerReply::Select(
                        if let Some(device) = device_list.find(&s.name) {
                            tokio::task::spawn_blocking(move || device.select(&s.input))
                                .await?
                                .into()
                        } else {
                            Err(
                                std::io::Error::new(std::io::ErrorKind::Other, "Device not found")
                                    .into(),
                            )
                        },
                    ))
                    .await
                    .map_err(|_| Error::new(ErrorKind::Other, "Internal error"))?;
            }
        }
    }
    Ok(())
}

async fn run_server<A: net::ToSocketAddrs>(
    addr: &A,
    stop_server: tokio::sync::mpsc::Sender<bool>,
) -> Result<()> {
    let addr = addr.to_socket_addrs().unwrap().next().unwrap();
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("Server listening on {}", addr);
    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::channel::<ServerRequest>(50);
    tokio::task::spawn(async move { cmd_loop(&mut cmd_rx).await });

    let control_extron = ControlExtronImpl {
        tx_channel: cmd_tx.clone(),
        stop: stop_server.clone(),
    };
    let extron_client: control_extron::Client = capnp_rpc::new_client(control_extron);
    loop {
        use futures::{AsyncReadExt, FutureExt};
        let (stream, _) = listener.accept().await?;
        stream.set_nodelay(true)?;
        let (reader, writer) =
            tokio_util::compat::Tokio02AsyncReadCompatExt::compat(stream).split();
        let network = twoparty::VatNetwork::new(
            reader,
            writer,
            rpc_twoparty_capnp::Side::Server,
            Default::default(),
        );
        let rpc_system = RpcSystem::new(Box::new(network), Some(extron_client.clone().client));
        tokio::task::spawn_local(Box::pin(rpc_system.map(|_| ())));
    }
}

async fn server_app<A: net::ToSocketAddrs>(addr: &A) -> Result<()> {
    use tokio::sync::mpsc;

    let (stop_tx, mut stop_rx) = mpsc::channel::<bool>(1);
    let local = tokio::task::LocalSet::new();

    let r = tokio::select! {
        r = local.run_until(run_server(addr, stop_tx)) => r,
        _ = stop_rx.recv() => Ok(()),
    };
    r
}

pub fn do_daemon<A: net::ToSocketAddrs>(addr: &A) -> Result<()> {
    use tokio::runtime;
    let rt = runtime::Runtime::new()?;
    rt.block_on(server_app(addr))?;
    info!("Server halted");
    Ok(())
}
