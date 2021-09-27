use txdemo::cli::run;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    run(
        std::env::args_os(),
        std::io::stdin().lock(),
        std::io::stdout().lock(),
    )
}
