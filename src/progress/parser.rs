use super::state::CloneStatus;
use regex::Regex;
use std::sync::LazyLock;

static RECEIVING_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)Receiving objects:\s*(\d+)%\s*\((\d+)/(\d+)\),\s*([\d.]+)\s*([KMG]?i?B)/s")
        .unwrap()
});

static RESOLVING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)Resolving deltas:\s*(\d+)%\s*\((\d+)/(\d+)\)").unwrap());

static COUNTING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)Counting objects:\s*(\d+)%\s*\((\d+)/(\d+)\)").unwrap());

static COMPRESSING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)Compressing objects:\s*(\d+)%\s*\((\d+)/(\d+)\)").unwrap());

pub fn apply_git_line(status: &mut CloneStatus, line: &str) {
    if line.contains("Receiving objects") {
        status.set_phase_active(1);
        if let Some(caps) = RECEIVING_RE.captures(line) {
            let pct: u8 = caps[1].parse().unwrap_or(0);
            let speed = format!("{} {}/s", &caps[4], &caps[5]);
            status.set_speed(&speed);
            status.set_phase_percent(1, pct, &speed);
        }
    } else if line.contains("Resolving deltas") {
        status.set_phase_active(2);
        if let Some(caps) = RESOLVING_RE.captures(line) {
            let pct: u8 = caps[1].parse().unwrap_or(0);
            status.set_phase_percent(2, pct, "");
        }
    } else if line.contains("Counting objects") {
        status.set_phase_active(0);
        if let Some(caps) = COUNTING_RE.captures(line) {
            let pct: u8 = caps[1].parse().unwrap_or(0);
            status.set_phase_percent(0, pct, "counting");
        }
    } else if line.contains("Compressing objects") {
        status.set_phase_active(0);
        if let Some(caps) = COMPRESSING_RE.captures(line) {
            let pct: u8 = caps[1].parse().unwrap_or(0);
            status.set_phase_percent(0, pct, "compressing");
        }
    } else if line.contains("Checking out files") || line.contains("Updating files") {
        status.set_phase_active(4);
        status.set_phase_percent(4, 50, "checkout");
    }
}

pub fn apply_lfs_progress(status: &mut CloneStatus, current: u64, total: u64, msg: &str) {
    status.set_phase_active(3);
    let pct = if total > 0 {
        ((current * 100) / total).min(100) as u8
    } else {
        0
    };
    status.set_phase_percent(3, pct, msg);
}
