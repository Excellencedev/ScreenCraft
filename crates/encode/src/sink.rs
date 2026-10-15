//! Where `WriteAt` operations end up. The muxers only describe writes; these apply them.

use crate::types::WriteAt;
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

pub trait Sink {
    fn apply(&mut self, op: &WriteAt) -> std::io::Result<()>;
}

/// In-memory file, used by tests and by the determinism check.
#[derive(Default)]
pub struct MemorySink {
    pub bytes: Vec<u8>,
}

impl Sink for MemorySink {
    fn apply(&mut self, op: &WriteAt) -> std::io::Result<()> {
        let end = op.offset as usize + op.data.len();
        if self.bytes.len() < end {
            self.bytes.resize(end, 0);
        }
        self.bytes[op.offset as usize..end].copy_from_slice(&op.data);
        Ok(())
    }
}

pub struct FileSink {
    file: File,
    pos: u64,
}

impl FileSink {
    pub fn create(path: &Path) -> std::io::Result<Self> {
        Ok(Self { file: File::create(path)?, pos: 0 })
    }
}

impl Sink for FileSink {
    fn apply(&mut self, op: &WriteAt) -> std::io::Result<()> {
        if self.pos != op.offset {
            self.file.seek(SeekFrom::Start(op.offset))?;
        }
        self.file.write_all(&op.data)?;
        self.pos = op.offset + op.data.len() as u64;
        Ok(())
    }
}
