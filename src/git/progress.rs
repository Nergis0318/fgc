use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use regex::Regex;
use std::sync::LazyLock;
use std::time::Instant;

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

pub struct CloneProgress {
    _multi: MultiProgress,
    steps: Vec<ProgressBar>,
    start: Instant,
    current_step: usize,
}

impl CloneProgress {
    pub fn new() -> Self {
        let multi = MultiProgress::new();
        let style = ProgressStyle::with_template(
            "[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
        )
        .unwrap()
        .progress_chars("█▓░");

        let step_style =
            ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] {msg}").unwrap();

        let steps = vec![
            multi.add(ProgressBar::new_spinner().with_style(step_style.clone())),
            multi.add(ProgressBar::new(100).with_style(style.clone())),
            multi.add(ProgressBar::new(100).with_style(style.clone())),
            multi.add(ProgressBar::new(100).with_style(style.clone())),
            multi.add(ProgressBar::new(100).with_style(style.clone())),
            multi.add(ProgressBar::new_spinner().with_style(step_style)),
        ];

        steps[0].set_message("[1/6] Negotiating refs with remote...");
        steps[0].enable_steady_tick(std::time::Duration::from_millis(100));

        Self {
            _multi: multi,
            steps,
            start: Instant::now(),
            current_step: 0,
        }
    }

    pub fn advance_to_receiving(&mut self) {
        self.steps[0].finish_and_clear();
        self.steps[1].set_message("[2/6] Receiving objects");
        self.current_step = 1;
    }

    pub fn advance_to_resolving(&mut self) {
        if self.current_step < 2 {
            self.steps[1].finish_and_clear();
            self.steps[2].set_message("[3/6] Resolving deltas");
            self.current_step = 2;
        }
    }

    pub fn advance_to_checkout(&mut self) {
        for i in 1..4 {
            self.steps[i].finish_and_clear();
        }
        self.steps[4].set_message("[5/6] Checking out files");
        self.steps[4].set_position(50);
        self.current_step = 4;
    }

    pub fn finish_checkout(&mut self) {
        self.steps[4].set_position(100);
        self.steps[4].finish_and_clear();
    }

    pub fn start_post_processing(&mut self) {
        self.steps[5].set_message("[6/6] Post-processing (config, hooks)");
        self.steps[5].enable_steady_tick(std::time::Duration::from_millis(100));
        self.current_step = 5;
    }

    pub fn finish_post_processing(&mut self) {
        self.steps[5].finish_and_clear();
    }

    pub fn set_lfs_progress(&mut self, current: u64, total: u64, msg: &str) {
        if self.current_step < 3 {
            self.advance_to_resolving();
        }
        if self.steps[3].length().unwrap_or(0) == 0 && total > 0 {
            self.steps[3].set_length(total);
            self.steps[3].set_message("[4/6] LFS files");
        }
        self.steps[3].set_position(current.min(total));
        if !msg.is_empty() {
            self.steps[3].set_message(format!("[4/6] LFS files - {msg}"));
        }
    }

    pub fn finish_lfs(&mut self) {
        let len = self.steps[3].length().unwrap_or(0);
        if len > 0 {
            self.steps[3].set_position(len);
        }
        self.steps[3].finish_and_clear();
    }

    pub fn process_line(&mut self, line: &str) {
        if line.contains("Receiving objects") {
            self.advance_to_receiving();
            if let Some(caps) = RECEIVING_RE.captures(line) {
                let pct: u64 = caps[1].parse().unwrap_or(0);
                let speed = format!("{} {}/s", &caps[4], &caps[5]);
                self.steps[1].set_position(pct);
                self.steps[1].set_message(format!("[2/6] Receiving objects ({speed})"));
            }
        } else if line.contains("Resolving deltas") {
            self.advance_to_resolving();
            if let Some(caps) = RESOLVING_RE.captures(line) {
                let pct: u64 = caps[1].parse().unwrap_or(0);
                self.steps[2].set_position(pct);
            }
        } else if line.contains("Counting objects") {
            if let Some(caps) = COUNTING_RE.captures(line) {
                let pct: u64 = caps[1].parse().unwrap_or(0);
                self.steps[0].set_message(format!("[1/6] Counting objects ({pct}%)"));
            }
        } else if line.contains("Compressing objects") {
            if let Some(caps) = COMPRESSING_RE.captures(line) {
                let pct: u64 = caps[1].parse().unwrap_or(0);
                self.steps[0].set_message(format!("[1/6] Compressing objects ({pct}%)"));
            }
        } else if line.contains("Checking out files") || line.contains("Updating files") {
            self.advance_to_checkout();
        }
    }

    pub fn elapsed(&self) -> std::time::Duration {
        self.start.elapsed()
    }

    pub fn print_summary(&self, dest: &str, strategy: &str) {
        let elapsed = self.elapsed();
        let mins = elapsed.as_secs() / 60;
        let secs = elapsed.as_secs() % 60;

        println!();
        println!("Total time: {mins}m {secs:02}s  |  Strategy: {strategy}  |  Destination: {dest}");
    }
}

impl Default for CloneProgress {
    fn default() -> Self {
        Self::new()
    }
}
