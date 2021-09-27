use crate::account::mem::MemAccountService;
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
}

pub fn run<Args, Arg, Stdin, Stdout>(
    args: Args,
    stdin: Stdin,
    stdout: Stdout,
) -> Result<(), Box<dyn std::error::Error>>
where
    Args: IntoIterator<Item = Arg>,
    Arg: Into<OsString> + Clone,
    Stdin: io::Read,
    Stdout: io::Write,
{
    let args = CliArgs::try_parse_from(args)?;
    let sort = args.sort;
    return match args.input.as_deref() {
        None => with_io(sort, stdin, stdout),
        Some(file) => {
            let file = File::open(file).expect("FailedToOpenInputFile");
            with_io(sort, file, stdout)
        }
    };

    fn with_io<Input: io::Read, Output: std::io::Write>(
        sort: bool,
        input: Input,
        output: Output,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut csv_reader = CsvCommandReader::from_reader(input);
        let mut csv_writer = CsvAccountWriter::from_writer(output);
        let mut account_service = MemAccountService::new();
        for cmd in csv_reader.commands() {
            let cmd = cmd?;
            account_service.submit(cmd)?;
            // dbg!(cmd);
        }
        let accounts = account_service.get_all_accounts();
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

        let args = vec!["txdemo", "--sort"];
        let stdio = File::open(input_path).expect("FailedToOpenInputFile");
        let stdout = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(actual_path.as_path())
            .expect("FailedToOpenActualFile");

        run(args, stdio, stdout).expect("runShouldSucceed");

        let actual = fs::read_to_string(actual_path).expect("FailedToReadActualFile");
        let expected = fs::read_to_string(expected_path).expect("FailedToReadExpectedFile");

        assert_eq!(actual, expected);
    }
}