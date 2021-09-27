use crate::core::{
    cmd, Account, ClientId, Command, SignedCurrencyAmount, TransactionId, TransactionMeta,
    UnsignedCurrencyAmount,
};
use serde::{Deserialize, Serialize};
use std::convert::{TryFrom, TryInto};
use std::io;

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
    amount: Option<UnsignedCurrencyAmount>,
}

impl TryFrom<CommandRecord> for cmd::Deposit {
    type Error = ();

    fn try_from(value: CommandRecord) -> Result<Self, Self::Error> {
        Ok(Self(TransactionMeta {
            id: value.tx,
            client: value.client,
            amount: value.amount.ok_or(())?,
        }))
    }
}

impl TryFrom<CommandRecord> for cmd::Withdrawal {
    type Error = ();

    fn try_from(value: CommandRecord) -> Result<Self, Self::Error> {
        Ok(Self(TransactionMeta {
            id: value.tx,
            client: value.client,
            amount: value.amount.ok_or(())?,
        }))
    }
}

impl TryFrom<CommandRecord> for cmd::Dispute {
    type Error = ();

    fn try_from(value: CommandRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            client: value.client,
            tx: value.tx,
        })
    }
}

impl TryFrom<CommandRecord> for Command {
    type Error = ();

    fn try_from(record: CommandRecord) -> Result<Self, Self::Error> {
        let cmd = match record.r#type {
            CommandType::Deposit => Self::Deposit(record.try_into()?),
            CommandType::Withdrawal => Self::Withdrawal(record.try_into()?),
            CommandType::Dispute => Self::Dispute(record.try_into()?),
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
}

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

    pub fn commands(&mut self) -> CsvCommandIter<R> {
        let inner = self.inner.deserialize::<CommandRecord>();
        CsvCommandIter { inner }
    }
}

pub struct CsvCommandIter<'r, R: io::Read + 'r> {
    inner: csv::DeserializeRecordsIter<'r, R, CommandRecord>,
}

impl<'r, R: io::Read + 'r> Iterator for CsvCommandIter<'r, R> {
    type Item = csv::Result<Command>;

    fn next(&mut self) -> Option<Self::Item> {
        let record = self.inner.next()?;
        Some(match record {
            Ok(record) => Ok(Command::try_from(record).unwrap()),
            Err(e) => Err(e),
        })
    }
}

/// An output account record.
///
/// The `csv` crate can only serialize flat structs: this serves as a
/// temporary helper struct to serialize [core::Account] value.
#[derive(Debug, Serialize, Deserialize)]
struct AccountRecord {
    client: ClientId,
    available: SignedCurrencyAmount,
    held: SignedCurrencyAmount,
    total: SignedCurrencyAmount,
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
            locked: false,
        })
    }
}

pub struct CsvAccountWriter<W: io::Write> {
    inner: csv::Writer<W>,
}

impl<W: io::Write> CsvAccountWriter<W> {
    pub fn from_writer(writer: W) -> Self {
        let inner = csv::Writer::from_writer(writer);
        Self { inner }
    }

    pub fn write(&mut self, account: Account) -> csv::Result<()> {
        let record: AccountRecord = account
            .try_into()
            .expect("failed to compute account record");
        self.inner.serialize(record)
    }

    pub fn write_all<Iter: Iterator<Item = Account>>(&mut self, accounts: Iter) -> csv::Result<()> {
        for account in accounts {
            self.write(account)?;
        }
        Ok(())
    }

    pub fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}
