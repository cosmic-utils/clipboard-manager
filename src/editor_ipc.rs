//! IPC protocol for applet ↔ editor communication.
//! Uses length-prefixed JSON frames for robust message framing.
//!
//! Channels:
//! - Applet → Editor: child's stdin pipe
//! - Editor → Applet: dedicated FD 3 pipe (avoids stdout, which COSMIC writes to)

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::io::{self, Read, Write};

/// Messages from applet to editor process.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum AppToEditor {
    /// Initial payload with entry content to edit.
    Init {
        entry_id: i64,
        mime: String,
        content: String,
        /// When true, saves should update the existing entry in-place (favorites).
        /// When false, saves create a new clipboard entry (default behavior).
        #[serde(default)]
        is_favorite: bool,
    },
    /// The entry being edited was deleted — editor should close without saving.
    EntryDeleted,
    /// Applet requests editor to close (e.g., re-edit different entry).
    CloseRequested,
}

/// Messages from editor process to applet.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum EditorToApp {
    /// Editor is ready and displaying content.
    Ready,
    /// Save edited content as a new clipboard entry + copy to clipboard.
    SaveAsNew { content: String },
    /// Update the existing entry in-place (for favorites).
    UpdateExisting { content: String },
    /// Editor closed without changes.
    Closed,
}

/// Write a length-prefixed JSON frame to the writer.
pub fn write_frame(writer: &mut impl Write, msg: &impl Serialize) -> io::Result<()> {
    let json =
        serde_json::to_vec(msg).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let len = json.len() as u32;
    writer.write_all(&len.to_be_bytes())?;
    writer.write_all(&json)?;
    writer.flush()?;
    Ok(())
}

/// Read a length-prefixed JSON frame from the reader.
pub fn read_frame<T: DeserializeOwned>(reader: &mut impl Read) -> io::Result<T> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > 64 * 1024 * 1024 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too large"));
    }
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf)?;
    serde_json::from_slice(&buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}
