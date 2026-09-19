//! Exercise the real SFTP reader over an in-memory SFTP server, including EOF
//! packets and resume offsets. No SSH account or external server is needed.

use super::*;
use russh_sftp::protocol::{Attrs, Data, FileAttributes, Handle, Status, StatusCode};
use std::sync::atomic::{AtomicU64, Ordering};

struct DownloadServer {
    size: Option<u64>,
    stat_fails: bool,
    available: Arc<AtomicU64>,
    reads: Arc<Mutex<Vec<u64>>>,
}

impl russh_sftp::server::Handler for DownloadServer {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    async fn stat(&mut self, id: u32, _path: String) -> std::result::Result<Attrs, Self::Error> {
        if self.stat_fails {
            return Err(StatusCode::PermissionDenied);
        }
        Ok(Attrs {
            id,
            attrs: FileAttributes {
                size: self.size,
                ..Default::default()
            },
        })
    }

    async fn open(
        &mut self,
        id: u32,
        _path: String,
        _flags: OpenFlags,
        _attrs: FileAttributes,
    ) -> std::result::Result<Handle, Self::Error> {
        Ok(Handle {
            id,
            handle: "payload".into(),
        })
    }

    async fn read(
        &mut self,
        id: u32,
        _handle: String,
        offset: u64,
        len: u32,
    ) -> std::result::Result<Data, Self::Error> {
        self.reads.lock().unwrap().push(offset);
        let available = self.available.load(Ordering::SeqCst);
        if offset >= available {
            return Err(StatusCode::Eof);
        }
        // Short DATA packets are legal; only EOF should end the download.
        let count = (available - offset).min(u64::from(len)).min(8192);
        Ok(Data {
            id,
            data: (offset..offset + count).map(|i| (i % 251) as u8).collect(),
        })
    }

    async fn close(
        &mut self,
        id: u32,
        _handle: String,
    ) -> std::result::Result<Status, Self::Error> {
        Ok(Status {
            id,
            status_code: StatusCode::Ok,
            error_message: String::new(),
            language_tag: String::new(),
        })
    }
}

fn client(
    size: Option<u64>,
    available: u64,
    stat_fails: bool,
) -> (SftpClient, Arc<AtomicU64>, Arc<Mutex<Vec<u64>>>) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let available = Arc::new(AtomicU64::new(available));
    let reads = Arc::new(Mutex::new(Vec::new()));
    let handler = DownloadServer {
        size,
        stat_fails,
        available: Arc::clone(&available),
        reads: Arc::clone(&reads),
    };
    let session = runtime.block_on(async {
        let (client, server) = tokio::io::duplex(64 * 1024);
        tokio::spawn(russh_sftp::server::run(server, handler));
        tokio::time::timeout(Duration::from_secs(5), SftpSession::new(client))
            .await
            .unwrap()
            .unwrap()
    });
    let mut client = SftpClient::new(ConnectionInfo::default(), None);
    client.runtime = Some(runtime);
    client.session = Some(Arc::new(session));
    client.connected = true;
    (client, available, reads)
}

#[test]
fn premature_eof_is_a_verification_error_and_preserves_progress() {
    let (mut client, _, _) = client(Some(90_000), 45_000, false);
    let mut output = Vec::new();
    let mut last_progress = (0, 0);
    let error = client
        .download(
            "/payload",
            &mut output,
            Some(&mut |sent, total| {
                last_progress = (sent, total);
                Ok(())
            }),
            0,
        )
        .unwrap_err();
    assert!(matches!(error, ProtocolError::Verification(_)));
    assert!(error.to_string().contains("90000"));
    assert!(error.to_string().contains("45000"));
    assert_eq!(output.len(), 45_000);
    assert_eq!(last_progress, (45_000, 90_000));
}

#[test]
fn exact_empty_and_resumed_downloads_succeed_with_short_data_packets() {
    for (size, offset) in [(90_000, 0), (90_000, 45_000), (90_000, 90_000), (0, 0)] {
        let (mut client, _, _) = client(Some(size), size, false);
        let mut output = Vec::new();
        client
            .download("/payload", &mut output, None, offset)
            .unwrap();
        assert_eq!(
            output,
            (offset..size).map(|i| (i % 251) as u8).collect::<Vec<_>>()
        );
    }
}

#[test]
fn a_short_resumed_download_still_fails() {
    let (mut client, _, _) = client(Some(90_000), 60_000, false);
    let mut output = Vec::new();
    assert!(matches!(
        client.download("/payload", &mut output, None, 45_000),
        Err(ProtocolError::Verification(_))
    ));
    assert_eq!(output.len(), 15_000);
}

#[test]
fn extra_bytes_and_offsets_past_the_known_end_are_rejected() {
    for (size, available, offset) in [(0, 10, 0), (10, 20, 0), (10, 10, 11)] {
        let (mut client, _, _) = client(Some(size), available, false);
        assert!(matches!(
            client.download("/payload", &mut Vec::new(), None, offset),
            Err(ProtocolError::Verification(_))
        ));
    }
}

#[test]
fn unknown_size_is_not_mistaken_for_an_empty_file() {
    for stat_fails in [false, true] {
        let (mut client, _, _) = client(None, 12_345, stat_fails);
        let mut output = Vec::new();
        client.download("/payload", &mut output, None, 0).unwrap();
        assert_eq!(output.len(), 12_345);
    }
}

#[test]
fn cancellation_remains_cancellation() {
    let (mut client, _, _) = client(Some(90_000), 90_000, false);
    assert!(matches!(
        client.download(
            "/payload",
            &mut Vec::new(),
            Some(&mut |_, _| Err(ProtocolError::Cancelled)),
            0
        ),
        Err(ProtocolError::Cancelled)
    ));
}

#[test]
fn failed_download_resumes_in_the_same_queue_row_and_preserves_contents() {
    use crate::transfer::{SharedClient, Status as JobStatus, TransferService};
    let (client, available, reads) = client(Some(90_000), 45_000, false);
    let client: SharedClient = Arc::new(Mutex::new(Box::new(client)));
    let service = TransferService::new(1);
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("payload");
    let id = service.submit_download(
        Arc::clone(&client),
        "/payload",
        destination.to_str().unwrap(),
        90_000,
        false,
        false,
    );
    let wait = |expected| {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let job = service.job(&id).unwrap();
            if job.status.is_finished() {
                assert_eq!(job.status, expected, "{:?}", job.error);
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "transfer did not finish"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    wait(JobStatus::Failed);
    assert_eq!(service.job(&id).unwrap().transferred_bytes, 45_000);
    assert_eq!(std::fs::metadata(&destination).unwrap().len(), 45_000);
    reads.lock().unwrap().clear();
    available.store(90_000, Ordering::SeqCst);
    assert!(service.retry(&id, Arc::clone(&client)));
    wait(JobStatus::Complete);
    assert_eq!(reads.lock().unwrap()[0], 45_000);
    assert_eq!(service.jobs().len(), 1);
    assert_eq!(service.job(&id).unwrap().progress, 100);
    assert_eq!(
        std::fs::read(destination).unwrap(),
        (0..90_000).map(|i| (i % 251) as u8).collect::<Vec<_>>()
    );
}
