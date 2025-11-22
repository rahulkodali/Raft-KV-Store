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

    pub fn tick_loop(&mut self) {
        if self.role == Role::Leader {
            return;
        }

        if Instant::now() > self.election_deadline {
            self.start_election();
        }
    }

    pub fn start_election(&mut self) {
        // lock raftNode

        self.role = Role::Candidate;
        self.current_term += 1;
        self.voted_for = Some(self.id);
        self.reset_election_timeout();


        let last_log_index = self.log.entries.len() as u64;
        let last_log_term =  self.log.entries.last().map(|e| e.term).unwrap_or(0);

        let voteRequest = RequestVoteArgs {
            term : last_log_term,
            candidate_id: self.id,
        };

        //unlock raftNode

        // loop through peers and spawn async tasks 
        // send_request_vote(addr, args);
    }
}