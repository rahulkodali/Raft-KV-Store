mod log;

use crate::log::{Wal, LogEntry, Command};
use std::fs;


fn main() -> anyhow::Result<()> {
    // fs::remove_file("raft.log")?;

    let mut wal = Wal::open("raft.log")?;
    let new_index = wal.entries.len() as u64 + 1;

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

    Ok(())
}