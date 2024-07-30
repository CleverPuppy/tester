use clap::Parser;
use std::{
    io::Write,
    process::{Command, Stdio},
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
    times: u8,
    exec: String,
    exec_args: Vec<String>,
}

fn main() {
    let cli_args = Cli::parse();

    let mut program = Command::new(&cli_args.exec);
    program
        .args(&cli_args.exec_args)
        .stderr(Stdio::piped())
        .stdout(Stdio::piped());

    // println!("#tester: Begin to run {:#?}", program);
    // println!("#Cliargs : {:#?}", cli_args);

    let mut fail_times = 0;
    let mut run_times = 0;
    let mut total_scores = 0.0;
    for _ in 0..cli_args.times {
        let p_instance = program.spawn().expect("cmd failed to start");
        let p_ret = p_instance.wait_with_output().expect("Unable to run cmd");
        if !p_ret.status.success() {
            fail_times += 1;
        }
        if !cli_args.silent {
            let mut stdout = std::io::stdout();
            let mut stderr = std::io::stderr();
            stdout.write_all(&p_ret.stdout).unwrap();
            stderr.write_all(&p_ret.stderr).unwrap();
        }

        if cli_args.score {
            let std_out = String::from_utf8(p_ret.stdout).unwrap();
            let score: f64 = std_out.trim().parse().unwrap();
            total_scores += score;
        }
        run_times += 1;
    }

    if fail_times > 0 {
        println!("#tester finished. Failed {} / {}", fail_times, run_times);
    } else {
        println!("#tester finished. No failure in {} runs.", run_times);
        if cli_args.score {
            let avg_score = total_scores / f64::from(run_times);
            println!("#tester average score: {}.", avg_score);
        }
    }
}
