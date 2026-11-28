//! Generate the conformance corpus and the Rust verdicts.
//!
//! Example:
//!
//! ```bash
//! cargo run -p swap-plan-check --bin gen_corpus -- \
//!   --plans 1200 --certs 600 \
//!   --corpus target/plan-check/corpus.txt \
//!   --expected target/plan-check/expected.txt
//! ```

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use swap_plan_check::corpus;

fn main() -> ExitCode {
    let mut plans = 1_200usize;
    let mut certs = 600usize;
    let mut corpus_path = PathBuf::from("target/plan-check/corpus.txt");
    let mut expected_path = PathBuf::from("target/plan-check/expected.txt");

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--plans" => {
                let Some(value) = args.next() else {
                    eprintln!("--plans needs a value");
                    return ExitCode::FAILURE;
                };
                let Ok(value) = value.parse() else {
                    eprintln!("--plans needs an integer");
                    return ExitCode::FAILURE;
                };
                plans = value;
            }
            "--certs" => {
                let Some(value) = args.next() else {
                    eprintln!("--certs needs a value");
                    return ExitCode::FAILURE;
                };
                let Ok(value) = value.parse() else {
                    eprintln!("--certs needs an integer");
                    return ExitCode::FAILURE;
                };
                certs = value;
            }
            "--corpus" => {
                let Some(value) = args.next() else {
                    eprintln!("--corpus needs a path");
                    return ExitCode::FAILURE;
                };
                corpus_path = PathBuf::from(value);
            }
            "--expected" => {
                let Some(value) = args.next() else {
                    eprintln!("--expected needs a path");
                    return ExitCode::FAILURE;
                };
                expected_path = PathBuf::from(value);
            }
            "--help" | "-h" => {
                println!(
                    "usage: gen_corpus [--plans N] [--certs N] [--corpus PATH] [--expected PATH]"
                );
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("unknown argument: {other}");
                return ExitCode::FAILURE;
            }
        }
    }

    let corpus = corpus::generate(plans, certs);
    if let Err(error) = corpus::write_corpus(&corpus, &corpus_path, &expected_path) {
        eprintln!("cannot write corpus: {error}");
        return ExitCode::FAILURE;
    }
    print!("{}", corpus::summary(&corpus));
    println!("corpus: {}", corpus_path.display());
    println!("expected: {}", expected_path.display());
    ExitCode::SUCCESS
}
