#[path = "../vendor/gpui-pre-linux/src/linux/wayland/clipboard_writer.rs"]
mod clipboard_writer;

use clipboard_writer::{MAX_WRITE_ATTEMPTS, MAX_WRITE_BYTES, WriteProgress, write_ready};
use std::{
    collections::VecDeque,
    io::{self, ErrorKind, Write},
};

enum Action {
    Accept(usize),
    Error(ErrorKind),
}

struct ScriptedWriter {
    actions: VecDeque<Action>,
    fallback_limit: usize,
    requests: Vec<usize>,
    received: Vec<u8>,
}

impl ScriptedWriter {
    fn new(actions: impl IntoIterator<Item = Action>) -> Self {
        Self {
            actions: actions.into_iter().collect(),
            fallback_limit: usize::MAX,
            requests: Vec::new(),
            received: Vec::new(),
        }
    }
}

impl Write for ScriptedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.requests.push(bytes.len());
        match self
            .actions
            .pop_front()
            .unwrap_or(Action::Accept(self.fallback_limit))
        {
            Action::Accept(limit) => {
                let count = limit.min(bytes.len());
                self.received.extend_from_slice(&bytes[..count]);
                Ok(count)
            }
            Action::Error(kind) => Err(io::Error::new(kind, "scripted clipboard writer")),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        panic!("a clipboard readiness callback must never flush or block")
    }
}

#[test]
fn partial_writes_send_each_byte_exactly_once() {
    let bytes = b"abcdefgh";
    let mut writer = ScriptedWriter::new([Action::Accept(2), Action::Accept(1), Action::Accept(5)]);
    let mut written = 0;
    assert_eq!(
        write_ready(&mut writer, bytes, &mut written).unwrap(),
        WriteProgress::Complete
    );
    assert_eq!(written, bytes.len());
    assert_eq!(writer.received, bytes);
    assert_eq!(writer.requests, [8, 6, 5]);
}

#[test]
fn a_slow_reader_yields_immediately_and_resumes_the_exact_suffix() {
    let bytes = b"a slow clipboard reader";
    let mut writer = ScriptedWriter::new([
        Action::Accept(3),
        Action::Error(ErrorKind::WouldBlock),
        Action::Accept(2),
        Action::Error(ErrorKind::WouldBlock),
    ]);
    let mut written = 0;
    assert_eq!(
        write_ready(&mut writer, bytes, &mut written).unwrap(),
        WriteProgress::Pending
    );
    assert_eq!(written, 3);
    assert_eq!(writer.requests.len(), 2);
    assert_eq!(writer.received, bytes[..3]);
    assert_eq!(
        write_ready(&mut writer, bytes, &mut written).unwrap(),
        WriteProgress::Pending
    );
    assert_eq!(written, 5);
    assert_eq!(writer.requests.len(), 4);
    assert_eq!(writer.received, bytes[..5]);
    assert_eq!(
        write_ready(&mut writer, bytes, &mut written).unwrap(),
        WriteProgress::Complete
    );
    assert_eq!(writer.received, bytes);
}

#[test]
fn blocked_before_the_first_byte_does_not_spin_or_advance() {
    let mut writer = ScriptedWriter::new([Action::Error(ErrorKind::WouldBlock)]);
    let mut written = 0;
    assert_eq!(
        write_ready(&mut writer, b"payload", &mut written).unwrap(),
        WriteProgress::Pending
    );
    assert_eq!(writer.requests.len(), 1);
    assert_eq!(written, 0);
    assert!(writer.received.is_empty());
}

#[test]
fn interrupted_writes_retry_the_same_suffix_without_losing_progress() {
    let bytes = b"interrupted";
    let mut writer = ScriptedWriter::new([
        Action::Error(ErrorKind::Interrupted),
        Action::Accept(2),
        Action::Error(ErrorKind::Interrupted),
        Action::Accept(usize::MAX),
    ]);
    let mut written = 0;
    assert_eq!(
        write_ready(&mut writer, bytes, &mut written).unwrap(),
        WriteProgress::Complete
    );
    assert_eq!(writer.requests, [11, 11, 9, 9]);
    assert_eq!(writer.received, bytes);
}

#[test]
fn repeated_interruptions_yield_at_the_attempt_limit() {
    let mut writer =
        ScriptedWriter::new((0..MAX_WRITE_ATTEMPTS).map(|_| Action::Error(ErrorKind::Interrupted)));
    let mut written = 0;
    assert_eq!(
        write_ready(&mut writer, b"retry", &mut written).unwrap(),
        WriteProgress::Pending
    );
    assert_eq!(writer.requests.len(), MAX_WRITE_ATTEMPTS);
    assert_eq!(written, 0);
    assert_eq!(
        write_ready(&mut writer, b"retry", &mut written).unwrap(),
        WriteProgress::Complete
    );
    assert_eq!(writer.received, b"retry");
}

#[test]
fn tiny_successful_writes_are_also_limited_to_one_turn_of_attempts() {
    let bytes: Vec<_> = (0..MAX_WRITE_ATTEMPTS * 2 + 3)
        .map(|n| (n % 251) as u8)
        .collect();
    let mut writer = ScriptedWriter::new([]);
    writer.fallback_limit = 1;
    let mut written = 0;
    for turn in 1..=2 {
        assert_eq!(
            write_ready(&mut writer, &bytes, &mut written).unwrap(),
            WriteProgress::Pending
        );
        assert_eq!(written, turn * MAX_WRITE_ATTEMPTS);
        assert_eq!(writer.requests.len(), turn * MAX_WRITE_ATTEMPTS);
        assert_eq!(writer.received, bytes[..written]);
    }
    assert_eq!(
        write_ready(&mut writer, &bytes, &mut written).unwrap(),
        WriteProgress::Complete
    );
    assert_eq!(writer.received, bytes);
}

