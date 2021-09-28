use exitcode::ExitCode;
use txdemo::cli::run;

fn main() {
    let code: ExitCode = run(
        std::env::args_os(),
        std::io::stdin().lock(),
        std::io::stdout().lock(),
        std::io::stderr(),
    );
    std::process::exit(code);
}
