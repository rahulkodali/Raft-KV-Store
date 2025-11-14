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

    fn append(&mut self, entry: &LogEntry) -> Result<()> {
        //add u32 log entry length header into 4 byte header
        //add log entry itself serialized next
    }

}





