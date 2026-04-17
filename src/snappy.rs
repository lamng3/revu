use anyhow::Result;
use rand::seq::SliceRandom;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub struct Snappy {
    pub affirmations: Vec<String>,
    pub pet_messages: Vec<String>,
    pub current_message: Option<String>,
    pub next_message_at: Instant,
    pub duck_frame: usize,
    pub next_anim_at: Instant,
    pub wave_phase: usize,
}

const DEFAULT_AFFIRMATIONS: &[&str] = &[
    "you got this!",
    "nice commit!",
    "that diff is lookin' clean.",
    "snip snap, ship it!",
    "review like you mean it.",
    "careful reader, sharp mind.",
    "small PRs, big wins.",
    "one hunk at a time.",
    "you're crushing it!",
    "bill up!",
    "delete more than you add.",
    "future-you says thanks.",
    "tests or it didn't happen.",
    "a rename a day keeps the tech debt away.",
    "green is good. red is honest.",
    "merge-base says hi.",
    "fewer abstractions, more clarity.",
    "simple > clever.",
    "typo? i got your back.",
    "ship it, little duck.",
];

const DEFAULT_PET_MESSAGES: &[&str] = &[
    "*claw click* thank you.",
    "antennae tingling!",
    "i snap in your honor.",
    "mmm, the carapace spot.",
    "i feel seen.",
    "softest pincers, reporting.",
    "*burble* noted. forever.",
    "warm tide, warm heart.",
    "morale: max. claws: wiggly.",
    "i do not quack. *clicks*",
    "small lobster, big love.",
    "pet accepted. thank you.",
    "you. me. tide pool. later.",
    "*happy snap*",
    "one more? ok, one more.",
];

pub fn affirmations_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".revu").join("affirmations.txt"))
}

pub fn ensure_affirmations_file() -> Result<()> {
    let Some(p) = affirmations_path() else { return Ok(()) };
    if let Some(parent) = p.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }
    if !p.exists() {
        let body = DEFAULT_AFFIRMATIONS.join("\n") + "\n";
        fs::write(&p, body)?;
    }
    Ok(())
}

pub fn load_affirmations() -> Vec<String> {
    if let Some(p) = affirmations_path() {
        if let Ok(data) = fs::read_to_string(&p) {
            let v: Vec<String> = data
                .lines()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty() && !s.starts_with('#'))
                .collect();
            if !v.is_empty() {
                return v;
            }
        }
    }
    DEFAULT_AFFIRMATIONS.iter().map(|s| s.to_string()).collect()
}

impl Snappy {
    pub fn new() -> Self {
        let _ = ensure_affirmations_file();
        Self {
            affirmations: load_affirmations(),
            pet_messages: DEFAULT_PET_MESSAGES.iter().map(|s| s.to_string()).collect(),
            current_message: Some("Snappy: you got this.".to_string()),
            next_message_at: Instant::now() + Duration::from_secs(45),
            duck_frame: 0,
            next_anim_at: Instant::now() + Duration::from_millis(140),
            wave_phase: 0,
        }
    }

    pub fn tick(&mut self) -> bool {
        let now = Instant::now();
        let mut changed = false;
        if now >= self.next_anim_at {
            self.duck_frame = self.duck_frame.wrapping_add(1);
            self.wave_phase = (self.wave_phase + 1) % 6;
            self.next_anim_at = now + Duration::from_millis(140);
            changed = true;
        }
        if now >= self.next_message_at {
            if let Some(msg) = self.affirmations.choose(&mut rand::thread_rng()).cloned() {
                self.current_message = Some(format!("Snappy: {msg}"));
            }
            let d = 45 + (rand::random::<u64>() % 76); // 45-120s
            self.next_message_at = now + Duration::from_secs(d);
            changed = true;
        }
        changed
    }

    pub fn pet(&mut self) -> String {
        let msg = self
            .pet_messages
            .choose(&mut rand::thread_rng())
            .cloned()
            .unwrap_or_else(|| "thank you. morale restored.".to_string());
        self.current_message = Some(format!("Snappy: {msg}"));
        self.duck_frame = self.duck_frame.wrapping_add(6);
        self.wave_phase = (self.wave_phase + 1) % 4;
        self.next_anim_at = Instant::now() + Duration::from_millis(160);
        self.next_message_at = Instant::now() + Duration::from_secs(45);
        msg
    }
}
