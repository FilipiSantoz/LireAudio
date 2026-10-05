use super::{AudioRouter, RoutingError};
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

pub struct LinuxRouter {
    module_index: Option<u32>,
    previous_default: Option<String>,
    watcher_stop: Arc<AtomicBool>,
    watcher_thread: Option<JoinHandle<()>>,
    watcher_child: Option<Arc<Mutex<Option<Child>>>>,
}

impl Default for LinuxRouter {
    fn default() -> Self {
        Self {
            module_index: None,
            previous_default: None,
            watcher_stop: Arc::new(AtomicBool::new(false)),
            watcher_thread: None,
            watcher_child: None,
        }
    }
}

impl LinuxRouter {
    pub fn new() -> Self {
        Self::default()
    }

    fn cleanup_zombies() {
        let _ = Command::new("sh")
            .args([
                "-c",
                "pactl list short modules 2>/dev/null | awk '/luna_in/{print $1}' | xargs -r pactl unload-module 2>/dev/null"
            ])
            .output();
    }

    fn block_is_own(block: &str) -> bool {
        let pid = std::process::id().to_string();

        if block.contains(&format!("application.process.id = \"{pid}\""))
            || block.contains(&format!("application.process.id = {pid}"))
        {
            return true;
        }

        if let Ok(exe) = std::env::current_exe()
            && let Some(name) = exe.file_name().and_then(|n| n.to_str())
            && (block.contains(&format!("application.process.binary = \"{name}\""))
                || block.contains(&format!("application.name = \"{name}\"")))
        {
            return true;
        }

        if block.contains("luna-analyzer") || block.contains("luna_analyzer") {
            return true;
        }

        false
    }

    fn create_sink(&mut self) -> Result<u32, RoutingError> {
        Self::cleanup_zombies();

        let out = Command::new("pactl")
            .args([
                "load-module",
                "module-null-sink",
                "sink_name=luna_in",
                "sink_properties=device.description=LunaInput",
            ])
            .output()
            .map_err(|e| RoutingError::CommandFailed(e.to_string()))?;

        if !out.status.success() {
            return Err(RoutingError::CommandFailed(
                String::from_utf8_lossy(&out.stderr).into_owned(),
            ));
        }

        let _ = Command::new("pactl")
            .args(["set-sink-volume", "luna_in", "100%"])
            .output();

        String::from_utf8(out.stdout)
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
            .ok_or_else(|| RoutingError::ParseFailed("module index".into()))
    }

    fn get_default_sink() -> Option<String> {
        Command::new("pactl")
            .args(["get-default-sink"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
    }

    fn move_foreign_new_input(id: &str, target: &str) {
        let Ok(out) = Command::new("pactl").args(["list", "sink-inputs"]).output() else {
            return;
        };
        let stdout = String::from_utf8_lossy(&out.stdout);

        for block in stdout.split("Sink Input #").skip(1) {
            let block_id = block.lines().next().unwrap_or_default().trim();
            if block_id != id {
                continue;
            }

            if Self::block_is_own(block) {
                return;
            }

            let _ = Command::new("pactl")
                .args(["move-sink-input", id, target])
                .output();
            return;
        }
    }

    fn move_all_safe_inputs(target: &str) {
        let Ok(out) = Command::new("pactl").args(["list", "sink-inputs"]).output() else {
            return;
        };
        let stdout = String::from_utf8_lossy(&out.stdout);

        for block in stdout.split("Sink Input #").skip(1) {
            if Self::block_is_own(block) {
                continue;
            }
            let id = block.lines().next().unwrap_or_default().trim().to_string();
            if !id.is_empty() {
                let _ = Command::new("pactl")
                    .args(["move-sink-input", &id, target])
                    .output();
            }
        }
    }

    fn destroy_sink(module_index: u32) {
        let _ = Command::new("pactl")
            .args(["unload-module", &module_index.to_string()])
            .output();
    }

    fn start_event_watcher(&mut self) {
        let stop = self.watcher_stop.clone();
        let child_slot = Arc::new(Mutex::new(None::<Child>));
        let child_slot2 = child_slot.clone();

        self.watcher_thread = Some(thread::spawn(move || {
            let Ok(mut child) = Command::new("pactl")
                .arg("subscribe")
                .stdout(Stdio::piped())
                .spawn()
            else {
                return;
            };

            let stdout = child.stdout.take();
            *child_slot2.lock().unwrap() = Some(child);

            if let Some(stdout) = stdout {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    if line.contains("Event 'new' on sink-input")
                        && let Some(id_part) = line.split('#').nth(1)
                    {
                        let id = id_part.trim().to_string();
                        thread::sleep(std::time::Duration::from_millis(80));
                        Self::move_foreign_new_input(&id, "luna_in");
                    }
                }
            }
            if let Some(mut c) = child_slot2.lock().unwrap().take() {
                let _ = c.kill();
            }
        }));

        self.watcher_child = Some(child_slot);
    }
}

impl AudioRouter for LinuxRouter {
    fn setup(&mut self) -> Result<(), RoutingError> {
        self.previous_default = Self::get_default_sink();

        let id = self.create_sink()?;
        self.module_index = Some(id);

        std::thread::sleep(std::time::Duration::from_millis(50));

        Self::move_all_safe_inputs("luna_in");

        self.start_event_watcher();

        Ok(())
    }

    fn teardown(&mut self) {
        self.watcher_stop.store(true, Ordering::Relaxed);

        if let Some(slot) = &self.watcher_child
            && let Some(mut c) = slot.lock().unwrap().take()
        {
            let _ = c.kill();
        }
        if let Some(t) = self.watcher_thread.take() {
            let _ = t.join();
        }

        if let Some(prev) = &self.previous_default {
            Self::move_all_safe_inputs(prev);
            let _ = Command::new("pactl")
                .args(["set-default-sink", prev])
                .output();
        }

        if let Some(id) = self.module_index.take() {
            Self::destroy_sink(id);
        }
    }

    fn input_device_hint(&self) -> Option<String> {
        if self.module_index.is_some() {
            Some("luna".to_string())
        } else {
            None
        }
    }
}

impl Drop for LinuxRouter {
    fn drop(&mut self) {
        self.teardown();
    }
}
