use std::thread;
use std::time::{Duration, Instant};

use console::{style, Emoji};
use indicatif::{HumanDuration, MultiProgress, ProgressBar, ProgressStyle};
use rand::seq::SliceRandom;
use rand::Rng;

static PACKAGES: &[&str] = &[
    "fs-events",
    "my-awesome-module",
    "emoji-speaker",
    "wrap-ansi",
    "stream-browserify",
    "acorn-dynamic-import",
];

static COMMANDS: &[&str] = &[
    "cmake .",
    "make",
    "make clean",
    "gcc foo.c -o foo",
    "gcc bar.c -o bar",
    "./helper.sh rebuild-cache",
    "make all-clean",
    "make test",
];

static LOOKING_GLASS: Emoji<'_, '_> = Emoji("🔍  ", "");
static TRUCK: Emoji<'_, '_> = Emoji("🚚  ", "");
static CLIP: Emoji<'_, '_> = Emoji("🔗  ", "");
static PAPER: Emoji<'_, '_> = Emoji("📃  ", "");
static SPARKLE: Emoji<'_, '_> = Emoji("✨ ", ":-)");

pub fn main() {
    let mut rng = rand::thread_rng();
    let started = Instant::now();
    let spinner_style = ProgressStyle::with_template("{prefix:.bold.dim} {spinner} {wide_msg}")
        .unwrap()
        .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ");

    println!(
        "{} {}Resolving packages...",
        style("[1/4]").bold().dim(),
        LOOKING_GLASS
    );
    println!(
        "{} {}Fetching packages...",
        style("[2/4]").bold().dim(),
        TRUCK
    );

    println!(
        "{} {}Linking dependencies...",
        style("[3/4]").bold().dim(),
        CLIP
    );
    let deps = 1232;
    let pb = ProgressBar::new(deps);
    for _ in 0..deps {
        thread::sleep(Duration::from_millis(3));
        pb.inc(1);
    }
    pb.finish_and_clear();

    println!(
        "{} {}Building fresh packages...",
        style("[4/4]").bold().dim(),
        PAPER
    );
    let m = MultiProgress::new();
    let handles: Vec<_> = (0..4u32)
        .map(|i| {
            let count = rng.gen_range(30..80);
            let pb = m.add(ProgressBar::new(count));
            pb.set_style(spinner_style.clone());
            pb.set_prefix(format!("[{}/?]", i + 1));
            thread::spawn(move || {
                let mut rng = rand::thread_rng();
                let pkg = PACKAGES.choose(&mut rng).unwrap();
                for _ in 0..count {
                    let cmd = COMMANDS.choose(&mut rng).unwrap();
                    thread::sleep(Duration::from_millis(rng.gen_range(25..200)));
                    pb.set_message(format!("{pkg}: {cmd}"));
                    pb.inc(1);
                }
                pb.finish_with_message("waiting...");
            })
        })
        .collect();
    for h in handles {
        let _ = h.join();
    }
    m.clear().unwrap();

    println!("{} Done in {}", SPARKLE, HumanDuration(started.elapsed()));
}
