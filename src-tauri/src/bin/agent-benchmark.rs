fn main() {
    if let Err(error) = aiide_lib::run_benchmark_cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
