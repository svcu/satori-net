use std::{net::{IpAddr, Ipv4Addr, SocketAddr}, thread::sleep, time::Duration};

use net_core::{Node, Session};
use rand::seq::IndexedRandom;
use tun_rs::DeviceBuilder;
use winroute::{Route, RouteManager};

pub async fn run_tun(mut node: Node, exit: SocketAddr) {
 

    let mut buf = vec![0u8; 65536];
    let d = node.get_tun();

    loop {
        let len = d.recv(&mut buf).await.unwrap();
        let packet = &buf[..len];

        let peers = node.list_peers().await;
        let res = node.send_to(&exit, packet).await;
        if res.is_err() {
            println!("Error sending packet to exit node: {:?}", res.err())
        }
        println!("Packet redirected to: {:?}", exit);
    }
}

pub fn setup_routing(exit_node_ip: IpAddr) -> Result<(), Box<dyn std::error::Error>> {
    // 1. obtener gateway real actual
    let gateway = get_default_gateway().unwrap();
  

    let manager = RouteManager::new()?;

    // 2. excepción para el exit node — va por el gateway real
    manager.add_route(&Route::new(exit_node_ip, 32).gateway(gateway))?;

    // 3. redirigir todo el tráfico por el TUN
    let tun_gateway: IpAddr = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
    manager.add_route(
        &Route::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0)
            .gateway(tun_gateway)
            .metric(1),
    )?;

    Ok(())
}

pub fn teardown_routing(exit_node_ip: IpAddr) -> Result<(), Box<dyn std::error::Error>> {
    let manager = RouteManager::new()?;
    manager.delete_route(&Route::new(exit_node_ip, 32))?;
    manager.delete_route(&Route::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0))?;
    Ok(())
}

pub fn get_default_gateway() -> Result<IpAddr, Box<dyn std::error::Error>> {
    let interfaces = netdev::get_interfaces();
    for iface in interfaces {
        if let Some(gateway) = iface.gateway {
            if let Some(ip) = gateway.ipv4.first() {
                // verificar que esta interfaz tiene la ruta default
                if !iface.ipv4.is_empty() {
                    return Ok(IpAddr::V4(*ip));
                }
            }
        }
    }
    Err("No gateway found".into())
}

#[tokio::main]
async fn main() {
    let mut node_a = Node::new(1812, "10.0.0.1");
    let mut node_b = Node::new(1813, "10.0.0.2");

    node_a.listen().await;
    node_b.listen().await;

    sleep(Duration::from_secs(35));

    node_a.connect("127.0.0.1:1813".parse().unwrap()).await
        .expect("failed to connect to node_b");

    let peers = node_a.list_peers().await;
    let exit_node_ip = *peers.last().expect("no peers after connect");

    // configurar routing
    setup_routing(exit_node_ip.ip());

    // correr TUN — todo el tráfico va por node_b
    tokio::select! {
        _ = run_tun(node_a, exit_node_ip) => {}
        _ = tokio::signal::ctrl_c() => {
            println!("Shutting down...");
            teardown_routing(exit_node_ip.ip()).unwrap();
        }
    }
}
