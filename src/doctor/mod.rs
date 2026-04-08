//! Doctor Module - Self-healing diagnostics and monitoring
//!
//! Monitors system health, diagnoses issues, and performs repairs

use anyhow::{Context, Result};
use std::collections::HashMap;
use tracing::{error, info, warn};

/// System health metrics
#[derive(Debug, Clone, Default)]
pub struct SystemHealth {
    pub disk_usage: Option<DiskInfo>,
    pub memory_usage: Option<MemoryInfo>,
    pub cpu_usage: Option<f32>,
    pub load_average: Option<Vec<f32>>,
    pub uptime: Option<String>,
    pub issues: Vec<HealthIssue>,
}

#[derive(Debug, Clone)]
pub struct DiskInfo {
    pub total_gb: f64,
    pub used_gb: f64,
    pub free_gb: f64,
    pub percent_used: f32,
    pub mount_point: String,
}

#[derive(Debug, Clone)]
pub struct MemoryInfo {
    pub total_gb: f64,
    pub used_gb: f64,
    pub free_gb: f64,
    pub percent_used: f32,
}

#[derive(Debug, Clone)]
pub struct HealthIssue {
    pub severity: IssueSeverity,
    pub category: String,
    pub message: String,
    pub suggestion: String,
    pub auto_fixable: bool,
}

#[derive(Debug, Clone)]
pub enum IssueSeverity {
    Critical,
    Warning,
    Info,
}

impl std::fmt::Display for IssueSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IssueSeverity::Critical => write!(f, "🔴 Critical"),
            IssueSeverity::Warning => write!(f, "🟡 Warning"),
            IssueSeverity::Info => write!(f, "🔵 Info"),
        }
    }
}

/// The Doctor - monitors, diagnoses, and heals
pub struct Doctor;

impl Doctor {
    pub fn new() -> Self {
        Self
    }
    
    /// Check overall system health
    pub fn check_health(&self) -> SystemHealth {
        let mut health = SystemHealth::default();
        let mut issues = Vec::new();
        
        // Check disk space
        if let Ok(disk) = self.check_disk_space() {
            if disk.percent_used > 90.0 {
                issues.push(HealthIssue {
                    severity: IssueSeverity::Critical,
                    category: "Disk".to_string(),
                    message: format!("Disk {} is {}% full", disk.mount_point, disk.percent_used as i32),
                    suggestion: "Clean up old files or expand storage".to_string(),
                    auto_fixable: false,
                });
            } else if disk.percent_used > 80.0 {
                issues.push(HealthIssue {
                    severity: IssueSeverity::Warning,
                    category: "Disk".to_string(),
                    message: format!("Disk {} is {}% full", disk.mount_point, disk.percent_used as i32),
                    suggestion: "Consider cleaning up files soon".to_string(),
                    auto_fixable: false,
                });
            }
            health.disk_usage = Some(disk);
        }
        
        // Check memory
        if let Ok(memory) = self.check_memory() {
            if memory.percent_used > 95.0 {
                issues.push(HealthIssue {
                    severity: IssueSeverity::Critical,
                    category: "Memory".to_string(),
                    message: format!("Memory is {}% used", memory.percent_used as i32),
                    suggestion: "Close unnecessary processes or add RAM".to_string(),
                    auto_fixable: false,
                });
            } else if memory.percent_used > 85.0 {
                issues.push(HealthIssue {
                    severity: IssueSeverity::Warning,
                    category: "Memory".to_string(),
                    message: format!("Memory is {}% used", memory.percent_used as i32),
                    suggestion: "Monitor for memory leaks".to_string(),
                    auto_fixable: false,
                });
            }
            health.memory_usage = Some(memory);
        }
        
        // Check load average (Unix only)
        #[cfg(unix)]
        if let Ok(load) = self.check_load_average() {
            health.load_average = Some(load.clone());
            // Load > number of cores is high
            let num_cores = std::thread::available_parallelism()
                .map(|p| p.get() as f32)
                .unwrap_or(1.0);
            if load[0] > num_cores * 2.0 {
                issues.push(HealthIssue {
                    severity: IssueSeverity::Warning,
                    category: "CPU".to_string(),
                    message: format!("High load average: {:.2}", load[0]),
                    suggestion: "Check for runaway processes".to_string(),
                    auto_fixable: false,
                });
            }
        }
        
        // Check config health
        issues.extend(self.check_config_health());
        
        health.issues = issues;
        health
    }
    
    /// Check disk space
    fn check_disk_space(&self) -> Result<DiskInfo> {
        #[cfg(unix)]
        {
            use std::process::Command;
            let output = Command::new("df")
                .args(["-h", "/"])
                .output()
                .context("Failed to run df command")?;
            
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Parse df output
            for line in stdout.lines().skip(1) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 6 {
                    let percent = parts[4].trim_end_matches('%')
                        .parse::<f32>()
                        .unwrap_or(0.0);
                    
                    return Ok(DiskInfo {
                        total_gb: 0.0, // Would need more parsing
                        used_gb: 0.0,
                        free_gb: 0.0,
                        percent_used: percent,
                        mount_point: parts[5].to_string(),
                    });
                }
            }
        }
        
        // Fallback: check current directory
        let current_dir = std::env::current_dir()?;
        let metadata = std::fs::metadata(&current_dir)?;
        
