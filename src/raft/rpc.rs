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
    pub leader_addr: String,
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

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "op", content = "data")]
pub enum ClientRequest {
    Get { key: String },
    Put { key: String, value: String },
    Delete { key: String },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "status", content = "data")]
pub enum ClientResponse {
    Ok,
    Value(Option<String>),
    NotLeader { leader_id: Option<u64>, leader_addr: Option<String> },
    Error { message: String },
}

/// Top-level RPC request envelope.
///
/// This avoids "guessing" message types based on JSON shape and makes the protocol extensible
/// (e.g., adding client-facing requests later) without breaking decoding.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", content = "data")]
pub enum RpcRequest {
    RequestVote(RequestVoteArgs),
    AppendEntries(AppendEntriesArgs),
    Client(ClientRequest),
}

/// Top-level RPC response envelope.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", content = "data")]
pub enum RpcResponse {
    RequestVote(RequestVoteReply),
    AppendEntries(AppendEntriesReply),
    Client(ClientResponse),
}

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{timeout, Duration};
use anyhow::{Result, anyhow};
use std::sync::{Arc, Mutex};
use crate::raft::log::Command;
use crate::raft::state::{RaftNode, Role};

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

    let req: RpcRequest = serde_json::from_slice(&buf)?;
    let resp = match req {
        RpcRequest::RequestVote(args) => {
            let reply = {
                let mut node = node.lock().unwrap();
                node.handle_request_vote(args)
            };
            RpcResponse::RequestVote(reply)
        }
        RpcRequest::AppendEntries(args) => {
            let reply = {
                let mut node = node.lock().unwrap();
                node.handle_append_entries(args)
            };
            RpcResponse::AppendEntries(reply)
        }
        RpcRequest::Client(req) => {
            let reply = handle_client_rpc(node.clone(), req).await;
            RpcResponse::Client(reply)
        }
    };

    let out = serde_json::to_vec(&resp)?;
    write_frame(&mut socket, &out).await?;
    Ok(())
}

async fn handle_client_rpc(node: Arc<Mutex<RaftNode>>, req: ClientRequest) -> ClientResponse {
    match req {
        ClientRequest::Get { key } => {
            let guard = node.lock().unwrap();
            if guard.role != Role::Leader {
                return ClientResponse::NotLeader {
                    leader_id: guard.leader_id,
                    leader_addr: guard.leader_addr.clone(),
                };
            }
            ClientResponse::Value(guard.kv.get(&key))
        }
        ClientRequest::Put { key, value } => {
            handle_client_write(node, Command::Put(key, value)).await
        }
        ClientRequest::Delete { key } => handle_client_write(node, Command::Delete(key)).await,
    }
}

async fn handle_client_write(node: Arc<Mutex<RaftNode>>, cmd: Command) -> ClientResponse {
    let (index, term, notify) = {
        let mut guard = node.lock().unwrap();
        if guard.role != Role::Leader {
            return ClientResponse::NotLeader {
                leader_id: guard.leader_id,
                leader_addr: guard.leader_addr.clone(),
            };
        }
        let term = guard.current_term;
        let index = match guard.propose_command(cmd) {
            Ok(index) => index,
            Err(e) => {
                return ClientResponse::Error {
                    message: format!("failed to append entry: {e:?}"),
                };
            }
        };
        (index, term, guard.commit_notify.clone())
    };

    let wait = async {
        loop {
            {
                let guard = node.lock().unwrap();
                if guard.role != Role::Leader || guard.current_term != term {
                    return ClientResponse::NotLeader {
                        leader_id: guard.leader_id,
                        leader_addr: guard.leader_addr.clone(),
                    };
                }
                if guard.commit_index >= index {
                    return ClientResponse::Ok;
                }
            }

            tokio::select! {
                _ = notify.notified() => {}
                _ = tokio::time::sleep(Duration::from_millis(50)) => {}
            }
        }
    };

    match timeout(Duration::from_secs(5), wait).await {
        Ok(resp) => resp,
        Err(_) => ClientResponse::Error {
            message: "timed out waiting for commit".to_string(),
        },
    }
}

/// Send a RequestVote RPC to a peer and decode the reply.
pub async fn send_request_vote(addr: &str, args: &RequestVoteArgs) -> Result<RequestVoteReply> {
    let mut stream = TcpStream::connect(addr).await?;

    let req = RpcRequest::RequestVote(args.clone());
    let data = serde_json::to_vec(&req)?;
    write_frame(&mut stream, &data).await?;

    let buf = read_frame(&mut stream).await?;
    let resp: RpcResponse = serde_json::from_slice(&buf)?;
    match resp {
        RpcResponse::RequestVote(reply) => Ok(reply),
        other => Err(anyhow!("unexpected response to RequestVote: {other:?}")),
    }
}

/// Send an AppendEntries (heartbeat or replication) RPC to a peer.
pub async fn send_append_entries(addr: &str, args: &AppendEntriesArgs) -> Result<AppendEntriesReply> {
    let mut stream = TcpStream::connect(addr).await?;
    let req = RpcRequest::AppendEntries(args.clone());
    let data = serde_json::to_vec(&req)?;
    write_frame(&mut stream, &data).await?;

    let buf = read_frame(&mut stream).await?;
    let resp: RpcResponse = serde_json::from_slice(&buf)?;
    match resp {
        RpcResponse::AppendEntries(reply) => Ok(reply),
        other => Err(anyhow!("unexpected response to AppendEntries: {other:?}")),
    }
}

/// Send a client request to a node and decode the reply.
pub async fn send_client_request(addr: &str, req: &ClientRequest) -> Result<ClientResponse> {
    let mut stream = TcpStream::connect(addr).await?;
    let wrapped = RpcRequest::Client(req.clone());
    let data = serde_json::to_vec(&wrapped)?;
    write_frame(&mut stream, &data).await?;

    let buf = read_frame(&mut stream).await?;
    let resp: RpcResponse = serde_json::from_slice(&buf)?;
    match resp {
        RpcResponse::Client(reply) => Ok(reply),
        other => Err(anyhow!("unexpected response to Client request: {other:?}")),
    }
}
