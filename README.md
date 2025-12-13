
# Raft-Lite Distributed Key–Value Store (Rust)

A minimal, educational implementation of the Raft Consensus Algorithm in Rust, featuring leader election, log replication, conflict resolution, crash recovery, and a write-ahead log (WAL).
This project demonstrates how real databases like etcd, CockroachDB, and TiKV maintain strong consistency across distributed nodes. Built to learn some stuff.

## Features

* Leader Election
* Heartbeats
* Log Replication
* Log Conflict Repair (next_index / match_index)
* Commit Index & Ordered State Machine Application
* Write-Ahead Log (WAL) using bincode
* Crash Recovery
* Asynchronous Networking (Tokio TCP)
* Shared Raft State with Arc<Mutex<>>

## Project Structure

```
src/
 ├── main.rs              # Node startup, argument parsing
 ├── raft/
 │    ├── state.rs        # Raft roles, terms, log pointers
 │    ├── rpc.rs          # TCP RPC server + client
 │    └── log.rs          # Write-Ahead Log
 ├── kv.rs                # The replicated key-value store
 └── util.rs
```

Each node runs:

* a Raft state machine
* a persistent WAL
* a KV store
* an asynchronous RPC server

All state is protected by:

```
Arc<Mutex<RaftNode>>
```

## Raft Overview

Raft ensures all nodes in the cluster agree on the same sequence of log entries.
The algorithm has three key components:

1. Leader Election
2. Log Replication
3. Safety Rules

This implementation follows the Raft paper’s Figure 2.


## Leader Election

All nodes start as followers.

Each follower has a randomized election timeout:

```
150–300 ms
```

If a follower does not receive a heartbeat before timeout:

```
Follower → Candidate
term += 1
vote for self
broadcast RequestVote to peers
```

A candidate becomes leader if:

* it receives majority votes
* its log is at least as up-to-date as others
* no other candidate has a higher term

Once elected:

```
role = Leader
initialize next_index and match_index
start sending heartbeats
```



## Heartbeats

Leaders send periodic AppendEntries calls with no log entries:

```
AppendEntries(term, entries = [])
```

Followers reset their election timers when receiving a heartbeat.

If a leader stops sending heartbeats, followers start a new election.



## Log Replication

When the leader receives a client write:

```
PUT x = 5
```

Sequence:

1. Leader appends entry to its local log
2. Leader sends AppendEntries(entries=[entry]) to followers
3. Followers append entry and respond
4. When a majority replicate it → entry is committed
5. Leader applies entry to KV store
6. Followers apply entry after receiving leader_commit

Internal structures:

```
next_index[f]  // next log index to send to follower f
match_index[f] // highest index known replicated on follower f
commit_index   // highest index committed on majority
last_applied   // highest index applied to KV state machine
```



## Log Conflict Resolution

If a follower’s log conflicts with the leader’s:

```
Leader log:   A B C D
Follower log: A X Y
```

Follower rejects AppendEntries.

Leader repairs by decrementing next_index:

```
next_index[f] -= 1
retry AppendEntries
```

Eventually, leader finds the matching prefix:

```
A
```

Follower deletes incorrect entries:

```
A X Y → A
```

Leader overwrites with correct entries:

```
A B C D
```

This guarantees log convergence across nodes.



## Write-Ahead Log (WAL)

Entries are serialized as:

```
[length: u32][entry bytes...]
```

On startup:

```
load WAL → rebuild log → reapply state
```

This simulates crash recovery:

* committed entries are applied to KV store
* uncommitted entries remain in the log



## RPC Protocol

### RequestVote

```
RequestVoteArgs {
  term,
  candidate_id,
  last_log_index,
  last_log_term,
}

RequestVoteReply {
  term,
  vote_granted,
}
```

### AppendEntries

```
AppendEntriesArgs {
  term,
  leader_id,
  prev_log_index,
  prev_log_term,
  entries: Vec<LogEntry>,
  leader_commit,
}

AppendEntriesReply {
  term,
  success,
}
```

Networking is implemented with Tokio TCP, and message encoding uses serde_json.



## Raft Behavior Cases

### Leader Timeout

If heartbeat is lost, followers start a new election.

### Split Vote

Randomized timeouts prevent endless ties.

### Slow Leader

Followers time out and elect a new leader; old leader steps down if it sees a higher term.

### Log Inconsistency

Leader resolves disagreement via next_index backtracking.

### Crash Recovery

WAL is loaded, state machine rebuilt.

### Network Partition

Majority partition elects leader; minority stays followers.

### Rejoining Node

If stale, it steps down and catches up using AppendEntries.



## Running Nodes Locally

Example with 3 nodes:

```
cargo run -- --id 1 --port 5001 --peers 127.0.0.1:5002,127.0.0.1:5003
cargo run -- --id 2 --port 5002 --peers 127.0.0.1:5001,127.0.0.1:5003
cargo run -- --id 3 --port 5003 --peers 127.0.0.1:5001,127.0.0.1:5002
```

You will observe:

* leader election
* heartbeats
* log replication
* commit progression



## Future Improvements

* Log compaction (snapshotting)
* Membership changes (joint consensus)
* Stable storage for term & voted_for
* Stronger testing with simulated partitions



## About

This project was built to develop a deep understanding of:

* distributed consensus
* state machine replication
* durability guarantees
* fault-tolerant distributed systems
* Rust async programming

It demonstrates the core ideas behind popular database systems such as etcd, TiKV, CockroachDB, and Consul.
