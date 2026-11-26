use std::fs::File;
use std::io::{self, BufReader, BufWriter};
use std::path::Path;
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::process::{Child, Command};

use super::{
    CompilePdfMetadata, CompiledSolutionPdf, PdfArtifactInput, PdfCompileFailure, compile_sync,
};
use crate::domain::PdfSettings;

const WORKER_FLAG: &str = "--theorem-pdf-worker";
const REQUEST: &str = "request.json";
const RESPONSE: &str = "response.json";

#[derive(Serialize)]
pub(super) struct PdfRequest<'a> {
    pub solution: &'a Value,
    pub settings: &'a PdfSettings,
    pub document_title: &'a str,
    pub problem_label: Option<&'a str>,
    pub problem_statement: Option<&'a str>,
    pub version_number: i64,
    pub model_name: Option<&'a str>,
    pub generated_at: &'a str,
    pub artifacts: Vec<PdfArtifactInput>,
}

// JSON cannot deserialize borrowed Value or PdfSettings. The input is owned in
// the child, while the parent can serialize directly from the validated inputs.
#[derive(Deserialize)]
struct WorkerInput {
    solution: Value,
    settings: PdfSettings,
    document_title: String,
    problem_label: Option<String>,
    problem_statement: Option<String>,
    version_number: i64,
    model_name: Option<String>,
    generated_at: String,
    artifacts: Vec<PdfArtifactInput>,
}

type WorkerResponse = Result<CompiledSolutionPdf, PdfCompileFailure>;

pub fn run_pdf_worker_if_requested() -> Option<bool> {
    let mut args = std::env::args_os().skip(1);
    if args.next()?.to_str() != Some(WORKER_FLAG) {
        return None;
    }
    Some(
        args.next()
            .is_some_and(|dir| args.next().is_none() && run_worker(Path::new(&dir)).is_ok()),
    )
}

fn run_worker(dir: &Path) -> io::Result<()> {
    let request: WorkerInput =
        serde_json::from_reader(BufReader::new(File::open(dir.join(REQUEST))?))?;
    let result = compile_sync(
        &request.solution,
        &request.settings,
        &CompilePdfMetadata {
            document_title: &request.document_title,
            problem_label: request.problem_label.as_deref(),
            problem_statement: request.problem_statement.as_deref(),
            version_number: request.version_number,
            model_name: request.model_name.as_deref(),
            generated_at: &request.generated_at,
        },
        request.artifacts,
    );
    let mut writer = BufWriter::new(File::create(dir.join(RESPONSE))?);
    serde_json::to_writer(&mut writer, &result)?;
    io::Write::flush(&mut writer)
}

fn command_for(dir: &Path) -> io::Result<Command> {
    let mut command = Command::new(std::env::current_exe()?);
    command
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    #[cfg(not(test))]
    command.arg(WORKER_FLAG).arg(dir);
    #[cfg(test)]
    command
        .args([
            "--ignored",
            "--exact",
            "pdf::compiler::worker::tests::worker_entry",
        ])
        .env("THEOREM_TEST_PDF_WORKER_DIR", dir);
    Ok(command)
}

async fn finish(child: &mut Child, timeout: Duration) -> WorkerResponseStatus {
    match tokio::time::timeout(timeout, child.wait()).await {
        Ok(result) => result.map_err(worker_error),
        Err(_) => {
            // Tokio kill waits for process termination, including on Windows.
            child.kill().await.map_err(worker_error)?;
            Err(PdfCompileFailure::Compile(format!(
                "PDF compile timed out after {} ms",
                timeout.as_millis()
            )))
        }
    }
}

type WorkerResponseStatus = Result<ExitStatus, PdfCompileFailure>;

pub(super) async fn compile(request: PdfRequest<'_>, timeout: Duration) -> WorkerResponse {
    let dir = tempfile::tempdir().map_err(worker_error)?;
    let bytes = serde_json::to_vec(&request).map_err(worker_error)?;
    tokio::fs::write(dir.path().join(REQUEST), bytes)
        .await
        .map_err(worker_error)?;
    let mut command = command_for(dir.path()).map_err(worker_error)?;
    #[cfg(windows)]
    let job = crate::agents::sandbox::ProcessJobGuard::new().map_err(worker_error)?;
    let mut child = command.spawn().map_err(worker_error)?;
    #[cfg(windows)]
    if let Some(pid) = child.id()
        && let Err(error) = job.assign_pid(pid)
    {
        child.kill().await.map_err(worker_error)?;
        return Err(worker_error(error));
    }
    let status = finish(&mut child, timeout).await?;
    if !status.success() {
        return Err(worker_error(status));
    }
    let response = tokio::fs::read(dir.path().join(RESPONSE))
        .await
        .map_err(worker_error)?;
    serde_json::from_slice::<WorkerResponse>(&response).map_err(worker_error)?
}

fn worker_error(error: impl std::fmt::Display) -> PdfCompileFailure {
    PdfCompileFailure::Compile(format!("PDF worker: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    #[ignore = "child process entry point"]
    fn worker_entry() {
        let Some(dir) = std::env::var_os("THEOREM_TEST_PDF_WORKER_DIR") else {
            return;
        };
        let dir = Path::new(&dir);
        if std::env::var_os("THEOREM_TEST_PDF_WORKER_DELAY").is_some() {
            std::fs::write(dir.join("started"), b"").expect("signal worker started");
            std::thread::sleep(Duration::from_secs(60));
        }
        run_worker(dir).expect("PDF worker");
    }

    #[tokio::test]
    async fn timeout_kills_and_reaps_worker_before_next_compile() {
        let dir = tempfile::tempdir().expect("worker directory");
        let mut command = command_for(dir.path()).expect("command");
        let mut child = command
            .env("THEOREM_TEST_PDF_WORKER_DELAY", "1")
            .spawn()
            .expect("start worker");
        #[cfg(windows)]
        let _job = {
            let job = crate::agents::sandbox::ProcessJobGuard::new().expect("job");
            job.assign_pid(child.id().expect("worker PID"))
                .expect("attach job");
            job
        };
        tokio::time::timeout(Duration::from_secs(10), async {
            while !dir.path().join("started").exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("worker entered its delay");
        let err = finish(&mut child, Duration::from_millis(25))
            .await
            .expect_err("timeout");
        assert!(
            matches!(err, PdfCompileFailure::Compile(message) if message.contains("timed out"))
        );
        assert!(
            !child
                .try_wait()
                .expect("worker exit status")
                .expect("worker reaped")
                .success()
        );
        assert!(!dir.path().join(RESPONSE).exists());

        let result = compile(
            PdfRequest {
                solution: &json!({"title": "Math", "steps": [], "answer": {"latex": "x = 1"}}),
                settings: &PdfSettings::default(),
                document_title: "Math",
                problem_label: None,
                problem_statement: None,
                version_number: 1,
                model_name: None,
                generated_at: "2026-11-26T00:00:00Z",
                artifacts: Vec::new(),
            },
            Duration::from_secs(20),
        )
        .await
        .expect("next PDF");
        assert!(result.pdf_bytes.starts_with(b"%PDF-"));
    }
}
