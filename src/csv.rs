use crate::core::{
    cmd, Account, ClientId, Command, TransactionId, TransactionMeta, UnsignedAssetCount,
};
use serde::{Deserialize, Serialize};
use std::convert::{TryFrom, TryInto};
use std::io;
use thiserror::Error;

/// A command record as found in input CSV files
///
/// It would be nice to avoid this intermediate representation and directly
/// use [core::Command] but the `csv` crate does not support support internally
/// tagged enum as rows (as of 2021-09-26).
/// (https://github.com/BurntSushi/rust-csv/issues/211#issuecomment-707620417).
#[derive(Debug, Serialize, Deserialize)]
struct CommandRecord {
    r#type: CommandType,
    client: ClientId,
    tx: TransactionId,
    amount: Option<UnsignedAssetCount>,
}

#[derive(Error, Debug, Copy, Clone, Eq, PartialEq)]
#[error("missing deposit amount")]
pub struct FromDepositRecordError;

impl TryFrom<CommandRecord> for cmd::Deposit {
    type Error = FromDepositRecordError;

    fn try_from(value: CommandRecord) -> Result<Self, Self::Error> {
        Ok(Self(TransactionMeta {
            id: value.tx,
            client: value.client,
            amount: value.amount.ok_or(FromDepositRecordError)?,
        }))
    }
}

#[derive(Error, Debug, Copy, Clone, Eq, PartialEq)]
#[error("missing withdrawal amount")]
pub struct FromWithdrawalRecordError;

impl TryFrom<CommandRecord> for cmd::Withdrawal {
    type Error = FromWithdrawalRecordError;

    fn try_from(value: CommandRecord) -> Result<Self, Self::Error> {
        Ok(Self(TransactionMeta {
            id: value.tx,
            client: value.client,
            amount: value.amount.ok_or(FromWithdrawalRecordError)?,
        }))
    }
}

impl From<CommandRecord> for cmd::Dispute {
    fn from(value: CommandRecord) -> Self {
        Self {
            client: value.client,
            tx: value.tx,
        }
    }
}

impl From<CommandRecord> for cmd::Resolve {
    fn from(value: CommandRecord) -> Self {
        Self {
            client: value.client,
            tx: value.tx,
        }
    }
}

impl From<CommandRecord> for cmd::Chargeback {
    fn from(value: CommandRecord) -> Self {
        Self {
            client: value.client,
            tx: value.tx,
        }
    }
}

