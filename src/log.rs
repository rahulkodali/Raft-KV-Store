use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Command {
    Put(String, String),
    Delete(String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LogEntry {
    pub index: u64,
    pub term: u64,
    pub cmd: Command,
}

use std::fs::{File, OpenOptions};
use std::io::{Read, Write, Seek, SeekFrom};
use anyhow::Result;

pub struct Wal{
    file: File,
    pub entries: Vector<LogEntry>,
}





