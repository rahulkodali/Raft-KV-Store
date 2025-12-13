mod raft;

use std::env;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::raft::kv::KvStore;
use crate::raft::log::Wal;
use crate::raft::rpc::start_rpc_server;
use crate::raft::state::{spawn_tick_loop, RaftNode};

/// Entry point for launching a single Raft node and its RPC server.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    let id = args.get(1).and_then(|s| s.parse::<u64>().ok()).unwrap_or(1);
    let addr = args.get(2).cloned().unwrap_or_else(|| "127.0.0.1:5001".to_string());
    let peers: Vec<String> = if args.len() > 3 { args[3..].to_vec() } else { Vec::new() };

    let wal = Wal::open(&format!("raft-{id}.log"))?;
    let mut kv = KvStore::new();
    for entry in &wal.entries {
        kv.apply(&entry.cmd);
    }

    let node = Arc::new(Mutex::new(RaftNode::new(id, peers.clone(), wal, kv)));

    spawn_tick_loop(node.clone());

    let server_node = node.clone();
    let addr_clone = addr.clone();
    tokio::spawn(async move {
        if let Err(e) = start_rpc_server(server_node, &addr_clone).await {
            eprintln!("RPC server failed: {:?}", e);
        }
    });

    println!("Raft node {id} listening on {addr} with peers {:?}", peers);
    println!("Press Ctrl+C to exit");

    tokio::signal::ctrl_c().await?;
    println!("Shutdown signal received, exiting after 1s...");
    tokio::time::sleep(Duration::from_secs(1)).await;

    Ok(())
}
