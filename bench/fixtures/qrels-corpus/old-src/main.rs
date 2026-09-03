use std::process;

fn main() {
    if let Err(e) = omengrep::cli::run() {
        eprintln!("Error: {e:#}");
        process::exit(omengrep::types::EXIT_ERROR);
    }
}
