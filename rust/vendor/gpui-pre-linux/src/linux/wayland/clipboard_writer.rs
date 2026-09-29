//! A bounded turn of a nonblocking clipboard transfer. The caller retains the
//! cursor and waits for write readiness when this returns Pending.
use std::io::{self, ErrorKind, Write};

pub(crate) const MAX_WRITE_BYTES: usize = 256 * 1024;
pub(crate) const MAX_WRITE_ATTEMPTS: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WriteProgress {
    Pending,
    Complete,
}

/// The writer must already be nonblocking. Both bytes and system-call attempts
/// are bounded so a large image, tiny partial writes or repeated EINTR cannot
/// monopolize the UI event loop. Errors are terminal for this transfer.
pub(crate) fn write_ready(
    writer: &mut impl Write,
    bytes: &[u8],
    written: &mut usize,
) -> io::Result<WriteProgress> {
    if *written > bytes.len() {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "clipboard write cursor exceeds payload",
        ));
    }
    if *written == bytes.len() {
        return Ok(WriteProgress::Complete);
    }
    let mut remaining = MAX_WRITE_BYTES;
    for _ in 0..MAX_WRITE_ATTEMPTS {
        let count = (bytes.len() - *written).min(remaining);
        match writer.write(&bytes[*written..*written + count]) {
            Ok(0) => {
                return Err(io::Error::new(
                    ErrorKind::WriteZero,
                    "clipboard receiver accepted no bytes",
                ));
            }
            Ok(count_written) if count_written > count => {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "clipboard writer exceeded the supplied buffer",
                ));
            }
            Ok(count_written) => {
                *written += count_written;
                remaining -= count_written;
                if *written == bytes.len() {
                    return Ok(WriteProgress::Complete);
                }
                if remaining == 0 {
                    break;
                }
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == ErrorKind::WouldBlock => break,
            Err(error) => return Err(error),
        }
    }
    Ok(WriteProgress::Pending)
}
