use std::time::Instant;

#[derive(Debug, Clone)]
pub struct PhaseStatus {
    pub name: String,
    pub percent: u8,
    pub detail: String,
    pub done: bool,
}

#[derive(Debug, Clone)]
pub struct CloneStatus {
    pub strategy: String,
    pub url: String,
    pub dest: String,
    pub phases: [PhaseStatus; 6],
    pub current_phase: usize,
    pub speed: String,
    pub message: String,
    pub done: bool,
    pub success: bool,
    started: Instant,
}

impl CloneStatus {
    pub fn new(strategy: &str, url: &str, dest: &str) -> Self {
        Self {
            strategy: strategy.to_string(),
            url: url.to_string(),
            dest: dest.to_string(),
            phases: [
                phase("Negotiating refs"),
                phase("Receiving objects"),
                phase("Resolving deltas"),
                phase("LFS files"),
                phase("Checking out files"),
                phase("Post-processing"),
            ],
            current_phase: 0,
            speed: String::new(),
            message: "Starting...".to_string(),
            done: false,
            success: false,
            started: Instant::now(),
        }
    }

    pub fn elapsed_secs(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    pub fn set_phase_active(&mut self, index: usize) {
        self.current_phase = index;
        for (i, p) in self.phases.iter_mut().enumerate() {
            p.done = i < index;
        }
    }

    pub fn set_phase_percent(&mut self, index: usize, percent: u8, detail: &str) {
        if index < self.phases.len() {
            self.phases[index].percent = percent.min(100);
            self.phases[index].detail = detail.to_string();
            self.current_phase = index;
        }
    }

    pub fn finish_phase(&mut self, index: usize) {
        if index < self.phases.len() {
            self.phases[index].percent = 100;
            self.phases[index].done = true;
        }
    }

    pub fn set_message(&mut self, msg: &str) {
        self.message = msg.to_string();
    }

    pub fn set_speed(&mut self, speed: &str) {
        self.speed = speed.to_string();
    }

    pub fn mark_done(&mut self, success: bool) {
        self.done = true;
        self.success = success;
        for p in &mut self.phases {
            if !p.done {
                p.percent = if success { 100 } else { p.percent };
                p.done = success;
            }
        }
    }
}

fn phase(name: &str) -> PhaseStatus {
    PhaseStatus {
        name: name.to_string(),
        percent: 0,
        detail: String::new(),
        done: false,
    }
}
