//! Interactive process manager — persistent process sessions.
//!
//! Allows agents to start long-running processes (REPLs, servers, watchers),
//! write to their stdin, read from stdout/stderr, and kill them.

use dashmap::DashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;
use tracing::{debug, warn};

/// Unique process identifier.
pub type ProcessId = String;

/// A managed persistent process.
struct ManagedProcess {
    /// stdin writer.
    stdin: Option<tokio::process::ChildStdin>,
    /// Accumulated stdout output.
    stdout_buf: Arc<Mutex<Vec<String>>>,
    /// Accumulated stderr output.
    stderr_buf: Arc<Mutex<Vec<String>>>,
    /// The child process handle.
    child: tokio::process::Child,
    /// Agent that owns this process.
    agent_id: String,
    /// Command that was started.
    command: String,
    /// When the process was started.
    started_at: std::time::Instant,
}

/// Process info for listing.
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    /// Process ID.
    pub id: ProcessId,
    /// Agent that owns this process.
    pub agent_id: String,
    /// Command that was started.
    pub command: String,
    /// Whether the process is still running.
    pub alive: bool,
    /// Uptime in seconds.
    pub uptime_secs: u64,
}

/// Manager for persistent agent processes.
pub struct ProcessManager {
    processes: DashMap<ProcessId, ManagedProcess>,
    max_per_agent: usize,
    next_id: std::sync::atomic::AtomicU64,
}

impl ProcessManager {
    /// Create a new process manager.
    pub fn new(max_per_agent: usize) -> Self {
        Self {
            processes: DashMap::new(),
            max_per_agent,
            next_id: std::sync::atomic::AtomicU64::new(1),
        }
    }

