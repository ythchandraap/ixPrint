use super::*;
use crate::domain::incoming_job::{IncomingSource, IncomingState, PageMedia};

fn job(id: String, os_job_id: &str) -> IncomingJob {
    IncomingJob {
        id,
        source: IncomingSource::SystemPrint,
        document_name: "sample.pdf".into(),
        pages: vec![PageMedia {
            width_pt: 595.28,
            height_pt: 841.89,
        }],
        copies: 1,
        os_job_id: Some(os_job_id.into()),
        state: IncomingState::AwaitingDestination,
        received_at: chrono::Utc::now().to_rfc3339(),
        printer: None,
        spooler_job: None,
        remote_job_id: None,
        remote_name: None,
        submission_started_at: None,
        message: None,
    }
}

fn temp_dir(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("ixprint-{}-{}", label, uuid::Uuid::new_v4()))
}

#[test]
fn repeated_os_job_returns_existing_incoming_job_without_copying_again() {
    let root = temp_dir("incoming-dedupe");
    let source = root.join("source.pdf");
    fs::create_dir_all(&root).unwrap();
    fs::write(&source, b"one copy").unwrap();
    let store = IncomingStore::new(root.join("store"));
    let original = job(uuid::Uuid::new_v4().to_string(), "os-job-7");
    let (created, inserted) = store.create(original.clone(), &source).unwrap();
    assert!(inserted);

    fs::write(&source, b"different duplicate bytes").unwrap();
    let duplicate = job(uuid::Uuid::new_v4().to_string(), "os-job-7");
    let (existing, inserted) = store.create(duplicate, &source).unwrap();
    assert!(!inserted);
    assert_eq!(existing.id, original.id);
    assert_eq!(
        fs::read(store.document_path(&created.id).unwrap()).unwrap(),
        b"one copy"
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn only_one_caller_can_claim_a_pending_document_for_submission() {
    let root = temp_dir("incoming-claim");
    let source = root.join("source.pdf");
    fs::create_dir_all(&root).unwrap();
    fs::write(&source, b"pdf").unwrap();
    let store = IncomingStore::new(root.join("store"));
    let job = job(uuid::Uuid::new_v4().to_string(), "os-job-8");
    store.create(job.clone(), &source).unwrap();

    assert!(store.claim_for_submission(&job.id, "Office").unwrap());
    assert!(!store.claim_for_submission(&job.id, "Other").unwrap());
    let current = store.get(&job.id).unwrap().unwrap();
    assert_eq!(current.state, IncomingState::Submitting);
    assert_eq!(current.printer.as_deref(), Some("Office"));

    fs::remove_dir_all(root).unwrap();
}
