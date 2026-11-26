use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::process::Command;

/// Environment variables that MUST NEVER be silently inherited into external AI harness
/// child processes (prevents accidental pay-as-you-go billing and cloud secret leakage).
const STRIPPED_ENV_KEYS: &[&str] = &[
    "OPENAI_API_KEY",
    "OPENAI_ORG_ID",
    "OPENAI_BASE_URL",
    "ANTHROPIC_API_KEY",
    "GEMINI_API_KEY",
    "GOOGLE_API_KEY",
    "GOOGLE_OAUTH_CLIENT_ID",
    "GOOGLE_OAUTH_CLIENT_SECRET",
    "AWS_ACCESS_KEY_ID",
    "AWS_SECRET_ACCESS_KEY",
    "AWS_SESSION_TOKEN",
    "GITHUB_TOKEN",
    "GH_TOKEN",
];

/// Session-scoped isolated workspace (`workspaces/<session-uuid>/{input,context,output,artifacts}`).
#[derive(Debug, Clone)]
pub struct IsolatedWorkspace {
    pub session_id: String,
    pub relative_path: String,
    pub root: PathBuf,
    pub input_dir: PathBuf,
    pub context_dir: PathBuf,
    pub output_dir: PathBuf,
    pub artifacts_dir: PathBuf,
}

impl IsolatedWorkspace {
    pub fn ensure(workspaces_root: &Path, session_id: &str) -> std::io::Result<Self> {
        let root = workspaces_root.join(session_id);
        let input_dir = root.join("input");
        let context_dir = root.join("context");
        let output_dir = root.join("output");
        let artifacts_dir = root.join("artifacts");

        fs::create_dir_all(&input_dir)?;
        fs::create_dir_all(&context_dir)?;
        fs::create_dir_all(&output_dir)?;
        fs::create_dir_all(&artifacts_dir)?;

        Ok(Self {
            session_id: session_id.to_string(),
            relative_path: format!("workspaces/{session_id}"),
            root,
            input_dir,
            context_dir,
            output_dir,
            artifacts_dir,
        })
    }

    pub fn write_input_file(&self, filename: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
        let safe_name: String = filename
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let target = self.input_dir.join(if safe_name.is_empty() {
            "input.bin".to_string()
        } else {
            safe_name
        });
        fs::write(&target, bytes)?;
        Ok(target)
    }

    pub fn cleanup(&self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Builds a sanitized environment map preserving OS essentials and official CLI home paths
/// while strictly stripping API keys and cloud secrets.
pub fn sanitized_env() -> BTreeMap<String, String> {
    let mut env_map = BTreeMap::new();
    for (k, v) in std::env::vars() {
        if STRIPPED_ENV_KEYS
            .iter()
            .any(|banned| banned.eq_ignore_ascii_case(&k))
        {
            continue;
        }
        env_map.insert(k, v);
    }
    env_map
}

/// Applies sanitized env to a child process, optionally adding explicit opt-in key pairs
/// (ONLY used when user explicitly opted into `api_key_keyring`).
pub fn apply_sanitized_env(cmd: &mut Command, explicit_opt_in: &[(&str, &str)]) {
    cmd.env_clear();
    for (k, v) in sanitized_env() {
        cmd.env(k, v);
    }
    for (k, v) in explicit_opt_in {
        cmd.env(k, v);
    }
}

/// Resolves an external CLI binary via explicit manual path, system PATH, or well-known Windows install directories.
pub fn resolve_cli_binary(manual_path: Option<&str>, binary_name: &str) -> Option<PathBuf> {
    if let Some(manual) = manual_path {
        let trimmed = manual.trim();
        if !trimmed.is_empty() {
            let p = PathBuf::from(trimmed);
            if p.is_file() {
                return Some(p);
            }
        }
    }

    let mut search_dirs: Vec<PathBuf> = Vec::new();
    if let Some(path_var) = std::env::var_os("PATH") {
        search_dirs.extend(std::env::split_paths(&path_var));
    }

    #[cfg(windows)]
    {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            let base = PathBuf::from(local);
            search_dirs.push(
                base.join("Programs")
                    .join("OpenAI")
                    .join("Codex")
                    .join("bin"),
            );
            search_dirs.push(base.join("agy").join("bin"));
            search_dirs.push(base.join("Microsoft").join("WinGet").join("Links"));
        }
        if let Some(roaming) = std::env::var_os("APPDATA") {
            search_dirs.push(PathBuf::from(roaming).join("npm"));
        }
        if let Some(profile) = std::env::var_os("USERPROFILE") {
            let home = PathBuf::from(profile);
            search_dirs.push(home.join(".cargo").join("bin"));
            search_dirs.push(home.join(".local").join("bin"));
        }
    }

    #[cfg(not(windows))]
    {
        if let Some(home) = std::env::var_os("HOME") {
            let h = PathBuf::from(home);
            search_dirs.push(h.join(".local").join("bin"));
            search_dirs.push(h.join(".cargo").join("bin"));
        }
        search_dirs.push(PathBuf::from("/usr/local/bin"));
    }

    #[cfg(windows)]
    let candidates = [
        format!("{binary_name}.exe"),
        format!("{binary_name}.cmd"),
        format!("{binary_name}.bat"),
        binary_name.to_string(),
    ];
    #[cfg(not(windows))]
    let candidates = [binary_name.to_string()];

    for dir in search_dirs {
        for cand in &candidates {
            let full = dir.join(cand);
            if full.is_file() {
                return Some(full);
            }
        }
    }
    None
}

/// Probes `<binary> --version` with sanitized env and a strict timeout.
pub async fn probe_cli_version(binary_path: &Path) -> Option<String> {
    let mut cmd = Command::new(binary_path);
    cmd.arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    apply_sanitized_env(&mut cmd, &[]);

    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let output = tokio::time::timeout(Duration::from_secs(5), cmd.output())
        .await
        .ok()?
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let line = if !stdout.is_empty() {
        stdout.lines().next().unwrap_or_default().trim().to_string()
    } else {
        stderr.lines().next().unwrap_or_default().trim().to_string()
    };
    if line.is_empty() { None } else { Some(line) }
}

/// Windows Job Object RAII guard configured with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
/// Guarantees child processes and all descendants terminate cleanly when dropped or on host crash.
#[cfg(windows)]
pub struct ProcessJobGuard {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
// SAFETY: Win32 Job Object HANDLE is safe to close/assign across threads.
unsafe impl Send for ProcessJobGuard {}
#[cfg(windows)]
unsafe impl Sync for ProcessJobGuard {}

#[cfg(windows)]
impl ProcessJobGuard {
    pub fn new() -> std::io::Result<Self> {
        use std::mem::{size_of, zeroed};
        use windows_sys::Win32::System::JobObjects::{
            CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        };

        // SAFETY: Passing null security attributes and null name creates an anonymous Job Object.
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error());
        }

