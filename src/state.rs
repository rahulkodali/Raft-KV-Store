use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::time;

use crate::log::{LogEntry, Wal};
use crate::kv::KvStore;
use crate::rpc::{
    AppendEntriesArgs, AppendEntriesReply, RequestVoteArgs, RequestVoteReply, send_append_entries,
    send_request_vote,
};


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
    /// Construct a new Raft node with empty volatile state and a loaded WAL/KV.
    pub fn new(id: u64, peers: Vec<String>, wal: Wal, kv: KvStore) -> Self {
        let mut node = RaftNode {
            id,
            peers,
            role: Role::Follower,
            current_term: 0,
            voted_for: None,
            log: wal,
            kv,
            commit_index: 0,
            last_applied: 0,
            next_index: Vec::new(),
            match_index: Vec::new(),
            election_deadline: Instant::now(),
        };
        node.reset_election_timeout();
        node
    }

    /// Refresh the randomized election timeout window.
    pub fn reset_election_timeout(&mut self) {
        let ms = 150 + rand::random::<u64>() % 150;
        self.election_deadline = Instant::now() + Duration::from_millis(ms);
    }

    /// Advance time-driven logic; trigger an election if this follower/candidate times out.
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

    // after this loop through peers and send vote requests -> if majority call become_leader() else change back to Follower
    /// Enter candidate role, bump term, self-vote, and return vote request + peer list.
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

    // after this start sending heartbeats
    /// Transition to leader and initialize replication tracking indices.
    pub fn become_leader(&mut self) {
        self.role = Role::Leader;
        let last_index = self.log.entries.len() as u64;

        self.next_index = vec![last_index + 1; self.peers.len()];
        self.match_index = vec![0; self.peers.len()];
    }

    /// Handle an inbound RequestVote RPC and decide whether to grant the vote.
    pub fn handle_request_vote(&mut self, args: RequestVoteArgs) -> RequestVoteReply {
        // case 1 if we want to vote for the candidate
        if args.term > self.current_term {
            self.current_term = args.term;
            self.role = Role::Follower;
            self.voted_for = None;
        }

        let mut vote_granted = false;
        if args.term == self.current_term {
            let log_ok = self.log_up_to_date(args.last_log_index, args.last_log_term);
            let can_vote = self.voted_for.is_none() || self.voted_for == Some(args.candidate_id);
            // same case 1 we want to vote for candidate
            if can_vote && log_ok {
                self.voted_for = Some(args.candidate_id);
                vote_granted = true;
                self.reset_election_timeout();
            }
        } else if args.term < self.current_term {
            vote_granted = false;
        }

        RequestVoteReply {
            term: self.current_term,
            vote_granted,
        }
    }

    /// Handle AppendEntries/heartbeat RPCs, updating log state and commit index.
    pub fn handle_append_entries(&mut self, args: AppendEntriesArgs) -> AppendEntriesReply {
        if args.term < self.current_term {
            return AppendEntriesReply {
                term: self.current_term,
                success: false,
            };
        }

        if args.term > self.current_term {
            self.current_term = args.term;
            self.role = Role::Follower;
            self.voted_for = None;
        }

        self.reset_election_timeout();

        if args.prev_log_index > self.log.last_index() {
            return AppendEntriesReply {
                term: self.current_term,
                success: false,
            };
        }

        if let Some(prev_term) = self.log.term_at(args.prev_log_index) {
            if prev_term != args.prev_log_term {
                return AppendEntriesReply {
                    term: self.current_term,
                    success: false,
                };
            }
        } else if args.prev_log_index != 0 {
            return AppendEntriesReply {
                term: self.current_term,
                success: false,
            };
        }

        for entry in &args.entries {
            let expected_next = self.log.last_index() + 1;
            if entry.index < expected_next {
                let position = (entry.index - 1) as usize;
                if let Some(existing) = self.log.entries.get(position) {
                    if existing.term != entry.term {
                        let _ = self.log.truncate_from(entry.index);
                        let _ = self.log.append(entry);
                    }
                }
            } else if entry.index == expected_next {
                let _ = self.log.append(entry);
            }
        }

        if args.leader_commit > self.commit_index {
            self.commit_index = args.leader_commit.min(self.log.last_index());
            self.apply_committed_entries();
        }

        AppendEntriesReply {
            term: self.current_term,
            success: true,
        }
    }

    /// Check whether a candidate's log is at least as up-to-date as ours.
    fn log_up_to_date(&self, other_last_index: u64, other_last_term: u64) -> bool {
        let last_term = self.log.entries.last().map(|e| e.term).unwrap_or(0);
        let last_index = self.log.entries.len() as u64;
        
        // if new term > current or if terms are the same but new log has more data
        other_last_term > last_term || (other_last_term == last_term && other_last_index >= last_index)
    }

    /// Build AppendEntries arguments for a specific follower index.
    fn heartbeat_args(&self, peer_idx: usize) -> AppendEntriesArgs {
        let next_index = self.next_index[peer_idx];
        let prev_log_index = next_index.saturating_sub(1);
        let prev_log_term = self.log.term_at(prev_log_index).unwrap_or(0);

        let entries = if next_index <= self.log.last_index() && next_index > 0 {
            let start = (next_index - 1) as usize;
            self.log.entries[start..].to_vec()
        } else {
            Vec::new()
        };

        AppendEntriesArgs {
            term: self.current_term,
            leader_id: self.id,
            prev_log_index,
            prev_log_term,
            entries,
            leader_commit: self.commit_index,
        }
    }

    /// Apply committed log entries to the key-value state machine.
    fn apply_committed_entries(&mut self) {
        while self.last_applied < self.commit_index {
            self.last_applied += 1;
            if let Some(entry) = self.log.entries.get((self.last_applied - 1) as usize) {
                self.kv.apply(&entry.cmd);
            }
        }
    }

    /// Advance commit index based on majority match indices and apply newly committed entries.
    fn update_commit_index(&mut self) {
        let mut match_indexes = self.match_index.clone();
        match_indexes.push(self.log.last_index());
        match_indexes.sort_unstable();
        let majority_pos = match_indexes.len() / 2;
        let candidate = match_indexes[majority_pos];
        if candidate > self.commit_index {
            if let Some(term) = self.log.term_at(candidate) {
                if term == self.current_term {
                    self.commit_index = candidate;
                    self.apply_committed_entries();
                }
            }
        }
    }
}