#[test]
fn a_fast_reader_gets_at_most_256_kib_per_callback() {
    let bytes: Vec<_> = (0..MAX_WRITE_BYTES * 2 + 123)
        .map(|n| (n % 251) as u8)
        .collect();
    let mut writer = ScriptedWriter::new([]);
    let mut written = 0;
    for turn in 1..=2 {
        assert_eq!(
            write_ready(&mut writer, &bytes, &mut written).unwrap(),
            WriteProgress::Pending
        );
        assert_eq!(written, turn * MAX_WRITE_BYTES);
        assert_eq!(writer.requests.len(), turn);
        assert_eq!(writer.received, bytes[..written]);
    }
    assert_eq!(
        write_ready(&mut writer, &bytes, &mut written).unwrap(),
        WriteProgress::Complete
    );
    assert_eq!(writer.requests, [MAX_WRITE_BYTES, MAX_WRITE_BYTES, 123]);
    assert_eq!(writer.received, bytes);
}

#[test]
fn partial_write_slices_respect_the_remaining_byte_budget() {
    let bytes = vec![19; MAX_WRITE_BYTES + 25];
    let mut writer = ScriptedWriter::new([]);
    writer.fallback_limit = 70_000;
    let mut written = 0;
    assert_eq!(
        write_ready(&mut writer, &bytes, &mut written).unwrap(),
        WriteProgress::Pending
    );
    assert_eq!(written, MAX_WRITE_BYTES);
    assert_eq!(
        writer.requests,
        [
            MAX_WRITE_BYTES,
            MAX_WRITE_BYTES - 70_000,
            MAX_WRITE_BYTES - 140_000,
            MAX_WRITE_BYTES - 210_000
        ]
    );
    assert_eq!(
        write_ready(&mut writer, &bytes, &mut written).unwrap(),
        WriteProgress::Complete
    );
    assert_eq!(writer.received, bytes);
}

#[test]
fn completion_at_the_byte_or_attempt_limit_does_not_need_an_extra_callback() {
    let bytes = vec![23; MAX_WRITE_BYTES];
    let mut writer = ScriptedWriter::new([]);
    let mut written = 0;
    assert_eq!(
        write_ready(&mut writer, &bytes, &mut written).unwrap(),
        WriteProgress::Complete
    );
    assert_eq!(written, MAX_WRITE_BYTES);
    assert_eq!(writer.requests.len(), 1);

    let bytes = vec![31; MAX_WRITE_ATTEMPTS];
    let mut writer = ScriptedWriter::new([]);
    writer.fallback_limit = 1;
    let mut written = 0;
    assert_eq!(
        write_ready(&mut writer, &bytes, &mut written).unwrap(),
        WriteProgress::Complete
    );
    assert_eq!(written, MAX_WRITE_ATTEMPTS);
    assert_eq!(writer.requests.len(), MAX_WRITE_ATTEMPTS);
}

#[test]
fn write_zero_is_terminal_after_partial_progress() {
    let mut writer = ScriptedWriter::new([Action::Accept(2), Action::Accept(0), Action::Accept(4)]);
    let mut written = 0;
    let error = write_ready(&mut writer, b"abcdef", &mut written).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::WriteZero);
    assert_eq!(written, 2);
    assert_eq!(writer.received, b"ab");
    assert_eq!(writer.requests.len(), 2);
    assert_eq!(
        writer.actions.len(),
        1,
        "terminal errors must not be retried"
    );
}

#[test]
fn permanent_errors_are_returned_once_without_losing_the_successful_prefix() {
    for kind in [
        ErrorKind::BrokenPipe,
        ErrorKind::ConnectionReset,
        ErrorKind::PermissionDenied,
        ErrorKind::Other,
    ] {
        let mut writer =
            ScriptedWriter::new([Action::Accept(2), Action::Error(kind), Action::Accept(4)]);
        let mut written = 0;
        let error = write_ready(&mut writer, b"abcdef", &mut written).unwrap_err();
        assert_eq!(error.kind(), kind);
        assert_eq!(written, 2);
        assert_eq!(writer.received, b"ab");
        assert_eq!(writer.requests.len(), 2);
        assert_eq!(writer.actions.len(), 1);
    }
}

#[test]
fn empty_or_completed_payload_never_calls_write() {
    let mut writer = ScriptedWriter::new([Action::Accept(0)]);
    let mut written = 0;
    assert_eq!(
        write_ready(&mut writer, b"", &mut written).unwrap(),
        WriteProgress::Complete
    );
    written = 4;
    assert_eq!(
        write_ready(&mut writer, b"done", &mut written).unwrap(),
        WriteProgress::Complete
    );
    assert!(writer.requests.is_empty());
    assert_eq!(writer.actions.len(), 1);
}

#[test]
fn invalid_cursor_is_rejected_without_panicking_or_touching_the_writer() {
    let mut writer = ScriptedWriter::new([]);
    let mut written = usize::MAX;
    let error = write_ready(&mut writer, b"payload", &mut written).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
    assert_eq!(written, usize::MAX);
    assert!(writer.requests.is_empty());
}

#[test]
fn an_invalid_writer_count_cannot_overflow_or_advance_the_cursor() {
    struct InvalidWriter;
    impl Write for InvalidWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            Ok(bytes.len() + 1)
        }
        fn flush(&mut self) -> io::Result<()> {
            unreachable!()
        }
    }
    let mut written = 0;
    let error = write_ready(&mut InvalidWriter, b"payload", &mut written).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidData);
    assert_eq!(written, 0);
}
