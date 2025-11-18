use serde::{Serialize, Deserialize};
use crate::log::LogEntry;

#[derive(Serialize, Deserialize, Debug)]
pub struct RequestVoteArgs {
    pub term: u64,
    pub candidate_id: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RequestVoteReply {
    pub term: u64,
    pub vote_granted: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AppendEntries {
    pub term: u64,
    pub leader_id: u64,
    pub entries: Vec<LogEntry>,
    pub prev_log_index: u64,
    pub prev_log_term: u64,
    pub leader_commit: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AppendEntriesReply {
    pub term: u64,
    pub success: bool
}