    /// Start a persistent process. Returns the process ID.
    pub async fn start(
        &self,
        agent_id: &str,
        command: &str,
        args: &[String],
    ) -> Result<ProcessId, String> {
        // Check per-agent limit
        let agent_count = self
            .processes
            .iter()
            .filter(|entry| entry.value().agent_id == agent_id)
            .count();

        if agent_count >= self.max_per_agent {
            return Err(format!(
                "Agent '{}' already has {} processes (max: {})",
                agent_id, agent_count, self.max_per_agent
            ));
        }

        let mut child = tokio::process::Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to start process '{}': {}", command, e))?;

        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let stdout_buf = Arc::new(Mutex::new(Vec::<String>::new()));
        let stderr_buf = Arc::new(Mutex::new(Vec::<String>::new()));

        // Spawn background readers for stdout/stderr
        if let Some(out) = stdout {
            let buf = stdout_buf.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(out);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let mut b = buf.lock().await;
                    // Cap buffer at 1000 lines
                    if b.len() >= 1000 {
                        b.drain(..100); // remove oldest 100
                    }
                    b.push(line);
                }
            });
        }

        if let Some(err) = stderr {
            let buf = stderr_buf.clone();
            tokio::spawn(async move {
                let reader = BufReader::new(err);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let mut b = buf.lock().await;
                    if b.len() >= 1000 {
                        b.drain(..100);
                    }
                    b.push(line);
                }
            });
        }

        let id = format!(
            "proc_{}",
            self.next_id
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        );

        let cmd_display = if args.is_empty() {
            command.to_string()
        } else {
            format!("{} {}", command, args.join(" "))
        };

        debug!(process_id = %id, command = %cmd_display, agent = %agent_id, "Started persistent process");

        self.processes.insert(
            id.clone(),
            ManagedProcess {
                stdin,
                stdout_buf,
                stderr_buf,
                child,
                agent_id: agent_id.to_string(),
                command: cmd_display,
                started_at: std::time::Instant::now(),
            },
        );

        Ok(id)
    }

    /// Write data to a process's stdin.
    pub async fn write(&self, process_id: &str, data: &str) -> Result<(), String> {
        let mut entry = self
            .processes
            .get_mut(process_id)
            .ok_or_else(|| format!("Process '{}' not found", process_id))?;

        let proc = entry.value_mut();
        if let Some(stdin) = &mut proc.stdin {
            stdin
                .write_all(data.as_bytes())
                .await
                .map_err(|e| format!("Write failed: {}", e))?;
            stdin
                .flush()
                .await
                .map_err(|e| format!("Flush failed: {}", e))?;
            Ok(())
        } else {
            Err("Process stdin is closed".to_string())
        }
    }

    /// Read accumulated stdout/stderr (non-blocking drain).
    pub async fn read(&self, process_id: &str) -> Result<(Vec<String>, Vec<String>), String> {
        let entry = self
            .processes
            .get(process_id)
            .ok_or_else(|| format!("Process '{}' not found", process_id))?;

        let mut stdout = entry.stdout_buf.lock().await;
        let mut stderr = entry.stderr_buf.lock().await;

        let out_lines: Vec<String> = stdout.drain(..).collect();
        let err_lines: Vec<String> = stderr.drain(..).collect();

        Ok((out_lines, err_lines))
    }

    /// Kill a process.
    pub async fn kill(&self, process_id: &str) -> Result<(), String> {
        let (_, mut proc) = self
            .processes
            .remove(process_id)
            .ok_or_else(|| format!("Process '{}' not found", process_id))?;

        if let Some(pid) = proc.child.id() {
            debug!(process_id, pid, "Killing persistent process");
            let _ = crate::subprocess_sandbox::kill_process_tree(pid, 3000).await;
        }
        let _ = proc.child.kill().await;
        Ok(())
    }

    /// List all processes for an agent.
    pub fn list(&self, agent_id: &str) -> Vec<ProcessInfo> {
        self.processes
            .iter()
            .filter(|entry| entry.value().agent_id == agent_id)
            .map(|entry| {
                let alive = entry.value().child.id().is_some();
                ProcessInfo {
                    id: entry.key().clone(),
                    agent_id: entry.value().agent_id.clone(),
                    command: entry.value().command.clone(),
                    alive,
                    uptime_secs: entry.value().started_at.elapsed().as_secs(),
                }
            })
            .collect()
    }

    /// Verify that a process belongs to the provided agent/session scope.
    pub fn verify_owner(&self, process_id: &str, agent_id: &str) -> Result<(), String> {
        let entry = self
            .processes
            .get(process_id)
            .ok_or_else(|| format!("Process '{}' not found", process_id))?;

        if entry.value().agent_id != agent_id {
            return Err(format!(
                "Process '{}' is not owned by '{}'",
                process_id, agent_id
            ));
        }

        Ok(())
    }

    /// Cleanup: kill processes older than timeout.
    pub async fn cleanup(&self, max_age_secs: u64) {
        let to_remove: Vec<ProcessId> = self
            .processes
            .iter()
            .filter(|entry| entry.value().started_at.elapsed().as_secs() > max_age_secs)
            .map(|entry| entry.key().clone())
            .collect();

        for id in to_remove {
            warn!(process_id = %id, "Cleaning up stale process");
            let _ = self.kill(&id).await;
        }
    }

    /// Total process count.
    pub fn count(&self) -> usize {
        self.processes.len()
    }
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new(5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_start_and_list() {
        let pm = ProcessManager::new(5);

        let cmd = if cfg!(windows) { "cmd" } else { "cat" };
        let args: Vec<String> = if cfg!(windows) {
            vec!["/C".to_string(), "echo".to_string(), "hello".to_string()]
        } else {
            vec![]
        };

        let id = pm.start("agent1", cmd, &args).await.unwrap();
        assert!(id.starts_with("proc_"));

        let list = pm.list("agent1");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].agent_id, "agent1");

        // Cleanup
        let _ = pm.kill(&id).await;
    }

    #[tokio::test]
    async fn test_per_agent_limit() {
        let pm = ProcessManager::new(1);

        let cmd = if cfg!(windows) { "cmd" } else { "cat" };
        let args: Vec<String> = if cfg!(windows) {
            vec![
                "/C".to_string(),
                "timeout".to_string(),
                "/t".to_string(),
                "10".to_string(),
            ]
        } else {
            vec![]
        };

        let id1 = pm.start("agent1", cmd, &args).await.unwrap();
        let result = pm.start("agent1", cmd, &args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("max: 1"));

        // Avoid kill_process_tree() in this unit test because it may target the
        // whole process group under some test runners.
        if let Some((_, mut proc)) = pm.processes.remove(&id1) {
            let _ = proc.child.kill().await;
        }
    }

    #[tokio::test]
    async fn test_kill_nonexistent() {
        let pm = ProcessManager::new(5);
        let result = pm.kill("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_read_nonexistent() {
        let pm = ProcessManager::new(5);
        let result = pm.read("nonexistent").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_default_process_manager() {
        let pm = ProcessManager::default();
        assert_eq!(pm.max_per_agent, 5);
        assert_eq!(pm.count(), 0);
    }
}
