use serde::{Serialize, Deserialize};
use crate::log::LogEntry;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RequestVoteArgs {
    pub term: u64,
    pub candidate_id: u64,
    pub last_log_index: u64,
    pub last_log_term: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RequestVoteReply {
    pub term: u64,
    pub vote_granted: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppendEntriesArgs {
    pub term: u64,
    pub leader_id: u64,
    pub entries: Vec<LogEntry>,
    pub prev_log_index: u64,
    pub prev_log_term: u64,
    pub leader_commit: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppendEntriesReply {
    pub term: u64,
    pub success: bool
}

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use anyhow::{Result, anyhow};
use std::sync::{Arc, Mutex};
use crate::state::RaftNode;

/// Start a TCP server to handle incoming Raft RPCs.
pub async fn start_rpc_server(node: Arc<Mutex<RaftNode>>, addr: &str) -> Result<()> {
    let listener = TcpListener::bind(addr).await?;

    loop {
        let (socket, _) = listener.accept().await?;
        let node = node.clone();

        // handle connection to be implemented
        tokio::spawn(async move {
            if let Err(e) = handle_connection(node, socket).await {
                eprintln!("RPC handler error: {:?}", e);
            }
        });
    }
}

/// Decode a single inbound RPC request and write the corresponding reply.
async fn handle_connection(node: Arc<Mutex<RaftNode>>, mut socket: TcpStream) -> Result<()> {

    let mut buf = Vec::new();
    socket.read_to_end(&mut buf).await?;

    if let Ok(args) = serde_json::from_slice::<RequestVoteArgs>(&buf) {
        let reply = {
            let mut node = node.lock().unwrap();
            node.handle_request_vote(args)
        };
        let out = serde_json::to_vec(&reply)?;
        socket.write_all(&out).await?;
    } else if let Ok(args) = serde_json::from_slice::<AppendEntriesArgs>(&buf) {
        let reply = {
            let mut node = node.lock().unwrap();
            node.handle_append_entries(args)
        };
        let out = serde_json::to_vec(&reply)?;
        socket.write_all(&out).await?;
    } else {
        return Err(anyhow!("unknown RPC payload"));
    }
    Ok(())
}

/// Send a RequestVote RPC to a peer and decode the reply.
pub async fn send_request_vote(addr: &str, args: &RequestVoteArgs) -> Result<RequestVoteReply> {
    let mut stream = TcpStream::connect(addr).await?;

    let data = serde_json::to_vec(args)?;
    stream.write_all(&data).await?;

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await?;
    let reply: RequestVoteReply = serde_json::from_slice(&buf)?;

    Ok(reply)
}

/// Send an AppendEntries (heartbeat or replication) RPC to a peer.
pub async fn send_append_entries(addr: &str, args: &AppendEntriesArgs) -> Result<AppendEntriesReply> {
    let mut stream = TcpStream::connect(addr).await?;
    let data = serde_json::to_vec(args)?;
    stream.write_all(&data).await?;

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await?;
    let reply: AppendEntriesReply = serde_json::from_slice(&buf)?;
    Ok(reply)
}