#[derive(Error, Debug, Copy, Clone)]
pub enum FromCommandRecordError {
    #[error("invalid record for the type `deposit`")]
    Deposit(#[from] FromDepositRecordError),
    #[error("invalid record for the type `withdrawal`")]
    Withdrawal(#[from] FromWithdrawalRecordError),
}

impl TryFrom<CommandRecord> for Command {
    type Error = FromCommandRecordError;

    fn try_from(record: CommandRecord) -> Result<Self, Self::Error> {
        let cmd = match record.r#type {
            CommandType::Deposit => Self::Deposit(record.try_into()?),
            CommandType::Withdrawal => Self::Withdrawal(record.try_into()?),
            CommandType::Dispute => Self::Dispute(record.into()),
            CommandType::Resolve => Self::Resolve(record.into()),
            CommandType::Chargeback => Self::Chargeback(record.into()),
        };
        Ok(cmd)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CommandType {
    Deposit,
    Withdrawal,
    Dispute,
    Resolve,
    Chargeback,
}

/// A wrapper providing a higher level API to read transaction commands from a
/// CSV stream.
pub struct CsvCommandReader<R: io::Read> {
    inner: csv::Reader<R>,
}

impl<R: io::Read> CsvCommandReader<R> {
    pub fn from_reader(reader: R) -> Self {
        let inner = csv::ReaderBuilder::new()
            .trim(csv::Trim::All)
            .from_reader(reader);
        Self { inner }
    }

    /// Return an iterator for all the transaction commands.
    ///
    /// If a row contains a syntax error, it is returned to allow the app
    /// to display an error message. You can continue the iteration after the
    /// error without any issue.
    pub fn commands(&mut self) -> CsvCommandIter<R> {
        let inner = self.inner.deserialize::<CommandRecord>();
        CsvCommandIter { inner }
    }
}

pub struct CsvCommandIter<'r, R: io::Read + 'r> {
    inner: csv::DeserializeRecordsIter<'r, R, CommandRecord>,
}

/// A row from the input CSV
///
/// The `.record` field is either a valid command if the row was well-formed,
/// or an error object indicating the issue.
#[derive(Debug)]
pub struct CsvRow {
    pub start: csv::Position,
    pub end: csv::Position,
    pub record: Result<Command, CsvRowError>,
}

#[derive(Error, Debug)]
pub enum CsvRowError {
    /// Malformed CSV row
    /// Some example of possible reasons are uneven row length, invalid encoding
    /// malformed data.
    #[error("CSV error")]
    Csv(#[from] csv::Error),
    /// The row could be read as a valid CSV record but could not be validated
    /// as a well-formed command.
    /// The most common reason is a missing amount for transaction commands.
    #[error("failed to read the record as a valid command")]
    ValidationError(#[from] FromCommandRecordError),
}

impl<'r, R: io::Read + 'r> Iterator for CsvCommandIter<'r, R> {
    type Item = CsvRow;

    fn next(&mut self) -> Option<Self::Item> {
        let start = self.inner.reader().position().clone();
        let record = self.inner.next()?;
        let end = self.inner.reader().position().clone();
        let row = CsvRow {
            start,
            end,
            record: match record {
                Ok(record) => Command::try_from(record).map_err(CsvRowError::ValidationError),
                Err(err) => Err(CsvRowError::Csv(err)),
            },
        };
        Some(row)
    }
}

/// An output account record.
///
/// The `csv` crate can only serialize flat structs: this serves as a
/// temporary helper struct to serialize [core::Account] value.
#[derive(Debug, Serialize, Deserialize)]
struct AccountRecord {
    client: ClientId,
    available: UnsignedAssetCount,
    held: UnsignedAssetCount,
    total: UnsignedAssetCount,
    locked: bool,
}

impl TryFrom<Account> for AccountRecord {
    type Error = ();

    fn try_from(value: Account) -> Result<Self, Self::Error> {
        Ok(Self {
            client: value.client,
            available: value.balance.available(),
            held: value.balance.held(),
            total: value.balance.total(),
            locked: value.locked,
        })
    }
}

/// Helper struct to provide a higher-level API to output account states.
///
/// Note that it does not write the CSV header row automatically, you must call
/// `.write_headers` manually.
pub struct CsvAccountWriter<W: io::Write> {
    inner: csv::Writer<W>,
}

impl<W: io::Write> CsvAccountWriter<W> {
    pub fn from_writer(writer: W) -> Self {
        let inner = csv::WriterBuilder::new()
            .has_headers(false)
            .from_writer(writer);
        Self { inner }
    }

    /// Write a header line.
    ///
    /// This must be called explicitly to support empty collections.
    /// See <https://github.com/BurntSushi/rust-csv/issues/161>
    pub fn write_headers(&mut self) -> csv::Result<()> {
        self.inner.write_record(std::array::IntoIter::new([
            "client",
            "available",
            "held",
            "total",
            "locked",
        ]))
    }

    /// Write a single CSV row.
    pub fn write(&mut self, account: Account) -> csv::Result<()> {
        let record: AccountRecord = account
            .try_into()
            .expect("failed to compute account record");
        self.inner.serialize(record)
    }

    /// Consume the iterator and write one row per item.
    pub fn write_all<Iter: Iterator<Item = Account>>(&mut self, accounts: Iter) -> csv::Result<()> {
        for account in accounts {
            self.write(account)?;
        }
        Ok(())
    }

    /// Flush the inner writer.
    pub fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}
