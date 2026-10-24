use std::io::{self, Read, Write};

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Largest frame accepted or sent, in bytes. Messages are small; this only
/// exists so a broken or hostile peer can't make the other side allocate
/// gigabytes.
pub const MAX_FRAME: usize = 1024 * 1024;

/// Why a frame could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    /// The peer closed the connection cleanly between frames.
    #[error("connection closed")]
    Closed,
    /// Socket error, or the connection dropped mid-frame.
    #[error("connection error: {0}")]
    Io(#[from] io::Error),
    /// The length prefix is larger than [`MAX_FRAME`].
    #[error("frame of {0} bytes is larger than the {MAX_FRAME}-byte limit")]
    TooLarge(usize),
    /// The payload is not the JSON we expected.
    #[error("malformed message: {0}")]
    Json(#[from] serde_json::Error),
}

/// Write one message as a length-prefixed JSON frame and flush.
pub fn write_frame<W: Write, T: Serialize>(w: &mut W, msg: &T) -> Result<(), FrameError> {
    let body = serde_json::to_vec(msg)?;
    if body.len() > MAX_FRAME {
        return Err(FrameError::TooLarge(body.len()));
    }
    let len = u32::try_from(body.len()).map_err(|_| FrameError::TooLarge(body.len()))?;
    let mut buf = Vec::with_capacity(4 + body.len());
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(&body);
    w.write_all(&buf)?;
    w.flush()?;
    Ok(())
}

/// Read one frame's raw bytes.
pub(crate) fn read_frame_bytes<R: Read>(r: &mut R) -> Result<Vec<u8>, FrameError> {
    let mut len = [0u8; 4];
    let mut got = 0;
    while got < 4 {
        match r.read(&mut len[got..]) {
            Ok(0) if got == 0 => return Err(FrameError::Closed),
            Ok(0) => return Err(FrameError::Io(io::ErrorKind::UnexpectedEof.into())),
            Ok(n) => got += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(FrameError::Io(e)),
        }
    }
    let len = u32::from_le_bytes(len) as usize;
    if len > MAX_FRAME {
        return Err(FrameError::TooLarge(len));
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body)?;
    Ok(body)
}

/// Read one length-prefixed JSON frame and decode it.
pub fn read_frame<R: Read, T: DeserializeOwned>(r: &mut R) -> Result<T, FrameError> {
    let body = read_frame_bytes(r)?;
    Ok(serde_json::from_slice(&body)?)
}