/// Periodic timer loop that triggers elections on timeout.
pub fn spawn_tick_loop(node: Arc<Mutex<RaftNode>>) {
    tokio::spawn(async move {
        let mut ticker = time::interval(Duration::from_millis(50));
        loop {
            ticker.tick().await;
            let election = {
                let mut guard = node.lock().unwrap();
                guard.tick_loop()
            };

            if let Some((args, peers)) = election {
                spawn_election(node.clone(), args, peers);
            }
        }
    });
}

/// Launch an asynchronous election attempt and promote to leader on majority votes.
fn spawn_election(node: Arc<Mutex<RaftNode>>, args: RequestVoteArgs, peers: Vec<String>) {
    tokio::spawn(async move {
        let total_nodes = peers.len() as u64 + 1;
        let majority = total_nodes / 2 + 1;
        let mut votes = 1; // self vote

        for peer in peers {
            if let Ok(reply) = send_request_vote(&peer, &args).await {
                if reply.term > args.term {
                    let mut guard = node.lock().unwrap();
                    if reply.term > guard.current_term {
                        guard.current_term = reply.term;
                        guard.role = Role::Follower;
                        guard.voted_for = None;
                        guard.reset_election_timeout();
                    }
                    return;
                }
                if reply.vote_granted {
                    votes += 1;
                }
            }
        }

        let mut guard = node.lock().unwrap();
        if guard.role == Role::Candidate && guard.current_term == args.term && votes >= majority {
            guard.become_leader();
            start_heartbeat_loop(node.clone());
        }
    });
}

/// Send periodic heartbeats and log replications to all peers while this node is leader.
pub fn start_heartbeat_loop(node: Arc<Mutex<RaftNode>>) {
    tokio::spawn(async move {
        let mut ticker = time::interval(Duration::from_millis(100));
        loop {
            ticker.tick().await;
            let tasks = {
                let guard = node.lock().unwrap();
                if guard.role != Role::Leader {
                    return;
                }
                guard
                    .peers
                    .iter()
                    .enumerate()
                    .map(|(idx, peer)| (peer.clone(), guard.heartbeat_args(idx), idx))
                    .collect::<Vec<_>>()
            };

            if tasks.is_empty() {
                continue;
            }

            for (peer, args, idx) in tasks {
                let node_clone = node.clone();
                tokio::spawn(async move {
                    if let Ok(reply) = send_append_entries(&peer, &args).await {
                        let mut guard = node_clone.lock().unwrap();
                        if reply.term > guard.current_term {
                            guard.current_term = reply.term;
                            guard.role = Role::Follower;
                            guard.voted_for = None;
                            guard.reset_election_timeout();
                            return;
                        }
                        if guard.role != Role::Leader || guard.current_term != args.term {
                            return;
                        }
                        if reply.success {
                            let entries_len = args.entries.len() as u64;
                            guard.next_index[idx] = args.prev_log_index + entries_len + 1;
                            guard.match_index[idx] = guard.next_index[idx].saturating_sub(1);
                            guard.update_commit_index();
                        } else if guard.next_index[idx] > 1 {
                            guard.next_index[idx] -= 1;
                        }
                    }
                });
            }
        }
    });
}