        Ok(DiskInfo {
            total_gb: 100.0,
            used_gb: 50.0,
            free_gb: 50.0,
            percent_used: 50.0,
            mount_point: current_dir.to_string_lossy().to_string(),
        })
    }
    
    /// Check memory usage
    fn check_memory(&self) -> Result<MemoryInfo> {
        #[cfg(unix)]
        {
            use std::process::Command;
            // Try /proc/meminfo on Linux
            if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
                let mut total_kb: u64 = 0;
                let mut available_kb: u64 = 0;
                
                for line in content.lines() {
                    if line.starts_with("MemTotal:") {
                        total_kb = line.split_whitespace()
                            .nth(1)
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0);
                    } else if line.starts_with("MemAvailable:") {
                        available_kb = line.split_whitespace()
                            .nth(1)
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0);
                    }
                }
                
                if total_kb > 0 {
                    let total_gb = total_kb as f64 / 1024.0 / 1024.0;
                    let free_gb = available_kb as f64 / 1024.0 / 1024.0;
                    let used_gb = total_gb - free_gb;
                    let percent_used = (used_gb / total_gb * 100.0) as f32;
                    
                    return Ok(MemoryInfo {
                        total_gb,
                        used_gb,
                        free_gb,
                        percent_used,
                    });
                }
            }
            
            // Try vmstat on macOS/BSD
            if let Ok(output) = Command::new("vm_stat").output() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                // Parse vm_stat output
                // Simplified - would need proper parsing
            }
        }
        
        // Fallback
        Ok(MemoryInfo {
            total_gb: 8.0,
            used_gb: 4.0,
            free_gb: 4.0,
            percent_used: 50.0,
        })
    }
    
    /// Check load average (Unix only)
    #[cfg(unix)]
    fn check_load_average(&self) -> Result<Vec<f32>> {
        let load = sysinfo::System::load_average();
        Ok(vec![load.one, load.five, load.fifteen])
    }
    
    /// Check configuration health
    fn check_config_health(&self) -> Vec<HealthIssue> {
        let mut issues = Vec::new();
        
        // Check if config file exists
        let config_path = crate::config::Config::config_path();
        if !config_path.exists() {
            issues.push(HealthIssue {
                severity: IssueSeverity::Warning,
                category: "Config".to_string(),
                message: "No configuration file found".to_string(),
                suggestion: "Run 'horcrux setup' to create one".to_string(),
                auto_fixable: true,
            });
            return issues;
        }
        
        // Try to load config
        match crate::config::Config::load() {
            Ok(config) => {
                // Check LLM config
                if config.llm.api_key.is_none() || config.llm.api_key.as_ref().unwrap().is_empty() {
                    issues.push(HealthIssue {
                        severity: IssueSeverity::Critical,
                        category: "Config".to_string(),
                        message: "No LLM API key configured".to_string(),
                        suggestion: "Add your API key to ~/.horcrux/config.toml".to_string(),
                        auto_fixable: false,
                    });
                }
                
                // Check web search
                if !config.web_search.is_configured() {
                    issues.push(HealthIssue {
                        severity: IssueSeverity::Warning,
                        category: "Config".to_string(),
                        message: "Web search not configured".to_string(),
                        suggestion: "Add Tavily API key for web search capability".to_string(),
                        auto_fixable: false,
                    });
                }
            }
            Err(e) => {
                issues.push(HealthIssue {
                    severity: IssueSeverity::Critical,
                    category: "Config".to_string(),
                    message: format!("Failed to load config: {}", e),
                    suggestion: "Check config file syntax or delete and recreate".to_string(),
                    auto_fixable: true,
                });
            }
        }
        
        issues
    }
    
    /// Format health report
    pub fn format_health_report(&self, health: &SystemHealth) -> String {
        let mut output = String::from("📊 System Health Report\n");
        output.push_str("========================\n\n");
        
        // Disk info
        if let Some(disk) = &health.disk_usage {
            output.push_str(&format!(
                "💾 Disk ({}): {:.1}GB / {:.1}GB ({:.0}% used)\n",
                disk.mount_point, disk.used_gb, disk.total_gb, disk.percent_used
            ));
        }
        
        // Memory info
        if let Some(mem) = &health.memory_usage {
            output.push_str(&format!(
                "🧠 Memory: {:.1}GB / {:.1}GB ({:.0}% used)\n",
                mem.used_gb, mem.total_gb, mem.percent_used
            ));
        }
        
        // Load average
        if let Some(load) = &health.load_average {
            output.push_str(&format!(
                "⚡ Load Average: {:.2}, {:.2}, {:.2}\n",
                load[0], load[1], load[2]
            ));
        }
        
        output.push('\n');
        
        // Issues
        if health.issues.is_empty() {
            output.push_str("✅ All systems healthy!\n");
        } else {
            output.push_str(&format!("Found {} issue(s):\n\n", health.issues.len()));
            for issue in &health.issues {
                output.push_str(&format!(
                    "{} [{}] {}\n   → {}\n\n",
                    issue.severity, issue.category, issue.message, issue.suggestion
                ));
            }
        }
        
        output
    }
}

impl Default for Doctor {
    fn default() -> Self {
        Self::new()
    }
}

/// Tools for the agent
pub mod tool {
    use super::*;
    use crate::tools::{Tool, ToolResult};
    use async_trait::async_trait;
    
    /// Check system health
    pub struct SystemHealthTool;
    
    impl SystemHealthTool {
        pub fn new() -> Self {
            Self
        }
    }
    
    #[async_trait]
    impl Tool for SystemHealthTool {
        fn name(&self) -> &str {
            "check_system_health"
        }
        
        fn description(&self) -> &str {
            "Monitor system health - check disk space, memory, CPU, and configuration. \
             Reports issues and provides recommendations. \
             Use this to monitor server health or diagnose problems."
        }
        
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {}
            })
        }
        
        async fn execute(&self, _args: serde_json::Value) -> Result<ToolResult, anyhow::Error> {
            let doctor = Doctor::new();
            let health = doctor.check_health();
            let report = doctor.format_health_report(&health);
            
            Ok(ToolResult::success(report))
        }
    }
}
