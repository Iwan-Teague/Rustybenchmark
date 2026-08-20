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
use std::time::{Duration, Instant};

/// Resource limits applied to a contained command. The wall-clock timeout is
/// the load-bearing one — it stops infinite loops, hangs, and fork bombs that
/// never terminate. The rest are backstops.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Harness-owned wall-clock deadline. On expiry the whole process group is
    /// killed (docs/08: timeouts are harness-owned, never submitter-set).
    pub wall: Duration,
    /// `RLIMIT_CPU` seconds — a *far* backstop, only for the case where the
    /// harness's own wait loop fails. It must stay well above the wall clock:
    /// a multi-threaded test binary accumulates CPU time N× faster than wall
    /// time, so tying CPU to the wall makes the CPU limit pre-empt the wall and
    /// mis-record the kill. The wall clock is always the primary control.
    pub cpu: Duration,
    /// `RLIMIT_AS` bytes — address-space cap. `None` by default: macOS enforces
    /// `RLIMIT_AS` unreliably, so it is opt-in until verified per platform.
    pub address_space: Option<u64>,
}

impl Default for Limits {
    fn default() -> Self {
        // Generous enough that a real frozen-task build/test never trips it;
        // tight enough to catch a runaway. Suites tune these.
        Limits {
            wall: Duration::from_secs(120),
            cpu: Duration::from_secs(3600),
            address_space: None,
        }
    }
}

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
    /// Resource limits applied to every command run under this policy.
    pub limits: Limits,
}

impl Policy {
    /// Build a policy for `workspace` with default limits. Paths are
    /// canonicalised because seatbelt matches on the real path, not symlinks.
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
            limits: Limits::default(),
        })
    }

    /// Set the resource limits (builder style).
    pub fn with_limits(mut self, limits: Limits) -> Policy {
        self.limits = limits;
        self
    }
}

/// The result of a contained command.
pub struct Outcome {
    pub status: std::process::ExitStatus,
    pub stdout: String,
    pub stderr: String,
    /// The wall-clock deadline fired and the process group was killed.
    pub timed_out: bool,
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
    finish(cmd, &policy.limits, Containment::Seatbelt)
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
    finish(cmd, &policy.limits, Containment::Unsupported)
}

/// Run `cmd` to completion under `limits`: apply rlimits and a private process
/// group in the child, capture stdio to temp files (so a full pipe cannot
/// deadlock the manual wait), and enforce the wall-clock deadline by killing the
/// whole process group.
fn finish(mut cmd: Command, limits: &Limits, containment: Containment) -> io::Result<Outcome> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        // The child leads its own process group, so the deadline can kill the
        // entire tree (cargo → rustc → the test binary), not just the direct
        // child.
        cmd.process_group(0);

        // rlimits are set post-fork, pre-exec, and inherit to every descendant.
        let cpu = limits.cpu.as_secs();
        let addr = limits.address_space;
        // SAFETY: setrlimit is async-signal-safe; the closure touches no heap
        // and no shared state, which is the contract for pre_exec.
        unsafe {
            cmd.pre_exec(move || {
                let cpu_lim = libc::rlimit {
                    rlim_cur: cpu as libc::rlim_t,
                    rlim_max: cpu as libc::rlim_t,
                };
                libc::setrlimit(libc::RLIMIT_CPU, &cpu_lim);
                if let Some(a) = addr {
                    let as_lim = libc::rlimit {
                        rlim_cur: a as libc::rlim_t,
                        rlim_max: a as libc::rlim_t,
                    };
                    libc::setrlimit(libc::RLIMIT_AS, &as_lim);
                }
                Ok(())
            });
        }

        let out_path = temp_log("out");
        let err_path = temp_log("err");
        cmd.stdout(std::fs::File::create(&out_path)?);
        cmd.stderr(std::fs::File::create(&err_path)?);

        let mut child = cmd.spawn()?;
        let pid = child.id() as i32;
        let deadline = Instant::now() + limits.wall;
        let mut timed_out = false;
        let status = loop {
            if let Some(st) = child.try_wait()? {
                break st;
            }
            if Instant::now() >= deadline {
                timed_out = true;
                // Negative pid = the whole process group.
                unsafe {
                    libc::kill(-pid, libc::SIGKILL);
                }
                break child.wait()?;
            }
            std::thread::sleep(Duration::from_millis(25));
        };

        let stdout = std::fs::read_to_string(&out_path).unwrap_or_default();
        let stderr = std::fs::read_to_string(&err_path).unwrap_or_default();
        let _ = std::fs::remove_file(&out_path);
        let _ = std::fs::remove_file(&err_path);
        Ok(Outcome {
            status,
            stdout,
            stderr,
            timed_out,
            containment,
        })
    }

    #[cfg(not(unix))]
    {
        // No process-group kill / rlimits without a Unix; timeout enforcement
        // for Windows is future work.
        let _ = limits;
        let out = cmd.output()?;
        Ok(Outcome {
            status: out.status,
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            timed_out: false,
            containment,
        })
    }
}

#[cfg(unix)]
fn temp_log(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("rb-sbx-{}-{}-{}.log", std::process::id(), tag, n))
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

    // The escape tests: they prove the sandbox actually contains. Network and
    // filesystem confinement are macOS-only for now; the wall-clock timeout is
    // Unix-wide.
    #[cfg(unix)]
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

    #[cfg(unix)]
    #[test]
    fn wall_timeout_kills_runaway() {
        let mut policy = tmp_policy();
        policy.limits.wall = Duration::from_secs(2);
        let start = Instant::now();
        // A busy loop that never returns. The wall clock must kill it.
        let out = run(
            &policy,
            "python3",
            &["-c", "\nwhile True:\n    pass\n"],
            &[],
        )
        .unwrap();
        assert!(out.timed_out, "runaway should have timed out");
        assert!(!out.success(), "a killed process is not a success");
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "should be killed near the 2s deadline, took {:?}",
            start.elapsed()
        );
    }

    #[cfg(unix)]
    #[test]
    fn normal_command_does_not_time_out() {
        let policy = tmp_policy();
        let out = run(&policy, "python3", &["-c", "print('ok')"], &[]).unwrap();
        assert!(!out.timed_out);
        assert!(out.success());
        assert!(out.stdout.contains("ok"));
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
