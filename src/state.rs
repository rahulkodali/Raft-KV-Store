use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

use crate::log::Wal;
use crate::log::LogEntry;
use crate::kv::KvStore;
use crate::rpc::RequestVoteArgs;
use crate::rpc::send_request_vote;


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

    pub election_deadline: Instant,
}

impl RaftNode {
    pub fn reset_election_timeout(&mut self) {
        let ms = 150 + rand::random::<u64>() % 150;
        self.election_deadline = Instant::now() + Duration::from_millis(ms);
    }

    pub fn tick_loop(&mut self) -> Option<(RequestVoteArgs, Vec<String>)> {
        // Leaders never time out
        if self.role == Role::Leader {
            return None;
        }

        // Election timeout?
        if Instant::now() > self.election_deadline {
            return Some(self.start_election());
        }

        None
    }

    pub fn start_election(&mut self) -> (RequestVoteArgs, Vec<String>) {

        self.role = Role::Candidate;
        self.current_term += 1;
        self.voted_for = Some(self.id);
        self.reset_election_timeout();

        let last_log_index = self.log.entries.len() as u64;
        let last_log_term = self.log.entries.last().map(|e| e.term).unwrap_or(0);

        let vote_request = RequestVoteArgs {
            term: self.current_term,
            candidate_id: self.id,
            last_log_index,
            last_log_term,
        };

        (vote_request, self.peers.clone())
    }
}