use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use net_core::Node;
use tokio::io::{AsyncBufReadExt, BufReader};

#[cfg(windows)]
use winroute::{Route, RouteManager};

async fn prompt(label: &str, default: &str) -> String {
    if !label.is_empty() {
        print!("{label} [{default}]: ");
    }
    let mut line = String::new();
    if BufReader::new(tokio::io::stdin())
        .read_line(&mut line)
        .await
        .is_err()
    {
        return default.to_string();
    }
    let trimmed = line.trim().to_string();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed
    }
}

pub async fn run_tun(node: &mut Node, exit: SocketAddr) {
    let tun = node.tun();
    let mut buf = vec![0u8; 65536];

    loop {
        let len = match tun.recv(&mut buf).await {
            Ok(n) => n,
            Err(e) => {
                tracing::error!("TUN read error: {e}");
                break;
            }
        };
        let packet = &buf[..len];

        if let Err(e) = node.send_to(&exit, packet).await {
            tracing::error!("failed to forward packet to exit node {exit}: {e}");
            break;
        }
        tracing::debug!("forwarded {} bytes to exit node {exit}", packet.len());
    }
}

#[cfg(windows)]
pub fn setup_routing(exit_node_ip: IpAddr) -> Result<(), Box<dyn std::error::Error>> {
    let gateway = get_default_gateway()?;
    let manager = RouteManager::new()?;

    manager.add_route(&Route::new(exit_node_ip, 32).gateway(gateway))?;

    let tun_gateway: IpAddr = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
    manager.add_route(
        &Route::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0)
            .gateway(tun_gateway)
            .metric(1),
    )?;

    Ok(())
}

#[cfg(windows)]
pub fn teardown_routing(exit_node_ip: IpAddr) -> Result<(), Box<dyn std::error::Error>> {
    let manager = RouteManager::new()?;
    manager.delete_route(&Route::new(exit_node_ip, 32))?;
    manager.delete_route(&Route::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0))?;
    Ok(())
}

#[cfg(windows)]
pub fn get_default_gateway() -> Result<IpAddr, Box<dyn std::error::Error>> {
    let interfaces = netdev::get_interfaces();
    for iface in interfaces {
        if let Some(gateway) = iface.gateway {
            if let Some(ip) = gateway.ipv4.first() {
                if !iface.ipv4.is_empty() {
                    return Ok(IpAddr::V4(*ip));
                }
            }
        }
    }
    Err("no default gateway found".into())
}

#[cfg(target_os = "linux")]
pub fn get_default_gateway() -> Result<IpAddr, Box<dyn std::error::Error>> {
    let output = std::process::Command::new("ip")
        .args(["route", "show", "default"])
        .output()?;
    if !output.status.success() {
        return Err("`ip route show default` failed".into());
    }
    let stdout = String::from_utf8(output.stdout)?;
    for line in stdout.lines() {
        let mut tokens = line.split_whitespace();
        // expected: "default via <gw> dev <iface> ..."
        if tokens.next() == Some("default") && tokens.next() == Some("via") {
            if let Some(gw) = tokens.next() {
                return Ok(gw.parse()?);
            }
        }
    }
    Err("no default gateway found".into())
}

#[cfg(target_os = "linux")]
pub fn setup_routing(exit_node_ip: IpAddr) -> Result<(), Box<dyn std::error::Error>> {
    let gateway = get_default_gateway()?;

    let status = std::process::Command::new("ip")
        .args([
            "route",
            "add",
            &format!("{}/32", exit_node_ip),
            "via",
            &gateway.to_string(),
        ])
        .status()?;
    if !status.success() {
        return Err(format!("failed to add exit-node route to {exit_node_ip}").into());
    }

    let status = std::process::Command::new("ip")
        .args(["route", "add", "default", "via", "10.0.0.1", "metric", "1"])
        .status()?;
    if !status.success() {
        return Err("failed to add default route via TUN gateway".into());
    }

    Ok(())
}

#[cfg(target_os = "linux")]
pub fn teardown_routing(exit_node_ip: IpAddr) -> Result<(), Box<dyn std::error::Error>> {
    let _ = std::process::Command::new("ip")
        .args(["route", "del", &format!("{}/32", exit_node_ip)])
        .status()?;
    let _ = std::process::Command::new("ip")
        .args(["route", "del", "default", "via", "10.0.0.1"])
        .status()?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let port: u16 = prompt("Local port", "1812").await.parse().unwrap_or(1812);
    let bootstrap = prompt("Bootstrap server URL", "http://localhost:1815").await;

    let mut node = Node::new(port, "10.0.0.1")?;
    node.listen(&bootstrap).await?;
    node.get_peers(&bootstrap).await;

    println!("Node started on port {port}");

    let mut exit_node: Option<SocketAddr> = None;

    loop {
        println!();
        println!("[1] List peers");
        println!("[2] Connect to peer");
        println!("[3] Status");
        println!("[q] Quit");
        print!("> ");

        let input = prompt("", "").await;

        match input.as_str() {
            "1" => {
                node.get_peers(&bootstrap).await;
                let peers = node.list_peers().await;
                if peers.is_empty() {
                    println!("No peers available.");
                } else {
                    for (i, peer) in peers.iter().enumerate() {
                        println!("[{i}] {peer}");
                    }
                }
            }
            "2" => {
                node.get_peers(&bootstrap).await;
                let peers = node.list_peers().await;
                if peers.is_empty() {
                    println!("No peers available.");
                    continue;
                }
                for (i, peer) in peers.iter().enumerate() {
                    println!("[{i}] {peer}");
                }
                let idx: usize = prompt("Select peer index", "0").await.parse().unwrap_or(0);
                if let Some(&addr) = peers.get(idx) {
                    match node.connect(addr).await {
                        Ok(_) => {
                            println!("Connected to {addr}");
                            exit_node = Some(addr);
                            if let Err(e) = setup_routing(addr.ip()) {
                                eprintln!("Warning: routing setup failed: {e}");
                            }
                            tokio::select! {
                                _ = run_tun(&mut node, addr) => {}
                                _ = tokio::signal::ctrl_c() => {
                                    println!("Shutting down...");
                                    if let Err(e) = teardown_routing(addr.ip()) {
                                        eprintln!("Warning: routing teardown failed: {e}");
                                    }
                                }
                            }
                        }
                        Err(e) => eprintln!("Connection failed: {e}"),
                    }
                }
            }
            "3" => match exit_node {
                Some(addr) => println!("Connected — exit node: {addr}"),
                None => println!("No active connection."),
            },
            "q" => {
                if let Some(addr) = exit_node {
                    if let Err(e) = teardown_routing(addr.ip()) {
                        eprintln!("Warning: routing teardown failed: {e}");
                    } else {
                        println!("Routes restored.");
                    }
                }
                break;
            }
            _ => eprintln!("Invalid option."),
        }
    }

    Ok(())
}
