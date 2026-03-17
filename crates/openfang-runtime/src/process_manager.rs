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
#[derive(Clone)]
struct ManagedProcess {
    /// stdin writer (lock protects concurrent writes).
    stdin: Arc<Mutex<Option<tokio::process::ChildStdin>>>,
    /// Accumulated stdout output.
    stdout_buf: Arc<Mutex<Vec<String>>>,
    /// Accumulated stderr output.
    stderr_buf: Arc<Mutex<Vec<String>>>,
    /// The child process handle shared with the reaper.
    child: Arc<Mutex<tokio::process::Child>>,
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
    processes: Arc<DashMap<ProcessId, ManagedProcess>>,
    max_per_agent: usize,
    next_id: std::sync::atomic::AtomicU64,
}

impl ProcessManager {
    fn kill_process_nowait(
        process_id: &str,
        child: Arc<Mutex<tokio::process::Child>>,
        agent_id: &str,
    ) {
        let process_id = process_id.to_string();
        let agent_id = agent_id.to_string();
        // Fire-and-forget killing thread so synchronous callers are non-blocking.
        std::thread::spawn(move || {
            if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                rt.block_on(async {
                    let mut child = child.lock().await;
                    if let Some(pid) = child.id() {
                        debug!(process_id = %process_id, pid, agent_id = %agent_id, "Stopping persistent process");
                        let _ = crate::subprocess_sandbox::kill_process_tree(pid, 3000).await;
                    }
                    let _ = child.start_kill();
                    let _ = child.wait().await;
                });
            }
        });
    }

    /// Create a new process manager.
    pub fn new(max_per_agent: usize) -> Self {
        Self {
            processes: Arc::new(DashMap::new()),
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

        let stdin = Arc::new(Mutex::new(stdin));
        let child = Arc::new(Mutex::new(child));

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
                child: Arc::clone(&child),
                agent_id: agent_id.to_string(),
                command: cmd_display,
                started_at: std::time::Instant::now(),
            },
        );

        // Reaper: wait for the process to exit and free the slot.
        {
            let processes = self.processes.clone();
            let child = Arc::clone(&child);
            let id = id.clone();
            let agent_id = agent_id.to_string();
            tokio::spawn(async move {
                let _ = child.lock().await.wait().await;
                if processes.remove(&id).is_some() {
                    debug!(process_id = %id, agent_id = %agent_id, "Persistent process exited");
                }
            });
        }

        Ok(id)
    }

    /// Write data to a process's stdin.
    pub async fn write(&self, process_id: &str, data: &str) -> Result<(), String> {
        let entry = self
            .processes
            .get(process_id)
            .ok_or_else(|| format!("Process '{}' not found", process_id))?;

        let stdin = Arc::clone(&entry.value().stdin);
        drop(entry);

        let mut guard = stdin.lock().await;
        if let Some(stdin) = guard.as_mut() {
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

        let stdout_buf = Arc::clone(&entry.value().stdout_buf);
        let stderr_buf = Arc::clone(&entry.value().stderr_buf);
        drop(entry);

        let mut stdout = stdout_buf.lock().await;
        let mut stderr = stderr_buf.lock().await;

        let out_lines: Vec<String> = stdout.drain(..).collect();
        let err_lines: Vec<String> = stderr.drain(..).collect();

        Ok((out_lines, err_lines))
    }

    /// Kill a process.
    pub async fn kill(&self, process_id: &str) -> Result<(), String> {
        let (_, proc) = self
            .processes
            .remove(process_id)
            .ok_or_else(|| format!("Process '{}' not found", process_id))?;

        Self::kill_process_nowait(process_id, Arc::clone(&proc.child), &proc.agent_id);
        Ok(())
    }

    /// Best-effort synchronous cleanup for every process owned by an agent.
    pub fn kill_agent_processes(&self, agent_id: &str) -> usize {
        let ids: Vec<ProcessId> = self
            .processes
            .iter()
            .filter(|entry| entry.value().agent_id == agent_id)
            .map(|entry| entry.key().clone())
            .collect();

        for id in &ids {
            if let Some((_, proc)) = self.processes.remove(id) {
                Self::kill_process_nowait(id, Arc::clone(&proc.child), &proc.agent_id);
            }
        }

        ids.len()
    }

    /// Best-effort synchronous shutdown for all managed processes.
    pub fn shutdown_all(&self) -> usize {
        let ids: Vec<ProcessId> = self
            .processes
            .iter()
            .map(|entry| entry.key().clone())
            .collect();

        for id in &ids {
            if let Some((_, proc)) = self.processes.remove(id) {
                Self::kill_process_nowait(id, Arc::clone(&proc.child), &proc.agent_id);
            }
        }

        ids.len()
    }

    /// List all processes for an agent.
    pub fn list(&self, agent_id: &str) -> Vec<ProcessInfo> {
        self.processes
            .iter()
            .filter(|entry| entry.value().agent_id == agent_id)
            .map(|entry| {
                let alive = entry
                    .value()
                    .child
                    .try_lock()
                    .map(|child| child.id().is_some())
                    .unwrap_or(true);
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
    use tokio::time::{sleep, Duration};

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

        let _ = pm.kill(&id1).await;
    }

    #[tokio::test]
    async fn test_natural_exit_releases_slot() {
        let pm = ProcessManager::new(1);
        let (cmd, args) = if cfg!(windows) {
            (
                "cmd",
                vec![
                    "/C".to_string(),
                    "timeout".to_string(),
                    "/T".to_string(),
                    "1".to_string(),
                    "/NOBREAK".to_string(),
                ],
            )
        } else {
            ("sh", vec!["-c".to_string(), "sleep 0.05".to_string()])
        };

        let id = pm.start("agent1", cmd, &args).await.unwrap();
        for _ in 0..20 {
            if pm.count() == 0 {
                break;
            }
            sleep(Duration::from_millis(50)).await;
        }
        assert_eq!(pm.count(), 0, "reaper should remove finished process");
        assert!(
            pm.start("agent1", cmd, &args).await.is_ok(),
            "slot should be free"
        );

        let _ = pm.kill(&id).await;
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
