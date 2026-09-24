use std::path::Path;

use serde_json::Value;

use crate::application::ports::RemoteGateway;
use crate::domain::printer::{Capabilities, Printer, PrinterQueue};
use crate::domain::remote::{Remote, RemoteError, RemotePrintResult};
use crate::domain::settings::PrintSettings;

#[derive(Default)]
pub struct RemoteClient;

impl RemoteGateway for RemoteClient {
    fn fetch_printers(&self, remote: &Remote) -> Result<Vec<Printer>, RemoteError> {
        crate::remote::fetch_printers(remote)
    }

    fn capabilities(&self, remote: &Remote, name: &str) -> Result<Capabilities, RemoteError> {
        crate::remote::capabilities(remote, name)
    }

    fn queue(&self, remote: &Remote, name: &str) -> Result<PrinterQueue, RemoteError> {
        crate::remote::queue(remote, name)
    }

    fn job_status(&self, remote: &Remote, job_id: &str) -> Result<Value, RemoteError> {
        crate::remote::job_status(remote, job_id)
    }

    fn print_pdf(
        &self,
        remote: &Remote,
        path: &Path,
        file_name: &str,
        settings: &PrintSettings,
    ) -> Result<RemotePrintResult, RemoteError> {
        crate::remote::print_pdf(remote, path, file_name, settings)
    }

    fn test_page(
        &self,
        remote: &Remote,
        pages: u32,
        settings: &PrintSettings,
    ) -> Result<RemotePrintResult, RemoteError> {
        crate::remote::test_page(remote, pages, settings)
    }
}
