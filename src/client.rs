use crate::extron_capnp::control_extron;
use anyhow::Result;
use capnp_rpc::{rpc_twoparty_capnp, twoparty, RpcSystem};
use futures::{AsyncReadExt, FutureExt};
use std::net;

pub struct Client {
    addr: std::net::SocketAddr,
}

fn setup_tokio_streams(
    stream: std::net::TcpStream,
) -> Result<(control_extron::Client, RpcSystem<rpc_twoparty_capnp::Side>)> {
    use tokio::net::TcpStream;

    stream.set_nonblocking(true)?;
    let stream = TcpStream::from_std(stream)?;

    let (reader, writer) = tokio_util::compat::Tokio02AsyncReadCompatExt::compat(stream).split();
    let rpc_network = Box::new(twoparty::VatNetwork::new(
        reader,
        writer,
        rpc_twoparty_capnp::Side::Client,
        Default::default(),
    ));
    let mut rpc_system = RpcSystem::new(rpc_network, None);
    let extron_client: control_extron::Client =
        rpc_system.bootstrap(rpc_twoparty_capnp::Side::Server);

    Ok((extron_client, rpc_system))
}

async fn do_list(stream: std::net::TcpStream) -> Result<()> {
    let (extron_client, rpc_system) = setup_tokio_streams(stream)?;
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            tokio::task::spawn_local(Box::pin(rpc_system.map(|_| ())));

            let request = extron_client.list_devices_request();
            let reply = request.send().promise.await.unwrap();

            println!("{:<32}Device","Name");

            for device in reply.get().unwrap().get_reply().unwrap().iter() {
                println!(
                    "{:<32}{}",
                    device.get_name().unwrap(),
                    device.get_path().unwrap()
                );
            }
        })
        .await;
    Ok(())
}

async fn do_select(stream: std::net::TcpStream, device: &str, input: &str) -> Result<()> {
    let (extron_client, rpc_system) = setup_tokio_streams(stream)?;
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            tokio::task::spawn_local(Box::pin(rpc_system.map(|_| ())));

            let mut request = extron_client.select_input_request();
            let mut request_builder = request.get();
            request_builder.set_name(device);
            request_builder.set_input(input);
            if let Err(e) = request.send().promise.await {
                println!("{}",e);
            }
        })
        .await;
    Ok(())
}

async fn do_rescan(stream: std::net::TcpStream) -> Result<()> {
    let (extron_client, rpc_system) = setup_tokio_streams(stream)?;
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            tokio::task::spawn_local(Box::pin(rpc_system.map(|_| ())));
            let request = extron_client.rescan_request();
            request.send().promise.await.unwrap();
        })
        .await;
    Ok(())
}

async fn do_stop(stream: std::net::TcpStream) -> Result<()> {
    let (extron_client, rpc_system) = setup_tokio_streams(stream)?;
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            tokio::task::spawn_local(Box::pin(rpc_system.map(|_| ())));
            let request = extron_client.stop_server_request();
            request.send().promise.await.unwrap();
        })
        .await;
    Ok(())
}

impl Client {
    pub fn new<A: net::ToSocketAddrs>(addr: &A) -> Result<Self> {
        let addr = addr.to_socket_addrs()?.next().ok_or(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Host not found",
        ))?;
        Ok(Client { addr })
    }

    pub fn list(&self) -> Result<()> {
        use tokio::runtime;
        let rt = runtime::Runtime::new()?;
        let stream = std::net::TcpStream::connect(self.addr)?;
        let result = rt.block_on(do_list(stream));
        result
    }

    pub fn select(&self, device: &str, input: &str) -> Result<()> {
        use tokio::runtime;
        let rt = runtime::Runtime::new()?;
        let stream = std::net::TcpStream::connect(self.addr)?;

        rt.block_on(do_select(stream, device, input))
    }

    pub fn rescan(&self) -> Result<()> {
        use tokio::runtime;
        let rt = runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let stream = std::net::TcpStream::connect(self.addr)?;

        rt.block_on(do_rescan(stream))
    }

    pub fn stop(&self) -> Result<()> {
        use tokio::runtime;
        let rt = runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let stream = std::net::TcpStream::connect(self.addr)?;
        rt.block_on(do_stop(stream))
    }
}
