use clap::Parser;
use std::{
    io::Write,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
    thread,
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
    /// Number of times to run the commands
    #[arg(short = 'n')]
    times: u32,
    /// Number of threads
    #[arg(short = 'p')]
    threads: u8,
    exec: String,
    exec_args: Vec<String>,
}

struct TesterInfo {
    fail_times: AtomicU32,
    run_times: AtomicU32,
    total_scores: Mutex<f64>,
    cli_args: Cli,
}

impl TesterInfo {
    fn print_summary(&self) {
        let fail_times = self.fail_times.load(Ordering::Relaxed);
        let run_times = self.run_times.load(Ordering::Relaxed);
        let total_score = *self.total_scores.lock().unwrap();
        if fail_times > 0 {
            println!("#tester finished. Failed {} / {}", fail_times, run_times);
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
            let p_instance = program.spawn().expect("cmd failed to start");
            let p_ret = p_instance.wait_with_output().expect("Unable to run cmd");
            if !p_ret.status.success() {
                fail_times += 1;
            }
            if !self.cli_args.silent {
                let mut stdout = std::io::stdout();
                let mut stderr = std::io::stderr();
                stdout.write_all(&p_ret.stdout).unwrap();
                stderr.write_all(&p_ret.stderr).unwrap();
            }

            if self.cli_args.score {
                let std_out = String::from_utf8(p_ret.stdout).unwrap();
                let score: f64 = std_out.trim().parse().unwrap();
                total_scores += score;
            }
            run_times += 1;
        }
        self.append_result(run_times, fail_times, total_scores);
    }

    fn append_result(&self, run_times: u32, fail_times: u32, total_scores: f64) {
        self.run_times.fetch_add(run_times, Ordering::Relaxed);
        self.fail_times.fetch_add(fail_times, Ordering::Relaxed);
        if self.cli_args.score {
            *self.total_scores.lock().unwrap() += total_scores;
        }
    }
}

fn main() {
    let test_info = Arc::new(TesterInfo {
        fail_times: AtomicU32::new(0),
        run_times: AtomicU32::new(0),
        total_scores: Mutex::new(0.0),
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

    for handle in handles {
        handle.join().unwrap();
    }

    test_info.print_summary();
}
