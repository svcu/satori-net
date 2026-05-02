use net_core::{Node, Session};

#[tokio::main]
async fn main() {
    let mut node_a = Node::new(1812);
    let mut node_b = Node::new(1813);

    node_a.listen().await;
    println!("node_a peers: {:?}", node_a.list_peers().await);

    node_b.listen().await;

    if let Some(&addr) = node_b.list_peers().await.last() {
        match Session::connect(addr).await {
            Ok(mut session) => {
                println!("connected to {addr}");
                session.send(b"hello").await.unwrap();
            }
            Err(e) => eprintln!("failed to connect: {e}"),
        }
    }

    std::future::pending::<()>().await;
}
