use clap::Parser;
use ctrlc;
use indicatif::{ProgressBar, ProgressState, ProgressStyle};
use std::{
    io::Write,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex,
    },
    thread::{self, sleep},
    time::Duration,
};

/// tester: A simple cli tool to help you run a test multi times
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Disable the execution's stdout and stderr
    #[arg(long, default_value_t = false)]
    silent: bool,
    /// Calculate the average score of every run
    #[arg(short, long, default_value_t = false)]
    score: bool,
    /// Show progress bar
    #[arg(long, default_value_t = false)]
    progress: bool,
    /// Number of times to run the commands
    #[arg(short = 'n')]
    times: u32,
    /// Number of threads
    #[arg(short = 'p', default_value_t = 1)]
    threads: u8,
    exec: String,
    exec_args: Vec<String>,
}

struct TesterInfo {
    fail_times: AtomicU32,
    run_times: AtomicU32,
    total_scores: Mutex<f64>,
    cli_args: Cli,
    ctrlc_signal: AtomicBool,
}

impl TesterInfo {
    fn print_summary(&self) {
        let fail_times = self.fail_times.load(Ordering::Relaxed);
        let run_times = self.run_times.load(Ordering::Relaxed);
        let total_score = *self.total_scores.lock().unwrap();
        if fail_times > 0 {
            println!("#tester finished. Failed {} / {}", fail_times, run_times);
            if self.cli_args.score {
                let avg_score = total_score / ((run_times - fail_times) as f64);
                println!("#tester average score(Ignore failed runs): {}.", avg_score);
            }
        } else {
            println!("#tester finished. No failure in {} runs.", run_times);
            if self.cli_args.score {
                let avg_score = total_score / run_times as f64;
                println!("#tester average score: {}.", avg_score);
            }
        }
    }

    fn do_test(&self, times: u32) {
        let mut run_times = 0;
        let mut fail_times = 0;
        let mut total_scores = 0.0;
        let mut program = Command::new(&self.cli_args.exec);
        program
            .args(&self.cli_args.exec_args)
            .stderr(Stdio::piped())
            .stdout(Stdio::piped());
        for _ in 0..times {
            if self.ctrlc_signaled() {
                break;
            }
            let p_instance = program.spawn().expect("cmd failed to start");
            let p_ret = p_instance.wait_with_output().expect("Unable to run cmd");

            if !self.cli_args.silent {
                let mut stdout = std::io::stdout();
                let mut stderr = std::io::stderr();
                stdout.write_all(&p_ret.stdout).unwrap();
                stderr.write_all(&p_ret.stderr).unwrap();
            }

            if !p_ret.status.success() {
                fail_times += 1;
            } else if self.cli_args.score {
                let std_out = String::from_utf8(p_ret.stdout).unwrap();
                let score: f64 = std_out.trim().parse().unwrap();
                total_scores += score;
            }

            run_times += 1;
            if self.cli_args.progress {
                self.append_run_times(1);
            }
        }
        self.append_result(fail_times, total_scores);
        if !self.cli_args.progress {
            self.append_run_times(run_times);
        }
    }

    fn append_result(&self, fail_times: u32, total_scores: f64) {
        self.fail_times.fetch_add(fail_times, Ordering::Relaxed);
        if self.cli_args.score {
            *self.total_scores.lock().unwrap() += total_scores;
        }
    }

    fn append_run_times(&self, run_times: u32) {
        self.run_times.fetch_add(run_times, Ordering::Relaxed);
    }

    fn get_progress(&self) -> u32 {
        return self.run_times.load(Ordering::Relaxed);
    }

    fn ctrlc_signaled(&self) -> bool {
        return self.ctrlc_signal.load(Ordering::Relaxed);
    }
}

fn main() {
    let test_info = Arc::new(TesterInfo {
        fail_times: AtomicU32::new(0),
        run_times: AtomicU32::new(0),
        total_scores: Mutex::new(0.0),
        ctrlc_signal: AtomicBool::new(false),
        cli_args: Cli::parse(),
    });

    let mut handles = vec![];
    let threads = std::cmp::max(test_info.cli_args.threads, 1);
    let times_per_thread = test_info.cli_args.times / threads as u32;
    let mut times_extra = test_info.cli_args.times % threads as u32;
    for _ in 0..threads {
        let mut times_this_thread = times_per_thread;
        if times_extra > 0 {
            times_this_thread += 1;
            times_extra -= 1;
        }

        if times_this_thread <= 0 {
            break;
        }

        let test_info_share = test_info.clone();
        let handle = thread::spawn(move || {
            test_info_share.do_test(times_per_thread);
        });
        handles.push(handle);
    }

    // handles ctrlc
    let test_info_share = test_info.clone();
    ctrlc::set_handler(move || {
        println!("Ctrl-c pressed. Terminating...");
        test_info_share.ctrlc_signal.store(true, Ordering::Relaxed);
    })
    .unwrap();

    if test_info.cli_args.progress {
        let total_progress = test_info.cli_args.times;
        let mut current_progress = test_info.get_progress();
        let progress_bar = ProgressBar::new(total_progress as u64);
        let progress_bar_template =
            "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} ({eta})";
        let eta_progress_fn = |state: &ProgressState, w: &mut dyn std::fmt::Write| {
            write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap()
        };
        let progress_bar_style = ProgressStyle::with_template(progress_bar_template)
            .unwrap()
            .with_key("eta", eta_progress_fn)
            .progress_chars("#>-");
        progress_bar.set_style(progress_bar_style);
        let update_duration = Duration::from_secs_f32(0.1);
        while current_progress < total_progress {
            if test_info.ctrlc_signaled() {
                break;
            }
            current_progress = test_info.get_progress();
            progress_bar.set_position(current_progress as u64);
            sleep(update_duration);
        }
        progress_bar.finish();
    }

    for handle in handles {
        handle.join().unwrap();
    }

    test_info.print_summary();
}