        // SAFETY: Zero-initialized POD Win32 struct, setting only BasicLimitInformation.LimitFlags.
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

        // SAFETY: `info` matches `JobObjectExtendedLimitInformation` size and layout.
        let ok = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&raw const info).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if ok == 0 {
            let err = std::io::Error::last_os_error();
            // SAFETY: Valid handle created above.
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(handle);
            }
            return Err(err);
        }

        Ok(Self { handle })
    }

    pub fn assign_pid(&self, pid: u32) -> std::io::Result<()> {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
        };

        // SAFETY: Standard Win32 handle acquisition for child PID with minimal rights.
        let proc_handle = unsafe { OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid) };
        if proc_handle.is_null() {
            return Err(std::io::Error::last_os_error());
        }

        // SAFETY: Both handles are valid Win32 handles.
        let ok = unsafe { AssignProcessToJobObject(self.handle, proc_handle) };
        let err = if ok == 0 {
            Some(std::io::Error::last_os_error())
        } else {
            None
        };
        // SAFETY: Closing process handle after assignment.
        unsafe {
            CloseHandle(proc_handle);
        }
        if let Some(e) = err {
            return Err(e);
        }
        Ok(())
    }
}

#[cfg(windows)]
impl Drop for ProcessJobGuard {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: Valid Job Object handle owned solely by this struct.
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(self.handle);
            }
        }
    }
}

#[cfg(not(windows))]
pub struct ProcessJobGuard;

#[cfg(not(windows))]
impl ProcessJobGuard {
    pub fn new() -> std::io::Result<Self> {
        Ok(Self)
    }

    pub fn assign_pid(&self, _pid: u32) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_sanitized_env_strips_sensitive_keys() {
        let env = sanitized_env();
        for key in [
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
            "GEMINI_API_KEY",
            "GOOGLE_OAUTH_CLIENT_SECRET",
            "AWS_SECRET_ACCESS_KEY",
            "GITHUB_TOKEN",
        ] {
            assert!(!env.keys().any(|k| k.eq_ignore_ascii_case(key)));
        }
    }

    #[test]
    fn test_isolated_workspace_dirs_and_safe_input() {
        let tmp = TempDir::new().expect("tempdir");
        let ws = IsolatedWorkspace::ensure(tmp.path(), "sess-123").expect("ws");
        assert!(ws.input_dir.exists());
        assert!(ws.context_dir.exists());
        assert!(ws.output_dir.exists());
        assert!(ws.artifacts_dir.exists());

        let written = ws.write_input_file("../evil.png", b"png").expect("write");
        assert!(written.starts_with(&ws.input_dir));
        ws.cleanup();
        assert!(!ws.root.exists());
    }
}
