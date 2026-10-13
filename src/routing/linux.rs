use super::{AudioRouter, RoutingError};
use serde_json::Value;
use std::collections::HashMap;
use std::process::Command;

#[derive(Default)]
pub struct LinuxRouter {
    module_index: Option<u32>,
    previous_default: Option<String>,
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

    fn create_sink() -> Result<u32, RoutingError> {
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

    fn default_sink_name() -> Option<String> {
        let out = Command::new("pactl")
            .args(["get-default-sink"])
            .output()
            .ok()?;

        String::from_utf8(out.stdout)
            .ok()
            .map(|s| s.trim().to_string())
    }

    fn pactl_json(args: &[&str]) -> Option<Value> {
        let out = Command::new("pactl")
            .arg("-f")
            .arg("json")
            .args(args)
            .output()
            .ok()?;
        serde_json::from_slice(&out.stdout).ok()
    }

    fn active_sink_description() -> Option<String> {
        let sinks = Self::pactl_json(&["list", "sinks"])?;
        let inputs = Self::pactl_json(&["list", "sink-inputs"])?;

        let my_pid = std::process::id();
        let default_name = Self::default_sink_name();

        let mut counts: HashMap<u64, usize> = HashMap::new();

        for stream in inputs.as_array()? {
            let pid = &stream["properties"]["application.process.id"];
            let my_pid_string = my_pid.to_string();

            let is_own =
                pid.as_str() == Some(my_pid_string.as_str()) || pid.as_u64() == Some(my_pid as u64);

            if is_own {
                continue;
            }

            if stream["corked"].as_bool() == Some(true) {
                continue;
            }

            if let Some(sink_index) = stream["sink"].as_u64() {
                *counts.entry(sink_index).or_default() += 1;
            }
        }

        let busiest = sinks
            .as_array()?
            .iter()
            .filter_map(|sink| {
                let name = sink["name"].as_str()?;

                if name.contains("luna_in") {
                    return None;
                }

                let index = sink["index"].as_u64()?;
                let streams = *counts.get(&index)?;

                if streams == 0 {
                    return None;
                }

                let description = sink["description"].as_str()?.to_string();
                let is_default = default_name.as_deref() == Some(name);

                Some((streams, is_default, description))
            })
            .max_by_key(|(streams, is_default, _)| (*streams, *is_default))
            .map(|(_, _, description)| description);

        busiest.or_else(|| Self::default_sink_description(&sinks))
    }

    fn default_sink_description(sinks: &Value) -> Option<String> {
        let out = Command::new("pactl")
            .args(["get-default-sink"])
            .output()
            .ok()?;
        let default_name = String::from_utf8(out.stdout).ok()?;
        let default_name = default_name.trim();

        sinks
            .as_array()?
            .iter()
            .find(|s| s["name"].as_str() == Some(default_name))
            .and_then(|s| s["description"].as_str().map(str::to_string))
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
}

impl AudioRouter for LinuxRouter {
    fn setup(&mut self) -> Result<(), RoutingError> {
        // self.previous_default = Self::get_default_sink();

        let id = Self::create_sink()?;
        self.module_index = Some(id);

        std::thread::sleep(std::time::Duration::from_millis(50));
        // Self::move_all_safe_inputs("luna_in");

        Ok(())
    }

    fn teardown(&mut self) {
        if let Some(prev) = &self.previous_default {
            Self::move_all_safe_inputs(prev);
            let _ = Command::new("pactl")
                .args(["set-default-sink", prev])
                .output();
        }

        if let Some(id) = self.module_index.take() {
            let _ = Command::new("pactl")
                .args(["unload-module", &id.to_string()])
                .output();
        }
    }

    fn input_device_hint(&self) -> Option<String> {
        Self::active_sink_description()
    }
}

impl Drop for LinuxRouter {
    fn drop(&mut self) {
        self.teardown();
    }
}
