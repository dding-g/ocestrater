use crate::agent::AgentAdapter;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

const BATCH_INTERVAL_MS: u64 = 16; // ~60fps batching
const MAX_SCROLLBACK: usize = 50_000; // lines

/// Represents a single PTY session for an agent
pub struct PtySession {
    writer: Box<dyn Write + Send>,
    _child: Box<dyn portable_pty::Child + Send>,
    pub alive: Arc<Mutex<bool>>,
}

/// Manages all PTY sessions
pub struct PtyManager {
    sessions: HashMap<String, PtySession>,
    app_handle: AppHandle,
    max_sessions: usize,
}

impl PtyManager {
    pub fn new(app_handle: AppHandle, max_sessions: usize) -> Self {
        Self {
            sessions: HashMap::new(),
            app_handle,
            max_sessions,
        }
    }

    /// Spawn a new PTY session for a workspace
    pub fn spawn(
        &mut self,
        workspace_id: &str,
        adapter: &AgentAdapter,
        working_dir: &str,
        model: Option<&str>,
        secret_env: Option<&std::collections::HashMap<String, String>>,
    ) -> Result<(), String> {
        if self.sessions.len() >= self.max_sessions {
            return Err(format!(
                "maximum concurrent agents reached ({}). Stop an existing agent to create a new one.",
                self.max_sessions
            ));
        }

        if self.sessions.contains_key(workspace_id) {
            return Err(format!("session already exists: {workspace_id}"));
        }

        let pty_system = NativePtySystem::default();
        let pair = pty_system
            .openpty(PtySize {
                rows: 40,
                cols: 120,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("pty open error: {e}"))?;

        let (command, args) = adapter.build_command(model);
        let mut cmd = CommandBuilder::new(&command);
        cmd.args(&args);
        cmd.cwd(working_dir);

        // Set env vars from adapter
        for (k, v) in adapter.env_vars() {
            cmd.env(k, v);
        }

        // Inject Keychain secrets as environment variables
        if let Some(secrets) = secret_env {
            for (k, v) in secrets {
                cmd.env(k, v);
            }
        }

        // Force color output
        cmd.env("FORCE_COLOR", "1");
        cmd.env("TERM", "xterm-256color");

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("spawn error: {e}"))?;

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("reader clone error: {e}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| format!("writer error: {e}"))?;

        let alive = Arc::new(Mutex::new(true));
        let alive_clone = alive.clone();
        let ws_id = workspace_id.to_string();
        let handle = self.app_handle.clone();

        // Spawn reader thread with batched IPC
        std::thread::spawn(move || {
            let buf_reader = BufReader::new(reader);
            let mut batch = String::new();
            let mut last_flush = std::time::Instant::now();

            for line in buf_reader.lines() {
                match line {
                    Ok(text) => {
                        batch.push_str(&text);
                        batch.push('\n');

                        let elapsed = last_flush.elapsed().as_millis() as u64;
                        if elapsed >= BATCH_INTERVAL_MS || batch.len() > 4096 {
                            let _ = handle.emit(
                                &format!("pty-output-{}", ws_id),
                                batch.clone(),
                            );
                            batch.clear();
                            last_flush = std::time::Instant::now();
                        }
                    }
                    Err(_) => break,
                }
            }

            // Flush remaining
            if !batch.is_empty() {
                let _ = handle.emit(&format!("pty-output-{}", ws_id), batch);
            }

            // Mark session as dead
            if let Ok(mut a) = alive_clone.lock() {
                *a = false;
            }
            let _ = handle.emit(&format!("pty-exit-{}", ws_id), ());
        });

        self.sessions.insert(
            workspace_id.to_string(),
            PtySession {
                writer,
                _child: child,
                alive,
            },
        );

        Ok(())
    }

    /// Write data to a PTY session's stdin
    pub fn write(&mut self, workspace_id: &str, data: &str) -> Result<(), String> {
        let session = self
            .sessions
            .get_mut(workspace_id)
            .ok_or_else(|| format!("no session: {workspace_id}"))?;

        session
            .writer
            .write_all(data.as_bytes())
            .map_err(|e| format!("write error: {e}"))?;
        session
            .writer
            .write_all(b"\n")
            .map_err(|e| format!("write error: {e}"))?;
        session
            .writer
            .flush()
            .map_err(|e| format!("flush error: {e}"))?;

        Ok(())
    }

    /// Kill a PTY session
    pub fn kill(&mut self, workspace_id: &str) -> Result<(), String> {
        if let Some(session) = self.sessions.remove(workspace_id) {
            if let Ok(mut alive) = session.alive.lock() {
                *alive = false;
            }
            // Drop writer to close PTY
            drop(session.writer);
        }
        Ok(())
    }

    /// Check if a session is alive
    pub fn is_alive(&self, workspace_id: &str) -> bool {
        self.sessions
            .get(workspace_id)
            .and_then(|s| s.alive.lock().ok().map(|a| *a))
            .unwrap_or(false)
    }

    /// List active session IDs
    pub fn active_sessions(&self) -> Vec<String> {
        self.sessions
            .iter()
            .filter(|(_, s)| s.alive.lock().map(|a| *a).unwrap_or(false))
            .map(|(id, _)| id.clone())
            .collect()
    }
}
