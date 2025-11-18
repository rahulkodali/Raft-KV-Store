mod log;
mod kv;

use crate::log::{Wal, LogEntry, Command};
use crate::kv::KvStore;
use std::fs;


fn main() -> anyhow::Result<()> {
    // fs::remove_file("raft.log")?;

    let mut wal = Wal::open("raft.log")?;

    let mut kv = KvStore::new();
    for entry in &wal.entries {
        kv.apply(&entry.cmd);
    }

    let new_index: u64 = wal.entries.len() as u64 + 1;


    let entry = LogEntry {
        index: new_index,
        term: 1,
        cmd: Command::Put("x".to_string(), "42".to_string()),
    };

    wal.append(&entry)?;
    wal.sync()?;

    println!("Loaded WAL entries:");
    for e in &wal.entries {
        println!("{:?}", e);
    }

    if let Some(val) = kv.get("x") {
        println!("x = {}", val);
    } else {
        println!("x not found");
    }

    Ok(())
}