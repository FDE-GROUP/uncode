//! OS-level sandbox for bash command execution.
//!
//! Wraps commands in `bwrap` (Linux) or `sandbox-exec` (macOS) to provide
//! filesystem and network isolation. Gracefully degrades to unsandboxed
//! execution when the sandbox tool is unavailable.

use std::path::Path;

use tokio::process::Command;
use uncode_shared::config::SandboxProfile;

/// Sandbox backend — platform-specific command wrapping.
pub trait SandboxBackend: Send + Sync {
    /// Check if the sandbox tool is available on this system.
    fn is_available(&self) -> bool;

    /// Wrap a bash command with sandbox invocation, rooted at `workdir`.
    fn wrap_command(&self, cmd: &str, workdir: &Path, profile: SandboxProfile) -> Command;
}

/// Detect and return the appropriate sandbox backend for the current platform.
pub fn detect_backend() -> Box<dyn SandboxBackend> {
    #[cfg(target_os = "linux")]
    {
        Box::new(BubblewrapBackend)
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(SandboxExecBackend)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Box::new(NoopBackend)
    }
}

// ── Linux: bubblewrap (bwrap) ──

#[cfg(target_os = "linux")]
struct BubblewrapBackend;

#[cfg(target_os = "linux")]
impl SandboxBackend for BubblewrapBackend {
    fn is_available(&self) -> bool {
        std::process::Command::new("bwrap")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn wrap_command(&self, cmd: &str, workdir: &Path, profile: SandboxProfile) -> Command {
        let mut bwrap = Command::new("bwrap");

        // Read-only system directories
        for dir in ["/usr", "/lib", "/lib64", "/bin", "/sbin", "/etc"] {
            if Path::new(dir).exists() {
                bwrap.arg("--ro-bind").arg(dir).arg(dir);
            }
        }

        // /proc and /dev needed for basic process operation
        bwrap.arg("--proc").arg("/proc");
        bwrap.arg("--dev").arg("/dev");

        // Writable workspace
        bwrap.arg("--bind").arg(workdir).arg(workdir);

        // Process lifecycle
        bwrap.arg("--die-with-parent");
        bwrap.arg("--unshare-ipc");
        bwrap.arg("--unshare-pid");
        bwrap.arg("--unshare-net");
        bwrap.arg("--unshare-uts");

        if profile == SandboxProfile::Permissive {
            bwrap.arg("--share-net");
            let tmp = Path::new("/tmp");
            if tmp.exists() {
                bwrap.arg("--bind").arg(tmp).arg(tmp);
            }
        }

        // Host resolv.conf for DNS (only in permissive mode with network)
        if profile == SandboxProfile::Permissive {
            let resolv = Path::new("/etc/resolv.conf");
            if resolv.exists() {
                bwrap.arg("--ro-bind").arg(resolv).arg("/etc/resolv.conf");
            }
        }

        bwrap
            .arg("--")
            .arg("bash")
            .arg("-c")
            .arg(cmd)
            .current_dir(workdir);

        #[cfg(unix)]
        bwrap.process_group(0);

        bwrap
    }
}

// ── macOS: sandbox-exec ──

#[cfg(target_os = "macos")]
struct SandboxExecBackend;

#[cfg(target_os = "macos")]
impl SandboxBackend for SandboxExecBackend {
    fn is_available(&self) -> bool {
        std::process::Command::new("sandbox-exec")
            .arg("-n")
            .arg("no-network")
            .arg("--")
            .arg("true")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn wrap_command(&self, cmd: &str, workdir: &Path, profile: SandboxProfile) -> Command {
        let profile_str = match profile {
            SandboxProfile::Strict => {
                "(version 1)(deny default)(allow process*)(allow file-read*)(allow file-write* (subpath \"${workdir}\"))"
            }
            SandboxProfile::Permissive => {
                "(version 1)(deny default)(allow process*)(allow file-read*)(allow file-write*)(allow network*)"
            }
        };

        let mut command = Command::new("sandbox-exec");
        command
            .arg("-p")
            .arg(profile_str)
            .arg("--")
            .arg("bash")
            .arg("-c")
            .arg(cmd)
            .current_dir(workdir);

        #[cfg(unix)]
        command.process_group(0);

        command
    }
}

// ── Fallback: no sandbox ──

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
struct NoopBackend;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
impl SandboxBackend for NoopBackend {
    fn is_available(&self) -> bool {
        false
    }

    fn wrap_command(&self, cmd: &str, workdir: &Path, _profile: SandboxProfile) -> Command {
        let mut command = Command::new("bash");
        command.arg("-c").arg(cmd).current_dir(workdir);
        #[cfg(unix)]
        command.process_group(0);
        command
    }
}

/// Resolved sandbox context for passing through the execution pipeline.
pub struct SandboxContext {
    backend: Box<dyn SandboxBackend>,
    profile: SandboxProfile,
    enabled: bool,
}

impl SandboxContext {
    /// Create from config.
    pub fn new(sandbox: bool, profile: SandboxProfile) -> Self {
        let backend = detect_backend();
        let available = backend.is_available();
        if sandbox && !available {
            tracing::warn!(
                "sandbox requested but no backend available — commands will run unsandboxed"
            );
        }
        Self {
            backend,
            profile,
            enabled: sandbox && available,
        }
    }

    /// Create a disabled sandbox context (no-op).
    pub fn disabled() -> Self {
        Self {
            backend: detect_backend(),
            profile: SandboxProfile::Strict,
            enabled: false,
        }
    }

    /// Whether sandboxing is active.
    pub fn is_active(&self) -> bool {
        self.enabled
    }

    /// Build a sandboxed or unsandboxed command.
    pub fn build_command(&self, cmd: &str, workdir: &Path) -> Command {
        if self.enabled {
            self.backend.wrap_command(cmd, workdir, self.profile)
        } else {
            let mut command = Command::new("bash");
            command.arg("-c").arg(cmd).current_dir(workdir);
            #[cfg(unix)]
            command.process_group(0);
            command
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_context_disabled() {
        let ctx = SandboxContext::disabled();
        assert!(!ctx.is_active());
    }

    #[test]
    fn test_sandbox_context_new_disabled() {
        let ctx = SandboxContext::new(false, SandboxProfile::Strict);
        assert!(!ctx.is_active());
    }

    #[test]
    fn test_sandbox_context_build_command_disabled() {
        let ctx = SandboxContext::disabled();
        let cmd = ctx.build_command("echo hello", Path::new("."));
        // Should produce a plain bash command, not bwrap
        let prog = cmd.as_std().get_program().to_string_lossy();
        assert_eq!(prog, "bash");
    }

    #[test]
    fn test_detect_backend_returns_something() {
        let backend = detect_backend();
        // Just verify it doesn't panic
        let _ = backend.is_available();
    }
}
