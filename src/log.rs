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
use std::io::{Read, Seek, SeekFrom, Write};
use anyhow::Result;

pub struct Wal{
    file: File,
    pub entries: Vec<LogEntry>,
}

impl Wal {
    pub fn open(path: &str) -> Result<Wal> {
        let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(path)?;

        let mut wal = Wal {
            file,
            entries: Vec::new(),
        };

        wal.load()?;
        Ok(wal)
    }

    fn load(&mut self) -> Result<()> {
        self.file.seek(SeekFrom::Start(0))?;
        let mut buf = Vec::new();
        self.file.read_to_end(&mut buf)?;

        let mut cursor = std::io::Cursor::new(buf);

        loop{

            let mut len_buf = [0u8; 4];
            if cursor.read_exact(&mut len_buf).is_err(){
                break;
            }

            let len = u32::from_le_bytes(len_buf) as usize;

            let mut data = vec![0u8; len];
            cursor.read_exact(&mut data)?;

            let entry: LogEntry = bincode::deserialize(&data)?;
            self.entries.push(entry);
        }
        Ok(())
    }

    pub fn append(&mut self, entry: &LogEntry) -> Result<()> {
        let data = bincode::serialize(entry)?;
        let len = data.len() as u32;
        let len_bytes = len.to_le_bytes();

        self.file.seek(SeekFrom::End(0))?;
        self.file.write_all(&len_bytes)?;
        self.file.write_all(&data)?;
        self.file.flush()?;

        self.entries.push(entry.clone());

        Ok(())
        //add u32 log entry length header into 4 byte header
        //add log entry itself serialized next
    }

    pub fn truncate_from(&mut self, start_index: u64) -> Result<()> {
        if start_index == 0 {
            return Ok(());
        }
        self.entries.retain(|entry| entry.index < start_index);
        self.rewrite_file()
    }

    fn rewrite_file(&mut self) -> Result<()> {
        self.file.set_len(0)?;
        self.file.seek(SeekFrom::Start(0))?;
        for entry in &self.entries {
            let data = bincode::serialize(entry)?;
            let len = data.len() as u32;
            self.file.write_all(&len.to_le_bytes())?;
            self.file.write_all(&data)?;
        }
        self.file.flush()?;
        Ok(())
    }

    pub fn last_index(&self) -> u64 {
        self.entries.last().map(|e| e.index).unwrap_or(0)
    }

    pub fn term_at(&self, index: u64) -> Option<u64> {
        if index == 0 {
            return Some(0);
        }
        let idx = (index - 1) as usize;
        self.entries.get(idx).map(|e| e.term)
    }

    pub fn sync(&mut self) -> Result<()> {
        self.file.sync_all()?;
        Ok(())
    }

}




