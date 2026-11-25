use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use uuid::Uuid;

/// Environment variables that MUST NEVER be silently inherited into subscription-based
/// external AI harness child processes (prevents accidental pay-as-you-go billing or secret leaks).
const STRIPPED_ENV_KEYS: &[&str] = &[
    "OPENAI_API_KEY",
    "OPENAI_ORG_ID",
    "ANTHROPIC_API_KEY",
    "GEMINI_API_KEY",
    "GOOGLE_API_KEY",
    "GOOGLE_OAUTH_CLIENT_ID",
    "GOOGLE_OAUTH_CLIENT_SECRET",
    "AWS_ACCESS_KEY_ID",
    "AWS_SECRET_ACCESS_KEY",
    "GITHUB_TOKEN",
    "GH_TOKEN",
];

#[derive(Debug)]
#[allow(dead_code)]
pub struct IsolatedWorkspace {
    pub session_id: String,
    pub root: PathBuf,
    pub input_dir: PathBuf,
    pub context_dir: PathBuf,
    pub output_dir: PathBuf,
    pub artifacts_dir: PathBuf,
}

impl IsolatedWorkspace {
    pub fn create(base_tmp: &Path) -> std::io::Result<Self> {
        let session_id = Uuid::new_v4().to_string();
        let root = base_tmp.join(&session_id);
        let input_dir = root.join("input");
        let context_dir = root.join("context");
        let output_dir = root.join("output");
        let artifacts_dir = root.join("artifacts");

        fs::create_dir_all(&input_dir)?;
        fs::create_dir_all(&context_dir)?;
        fs::create_dir_all(&output_dir)?;
        fs::create_dir_all(&artifacts_dir)?;

        Ok(Self {
            session_id,
            root,
            input_dir,
            context_dir,
            output_dir,
            artifacts_dir,
        })
    }

    pub fn cleanup(&self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Builds a sanitized environment map preserving OS essentials and official CLI home paths
/// while stripping API keys and cloud secrets.
pub fn sanitized_env() -> BTreeMap<String, String> {
    let mut env_map = BTreeMap::new();
    for (k, v) in std::env::vars() {
        let upper = k.to_ascii_uppercase();
        if STRIPPED_ENV_KEYS
            .iter()
            .any(|banned| banned.eq_ignore_ascii_case(&upper))
        {
            continue;
        }
        env_map.insert(k, v);
    }
    env_map
}

pub fn apply_sanitized_env(cmd: &mut Command) {
    cmd.env_clear();
    for (k, v) in sanitized_env() {
        cmd.env(k, v);
    }
}

/// Windows Job Object RAII guard configured with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
/// Ensures child processes and all their descendants terminate cleanly when dropped or
/// if the host process exits unexpectedly (orphan-process prevention).
#[cfg(windows)]
pub struct ProcessJobGuard {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
// SAFETY: A Win32 Job Object HANDLE is thread-safe to close/assign across threads.
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

    #[test]
    fn test_sanitized_env_strips_api_keys() {
        let env = sanitized_env();
        assert!(!env.keys().any(|k| k.eq_ignore_ascii_case("OPENAI_API_KEY")));
        assert!(
            !env.keys()
                .any(|k| k.eq_ignore_ascii_case("ANTHROPIC_API_KEY"))
        );
        assert!(!env.keys().any(|k| k.eq_ignore_ascii_case("GEMINI_API_KEY")));
    }

    #[test]
    fn test_workspace_isolation_dirs() {
        let base = std::env::temp_dir().join("formuladesk-ws-test");
        let ws = IsolatedWorkspace::create(&base).expect("create workspace");
        assert!(ws.input_dir.exists());
        assert!(ws.context_dir.exists());
        assert!(ws.output_dir.exists());
        assert!(ws.artifacts_dir.exists());
        ws.cleanup();
        assert!(!ws.root.exists());
    }
}
