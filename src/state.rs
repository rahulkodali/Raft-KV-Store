use crate::log::Wal;
use crate::log::LogEntry;
use crate::kv::KvStore;

#[derive(Clone, PartialEq, Debug)]
pub enum Role {
    Follower,
    Leader,
    Candidate
}

pub struct RaftNode {
    pub id: u64,
    pub peers: Vec<String>,

    pub role: Role,
    pub current_term: u64,
    pub voted_for: Option<u64>,

    pub log: Wal,
    pub kv: KvStore,

    pub commit_index: u64, // index known to be commmitted across majority of nodes
    pub last_applied: u64, // index known to be stored in current Node's state machine

    pub next_index: Vec<u64>, // Only leader uses next_index to know where to start sending entries. tail of follower nodes pretty much
    pub match_index: Vec<u64>, // index known to be committed per each node

    pub election_deadline: std::time::Instant,
}

impl RaftNode {
    
}