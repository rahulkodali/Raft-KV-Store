use serde::{Serialize, Deserialize};
use crate::raft::log::LogEntry;

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
use crate::raft::state::RaftNode;

const MAX_FRAME_LEN: u32 = 16 * 1024 * 1024; // 16 MiB

async fn read_frame(stream: &mut TcpStream) -> Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf);

    if len > MAX_FRAME_LEN {
        return Err(anyhow!("frame too large: {len} > {MAX_FRAME_LEN}"));
    }

    let mut buf = vec![0u8; len as usize];
    stream.read_exact(&mut buf).await?;
    Ok(buf)
}

async fn write_frame(stream: &mut TcpStream, payload: &[u8]) -> Result<()> {
    let len: u32 = payload
        .len()
        .try_into()
        .map_err(|_| anyhow!("payload too large: {} bytes", payload.len()))?;
    stream.write_all(&len.to_be_bytes()).await?;
    stream.write_all(payload).await?;
    stream.flush().await?;
    Ok(())
}

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
    // Use explicit framing instead of `read_to_end()`: relying on connection-close for message
    // boundaries can deadlock when both sides wait for EOF before responding.
    let buf = read_frame(&mut socket).await?;

    if let Ok(args) = serde_json::from_slice::<RequestVoteArgs>(&buf) {
        let reply = {
            let mut node = node.lock().unwrap();
            node.handle_request_vote(args)
        };
        let out = serde_json::to_vec(&reply)?;
        write_frame(&mut socket, &out).await?;
    } else if let Ok(args) = serde_json::from_slice::<AppendEntriesArgs>(&buf) {
        let reply = {
            let mut node = node.lock().unwrap();
            node.handle_append_entries(args)
        };
        let out = serde_json::to_vec(&reply)?;
        write_frame(&mut socket, &out).await?;
    } else {
        return Err(anyhow!("unknown RPC payload"));
    }
    Ok(())
}

/// Send a RequestVote RPC to a peer and decode the reply.
pub async fn send_request_vote(addr: &str, args: &RequestVoteArgs) -> Result<RequestVoteReply> {
    let mut stream = TcpStream::connect(addr).await?;

    let data = serde_json::to_vec(args)?;
    write_frame(&mut stream, &data).await?;

    let buf = read_frame(&mut stream).await?;
    let reply: RequestVoteReply = serde_json::from_slice(&buf)?;

    Ok(reply)
}

/// Send an AppendEntries (heartbeat or replication) RPC to a peer.
pub async fn send_append_entries(addr: &str, args: &AppendEntriesArgs) -> Result<AppendEntriesReply> {
    let mut stream = TcpStream::connect(addr).await?;
    let data = serde_json::to_vec(args)?;
    write_frame(&mut stream, &data).await?;

    let buf = read_frame(&mut stream).await?;
    let reply: AppendEntriesReply = serde_json::from_slice(&buf)?;
    Ok(reply)
}
