//! NVIDIA GPU telemetry via `nvidia-smi` (dual-GPU workstation layout).

use serde::Serialize;
use std::collections::HashMap;
use std::time::Duration;
use tokio::process::Command;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuSlot {
    pub temp: u32,
    pub usage: u32,
    pub memory: f64,
    #[serde(rename = "memoryTotal")]
    pub memory_total: u32,
    pub processes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuStats {
    pub gpu0: GpuSlot,
    pub gpu1: GpuSlot,
    pub total_vram: u32,
    pub used_vram: f64,
    pub model: String,
    pub available: bool,
}

fn empty_slot() -> GpuSlot {
    GpuSlot {
        temp: 0,
        usage: 0,
        memory: 0.0,
        memory_total: 24,
        processes: vec![],
    }
}

fn default_stats() -> GpuStats {
    GpuStats {
        gpu0: empty_slot(),
        gpu1: empty_slot(),
        total_vram: 48,
        used_vram: 0.0,
        model: std::env::var("VLLM_MODEL_DEFAULT")
            .or_else(|_| std::env::var("OLLAMA_MODEL_DEFAULT"))
            .unwrap_or_else(|_| "vLLM".to_string()),
        available: false,
    }
}

fn short_process_name(raw: &str) -> String {
    let name = raw.trim();
    if name.is_empty() {
        return "unknown".to_string();
    }
    let lower = name.to_lowercase();
    if lower.contains("vllm") {
        return "VLLM::EngineCore".to_string();
    }
    if lower.contains("gnome-shell") {
        return "gnome-shell".to_string();
    }
    if lower.contains("cursor") {
        return "cursor".to_string();
    }
    if lower.contains("chrome") {
        return "chrome".to_string();
    }
    let without_args = name.split_whitespace().next().unwrap_or(name);
    let base = without_args
        .rsplit('/')
        .next()
        .unwrap_or(without_args);
    if base.len() > 28 {
        format!("{}…", &base[..26])
    } else {
        base.to_string()
    }
}

async fn run_nvidia_smi(args: &[&str]) -> Option<String> {
    let output = Command::new("nvidia-smi")
        .args(args)
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

fn parse_f64(s: &str) -> f64 {
    s.trim().parse().unwrap_or(0.0)
}

/// Collect GPU0 (display) and GPU1 (compute/vLLM) stats from `nvidia-smi`.
pub async fn collect_gpu_stats() -> GpuStats {
    let mut stats = default_stats();

    let stdout = match run_nvidia_smi(&[
        "--query-gpu=temperature.gpu,utilization.gpu,memory.used,memory.total",
        "--format=csv,noheader,nounits",
    ])
    .await
    {
        Some(s) if !s.trim().is_empty() => s,
        _ => return stats,
    };

    stats.available = true;
    let lines: Vec<&str> = stdout.trim().lines().collect();
    let mut total_used_mb = 0.0_f64;
    let mut total_vram_mb = 0.0_f64;

    if let Some(line) = lines.first() {
        let parts: Vec<&str> = line.split(',').map(str::trim).collect();
        if parts.len() >= 4 {
            let mem_used = parse_f64(parts[2]);
            let mem_total = parse_f64(parts[3]);
            stats.gpu0 = GpuSlot {
                temp: parse_f64(parts[0]) as u32,
                usage: parse_f64(parts[1]) as u32,
                memory: (mem_used / 1024.0 * 10.0).round() / 10.0,
                memory_total: (mem_total / 1024.0).round() as u32,
                processes: vec![],
            };
            total_used_mb += mem_used;
            total_vram_mb += mem_total;
        }
    }

    if let Some(line) = lines.get(1) {
        let parts: Vec<&str> = line.split(',').map(str::trim).collect();
        if parts.len() >= 4 {
            let mem_used = parse_f64(parts[2]);
            let mem_total = parse_f64(parts[3]);
            stats.gpu1 = GpuSlot {
                temp: parse_f64(parts[0]) as u32,
                usage: parse_f64(parts[1]) as u32,
                memory: (mem_used / 1024.0 * 10.0).round() / 10.0,
                memory_total: (mem_total / 1024.0).round() as u32,
                processes: vec![],
            };
            total_used_mb += mem_used;
            total_vram_mb += mem_total;
        }
    }

    stats.used_vram = (total_used_mb / 1024.0 * 10.0).round() / 10.0;
    stats.total_vram = (total_vram_mb / 1024.0).round() as u32;

    if let Some(uuid_out) = run_nvidia_smi(&[
        "--query-gpu=index,uuid",
        "--format=csv,noheader",
    ])
    .await
    {
        let mut uuid_to_index: HashMap<String, usize> = HashMap::new();
        for row in uuid_out.trim().lines() {
            let mut parts = row.splitn(2, ',');
            let idx_str = parts.next().unwrap_or("").trim();
            let uuid = parts.next().unwrap_or("").trim();
            if let Ok(idx) = idx_str.parse::<usize>() {
                if !uuid.is_empty() {
                    uuid_to_index.insert(uuid.to_string(), idx);
                }
            }
        }

        if let Some(proc_out) = run_nvidia_smi(&[
            "--query-compute-apps=gpu_uuid,pid,process_name,used_gpu_memory",
            "--format=csv,noheader,nounits",
        ])
        .await
        {
            let mut proc_by_gpu: HashMap<usize, Vec<String>> =
                HashMap::from([(0, vec![]), (1, vec![])]);
            for row in proc_out.trim().lines() {
                if row.trim().is_empty() {
                    continue;
                }
                let first_comma = row.find(',').unwrap_or(0);
                let second_comma = row[first_comma + 1..]
                    .find(',')
                    .map(|i| first_comma + 1 + i)
                    .unwrap_or(0);
                let last_comma = row.rfind(',').unwrap_or(0);
                if second_comma == 0 || last_comma <= second_comma {
                    continue;
                }
                let uuid = row[..first_comma].trim();
                let proc_name = row[second_comma + 1..last_comma].trim();
                if let Some(&idx) = uuid_to_index.get(uuid) {
                    if idx == 0 || idx == 1 {
                        proc_by_gpu
                            .entry(idx)
                            .or_default()
                            .push(short_process_name(proc_name));
                    }
                }
            }
            if let Some(p) = proc_by_gpu.get(&0) {
                let mut unique: Vec<String> = p.clone();
                unique.sort();
                unique.dedup();
                stats.gpu0.processes = unique;
            }
            if let Some(p) = proc_by_gpu.get(&1) {
                let mut unique: Vec<String> = p.clone();
                unique.sort();
                unique.dedup();
                stats.gpu1.processes = unique;
            }
        }
    }

    // Optional: resolve vLLM model name from /models endpoint
    let provider = std::env::var("LLM_PROVIDER").unwrap_or_default();
    let vllm_base = std::env::var("VLLM_BASE_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8000/v1".to_string())
        .trim_end_matches('/')
        .to_string();
    if provider == "vllm" || provider.is_empty() || vllm_base.contains(":8000") {
        if let Ok(client) = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
        {
            let mut req = client.get(format!("{vllm_base}/models"));
            if let Ok(key) = std::env::var("VLLM_API_KEY") {
                if !key.is_empty() {
                    req = req.header("Authorization", format!("Bearer {key}"));
                }
            }
            if let Ok(resp) = req.send().await {
                if resp.status().is_success() {
                    if let Ok(body) = resp.json::<serde_json::Value>().await {
                        if let Some(id) = body
                            .get("data")
                            .and_then(|d| d.as_array())
                            .and_then(|a| a.first())
                            .and_then(|m| m.get("id"))
                            .and_then(|v| v.as_str())
                        {
                            stats.model = id.to_string();
                        }
                    }
                }
            }
        }
    }

    stats
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_process_name_vllm() {
        assert_eq!(
            short_process_name("python -m vllm.entrypoints"),
            "VLLM::EngineCore"
        );
    }
}
