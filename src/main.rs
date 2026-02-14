use std::process;

fn main() {
    if let Err(e) = hhg::cli::run() {
        eprintln!("Error: {e:#}");
        process::exit(hhg::types::EXIT_ERROR);
    }
}
