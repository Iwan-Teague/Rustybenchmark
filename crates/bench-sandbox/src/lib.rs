//! `bench-sandbox` — containment for the one thing that runs untrusted code.
//!
//! The oracle executes model-authored Rust during grading (compile — proc
//! macros run at build time — and the test binaries). Everything the oracle
//! shells out runs through [`run`], so this crate is the single place that
//! containment is applied (docs/08-run-protocol.md, docs/13-architecture.md).
//! Generation is *not* here: the model call is an HTTP request from the harness
//! process and executes nothing.
//!
//! **macOS (implemented):** a seatbelt profile via `sandbox-exec` that denies
//! all network (the documented Terminal-Bench "fetched solutions online" threat)
//! and confines filesystem writes to the workspace, the cargo caches, and the
//! temp dirs cargo needs — so model code cannot write or delete outside the
//! grading workspace. `sandbox-exec` is deprecated-but-functional and is what
//! Chromium, Claude Code and others use for the same job.
//!
//! **Linux / Windows (not yet):** [`available`] reports `Unsupported` and [`run`]
//! executes without containment. The netns (Linux) and job-object (Windows)
//! paths are P1 follow-ups; until then the caller must treat a non-macOS host as
//! unsandboxed and record it. This crate never *pretends* to contain.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// What containment the current platform can actually apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Containment {
    /// macOS seatbelt: network denied, writes confined to the workspace.
    Seatbelt,
    /// No containment available on this platform yet. The caller must decide
    /// whether to proceed, and must record that grading was unsandboxed.
    Unsupported,
}

/// The containment this platform can apply. Pure — safe to call for reporting.
pub fn available() -> Containment {
    if cfg!(target_os = "macos") {
        Containment::Seatbelt
    } else {
        Containment::Unsupported
    }
}

/// A containment policy bound to one grading workspace.
pub struct Policy {
    /// The workspace root — the only place the sandboxed command may write
    /// (besides the cargo caches and temp).
    pub workspace: PathBuf,
    /// Cargo's home, allowlisted for writes so `--offline` builds can take the
    /// `.package-cache` lock and reuse the registry.
    pub cargo_home: PathBuf,
}

impl Policy {
    /// Build a policy for `workspace`. Paths are canonicalised because seatbelt
    /// matches on the real path, not symlinks.
    pub fn for_workspace(workspace: &Path) -> io::Result<Policy> {
        let workspace = workspace.canonicalize()?;
        let cargo_home = std::env::var_os("CARGO_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cargo")))
            .unwrap_or_else(|| PathBuf::from("/nonexistent"));
        let cargo_home = cargo_home.canonicalize().unwrap_or(cargo_home);
        Ok(Policy {
            workspace,
            cargo_home,
        })
    }
}

/// The result of a contained command.
pub struct Outcome {
    pub status: std::process::ExitStatus,
    pub stdout: String,
    pub stderr: String,
    /// What containment was actually applied to this run.
    pub containment: Containment,
}

impl Outcome {
    pub fn success(&self) -> bool {
        self.status.success()
    }
}

/// Run `program args…` with the current directory set to the policy's
/// workspace, under whatever containment the platform provides. `envs` are set
/// on the child (and propagate through `sandbox-exec` to the real program).
pub fn run(
    policy: &Policy,
    program: &str,
    args: &[&str],
    envs: &[(&str, &str)],
) -> io::Result<Outcome> {
    #[cfg(target_os = "macos")]
    {
        run_seatbelt(policy, program, args, envs)
    }
    #[cfg(not(target_os = "macos"))]
    {
        run_unconfined(policy, program, args, envs)
    }
}

#[cfg(target_os = "macos")]
fn run_seatbelt(
    policy: &Policy,
    program: &str,
    args: &[&str],
    envs: &[(&str, &str)],
) -> io::Result<Outcome> {
    let profile = seatbelt_profile(policy).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "workspace or cargo-home path contains a double quote; refusing to build a seatbelt profile",
        )
    })?;

    let mut cmd = Command::new("sandbox-exec");
    cmd.arg("-p").arg(&profile).arg(program).args(args);
    cmd.current_dir(&policy.workspace);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.output()?;
    Ok(Outcome {
        status: out.status,
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        containment: Containment::Seatbelt,
    })
}

/// The seatbelt profile (SBPL): permit by default, then subtract the two things
/// that matter — network, and writes outside the confinement set. Reads are
/// left broad because the toolchain needs them; a build reads far more of the
/// filesystem than it is worth enumerating, and reads are not the threat.
#[cfg(target_os = "macos")]
fn seatbelt_profile(policy: &Policy) -> Option<String> {
    let ws = policy.workspace.to_str()?;
    let ch = policy.cargo_home.to_str()?;
    if ws.contains('"') || ch.contains('"') {
        return None;
    }
    Some(format!(
        "(version 1)\n\
         (allow default)\n\
         (deny network*)\n\
         (deny file-write*)\n\
         (allow file-write* (subpath \"{ws}\"))\n\
         (allow file-write* (subpath \"{ch}\"))\n\
         (allow file-write* (subpath \"/private/tmp\"))\n\
         (allow file-write* (subpath \"/private/var/folders\"))\n\
         (allow file-write-data (literal \"/dev/null\") (literal \"/dev/dtracehelper\") (literal \"/dev/urandom\"))\n"
    ))
}

#[cfg(not(target_os = "macos"))]
fn run_unconfined(
    policy: &Policy,
    program: &str,
    args: &[&str],
    envs: &[(&str, &str)],
) -> io::Result<Outcome> {
    let mut cmd = Command::new(program);
    cmd.args(args).current_dir(&policy.workspace);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.output()?;
    Ok(Outcome {
        status: out.status,
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        containment: Containment::Unsupported,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn available_matches_platform() {
        let c = available();
        if cfg!(target_os = "macos") {
            assert_eq!(c, Containment::Seatbelt);
        } else {
            assert_eq!(c, Containment::Unsupported);
        }
    }

    // The escape tests: they prove the sandbox actually contains. macOS only,
    // because that is the only platform with an implementation to test.
    #[cfg(target_os = "macos")]
    fn tmp_policy() -> Policy {
        let dir = std::env::temp_dir().join(format!("rb-sbx-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Policy::for_workspace(&dir).unwrap()
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn network_is_denied() {
        let policy = tmp_policy();
        // python3 is present on macOS. A sandboxed connect must fail.
        let out = run(
            &policy,
            "python3",
            &[
                "-c",
                "import socket; socket.create_connection(('1.1.1.1',80),2)",
            ],
            &[],
        )
        .unwrap();
        assert!(
            !out.success(),
            "sandboxed network connect should fail, but it succeeded: {}",
            out.stderr
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn write_outside_workspace_is_denied() {
        let policy = tmp_policy();
        // Try to write to $HOME from inside the sandbox: must be denied.
        let out = run(
            &policy,
            "python3",
            &[
                "-c",
                "import os; open(os.path.expanduser('~/rb-escape-probe.txt'),'w').write('x')",
            ],
            &[],
        )
        .unwrap();
        assert!(
            !out.success(),
            "sandboxed write to $HOME should fail, but it succeeded: {}",
            out.stderr
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn write_inside_workspace_is_allowed() {
        let policy = tmp_policy();
        // A write into the workspace itself must still work.
        let out = run(
            &policy,
            "python3",
            &["-c", "open('rb-inside.txt','w').write('ok')"],
            &[],
        )
        .unwrap();
        assert!(
            out.success(),
            "workspace write should succeed: {}",
            out.stderr
        );
        assert!(policy.workspace.join("rb-inside.txt").exists());
    }
}
