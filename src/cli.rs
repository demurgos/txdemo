use crate::account::mem::{MemAccountService, WithdrawalDisputePolicy};
use crate::core::{Account, ClientId};
use crate::csv::{CsvAccountWriter, CsvCommandReader};
use clap::Clap;
use std::ffi::OsString;
use std::fs::File;
use std::io;
use std::path::PathBuf;

/// Execute a stream of commands against an in-memory account service.
#[derive(Debug, Clap)]
pub struct CliArgs {
    /// Input CSV file. See test assignment for format documentation.
    ///
    /// If you do not provide any file, it will read the input from stdin.
    input: Option<PathBuf>,
    /// Sort output accounts by client id (default: false)
    #[clap(long)]
    sort: bool,
    /// Deny all disputes related to withdrawals (default: allow if the account has more available
    /// assets than the disputed amount).
    #[clap(long)]
    deny_withdrawal_dispute: bool,
}

pub fn run<Args, Arg, Stdin, Stdout, Stderr>(
    args: Args,
    stdin: Stdin,
    stdout: Stdout,
    stderr: Stderr,
) -> Result<(), Box<dyn std::error::Error>>
where
    Args: IntoIterator<Item = Arg>,
    Arg: Into<OsString> + Clone,
    Stdin: io::Read,
    Stdout: io::Write,
    Stderr: io::Write,
{
    let args = CliArgs::try_parse_from(args)?;
    let sort = args.sort;
    let withdrawal_dispute_policy = if args.deny_withdrawal_dispute {
        WithdrawalDisputePolicy::Deny
    } else {
        WithdrawalDisputePolicy::IfMoreAvailableThanDisputed
    };
    return match args.input.as_deref() {
        None => with_io(sort, withdrawal_dispute_policy, stdin, stdout, stderr),
        Some(file) => {
            let file = File::open(file).expect("FailedToOpenInputFile");
            with_io(sort, withdrawal_dispute_policy, file, stdout, stderr)
        }
    };

    fn with_io<Input: io::Read, Output: io::Write, ErrOutput: io::Write>(
        sort: bool,
        withdrawal_dispute_policy: WithdrawalDisputePolicy,
        input: Input,
        output: Output,
        mut err_output: ErrOutput,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut csv_reader = CsvCommandReader::from_reader(input);
        let mut csv_writer = CsvAccountWriter::from_writer(output);
        let mut account_service = MemAccountService::new(withdrawal_dispute_policy);
        for row in csv_reader.commands() {
            let cmd = match row.record {
                Ok(cmd) => cmd,
                Err(e) => {
                    print_error(e, &row.start, &mut err_output);
                    continue;
                }
            };
            match account_service.submit(cmd) {
                Ok(()) => {}
                Err(e) => {
                    print_error(e, &row.start, &mut err_output);
                }
            };
        }
        let accounts = account_service.get_all_accounts();
        csv_writer.write_headers()?;
        if sort {
            let mut accounts: Vec<Account> = accounts.collect();
            accounts.sort_by(|left, right| ClientId::cmp(&left.client, &right.client));
            csv_writer.write_all(accounts.into_iter())?;
        } else {
            csv_writer.write_all(accounts)?;
        }
        csv_writer.flush()?;
        Ok(())
    }

    fn print_error<E: std::error::Error + 'static, ErrOutput: io::Write>(
        error: E,
        pos: &csv::Position,
        err_output: &mut ErrOutput,
    ) {
        writeln!(
            err_output,
            "Command #{} (line {}) failed:",
            pos.record(),
            pos.line(),
        )
        .expect("failed to log error");
        let mut e: &(dyn std::error::Error + 'static) = &error;
        loop {
            writeln!(err_output, "- {}", e).expect("failed to log error");
            if let Some(cause) = e.source() {
                e = cause;
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::cli::run;
    use std::fs;
    use std::fs::File;
    use std::path::PathBuf;
    use test_generator::test_resources;

    #[test_resources("./test-resources/*/")]
    fn test_app(path: &str) {
        let test_item_dir = PathBuf::from(path);
        let input_path = test_item_dir.join("input.csv");
        let expected_path = test_item_dir.join("expected.csv");
        let actual_path = test_item_dir.join("actual.csv");
        let errors_path = test_item_dir.join("errors.log");
        let flags_path = test_item_dir.join("flags.txt");

        let extra_flags = match fs::read_to_string(flags_path) {
            Ok(extra_flags) => extra_flags,
            Err(e) => match e.kind() {
                std::io::ErrorKind::NotFound => String::new(),
                _ => panic!("Failed to read flags: {}", e),
            },
        };

        let mut args = vec!["txdemo", "--sort"];
        args.extend(
            extra_flags
                .split('\n')
                .map(str::trim)
                .filter(|f| !f.is_empty()),
        );
        let stdio = File::open(input_path).expect("FailedToOpenInputFile");
        let stdout = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(actual_path.as_path())
            .expect("FailedToOpenActualFile");
        let stderr = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(errors_path.as_path())
            .expect("FailedToOpenErrorsFile");

        run(args, stdio, stdout, stderr).expect("runShouldSucceed");

        let actual = fs::read_to_string(actual_path).expect("FailedToReadActualFile");
        let expected = fs::read_to_string(expected_path).expect("FailedToReadExpectedFile");

        assert_eq!(actual, expected);
    }
}
