use rand::SeedableRng;
use rand_distr::Exp;
use rand_pcg::Lcg128Xsl64;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use txdemo::core::fixed_decimal::FixedDecimal;
use txdemo::core::{cmd, Transaction, UnsignedAssetCount};
use txdemo::core::{ClientId, Command, TransactionId, TransactionMeta};

// Number of commands to generate
const CMD_COUNT: u32 = 1100000;
// Max number of clients
const CLIENT_COUNT: u16 = 100;
// Max number of transactions
const TX_COUNT: u32 = 1000000;
// Probability that the transaction id will be picked sequentially
const SEQUENTIAL_TX_ID_PROBA: f64 = 0.95;
// Probability that a transaction is a deposit (as opposed to a withdrawal)
const DEPOSIT_PROBA: f64 = 0.55;
// Average amount for deposits and withdrawals in fractions (100.0000)
const AVERAGE_AMOUNT_FRACS: u64 = 1000000;
// Probability that a command is a transaction (deposit or withdrawal)
const TRANSACTION_PROBA: f64 = 0.95;
// Probability that the transaction may be marked as a target for future disputes.
const ADD_TO_DISPUTABLE_PROBA: f64 = 0.1;
// When the command is not a transaction, probability that it opens a dispute (instead of settling one)
const NEW_DISPUTE_PROBA: f64 = 0.5;
// Probability that a dispute will be settled with a `resolve`.
const SETTLE_WITH_RESOLVE: f64 = 0.8;
// Probability that commands related to disputes are picked randomly
const RAND_DISPUTE_ARGS: f64 = 0.03;
// Probability that the client properly handles disputed transaction state changes (disputed <-> non-disputed)
const PROBA_DISPUTE_CHANGE_STATE: f64 = 0.95;

fn main() {
    let samples = ["sample0", "sample1", "sample2"];

    let generated_dir = PathBuf::from("./generated");

    samples.par_iter().for_each(|sample| {
        eprintln!("Generating sample {:?}...", sample);
        let seed: [u8; 32] = Sha256::digest(sample.as_bytes()).into();
        let rng = Lcg128Xsl64::from_seed(seed);
        let sample_path = generated_dir.join(sample);
        fs::create_dir_all(sample_path.as_path()).unwrap();
        let commands_path = sample_path.join("input.csv");
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(commands_path.as_path());
        let mut file = match file {
            Ok(file) => file,
            Err(_) => panic!("Failed to open file {}", commands_path.display()),
        };
        let generator = CommandGenerator::new(rng);
        let mut deposit_count: usize = 0;
        let mut withdrawal_count: usize = 0;
        let mut dispute_count: usize = 0;
        let mut resolve_count: usize = 0;
        let mut chargeback_count: usize = 0;
        writeln!(file, "type, client, tx, amount").unwrap();
        for command in generator.take(CMD_COUNT as usize) {
            let write_res = match command {
                Command::Deposit(cmd::Deposit(tx)) => {
                    deposit_count += 1;
                    writeln!(file, "deposit, {}, {}, {}", tx.client, tx.id, tx.amount)
                }
                Command::Withdrawal(cmd::Withdrawal(tx)) => {
                    withdrawal_count += 1;
                    writeln!(file, "withdrawal, {}, {}, {}", tx.client, tx.id, tx.amount)
                }
                Command::Dispute(cmd) => {
                    dispute_count += 1;
                    writeln!(file, "dispute, {}, {},", cmd.client, cmd.client)
                }
                Command::Resolve(cmd) => {
                    resolve_count += 1;
                    writeln!(file, "resolve, {}, {},", cmd.client, cmd.client)
                }
                Command::Chargeback(cmd) => {
                    chargeback_count += 1;
                    writeln!(file, "chargeback, {}, {},", cmd.client, cmd.client)
                }
            };
            write_res.unwrap();
        }
        file.flush().unwrap();
        eprintln!(
            "{:?} Done (Tot: {}. dep: {}, wd: {}, disp: {}, res: {}, cb: {})",
            sample,
            CMD_COUNT,
            deposit_count,
            withdrawal_count,
            dispute_count,
            resolve_count,
            chargeback_count
        );
    });
}

struct CommandGenerator<Rng: rand::Rng> {
    rng: Rng,
    next_tx: u32,
    deposit_amount_distr: Exp<f64>,
    disputable_tx: Vec<(ClientId, TransactionId)>,
    open_disputes: Vec<(ClientId, TransactionId)>,
}

impl<Rng: rand::Rng> CommandGenerator<Rng> {
    pub fn new(rng: Rng) -> Self {
        // lambda = 1 / mean for the exponential distribution
        let deposit_amount_distr = Exp::new(1f64 / (AVERAGE_AMOUNT_FRACS as f64)).unwrap();
        Self {
            rng,
            next_tx: 0,
            deposit_amount_distr,
            disputable_tx: Vec::new(),
            open_disputes: Vec::new(),
        }
    }

    fn gen_tx(&mut self) -> Transaction {
        let client = ClientId::new(self.rng.gen_range(0..CLIENT_COUNT));
        let id = if self.rng.gen_bool(SEQUENTIAL_TX_ID_PROBA) {
            let tx_id = TransactionId::new(self.next_tx);
            self.next_tx += 1;
            tx_id
        } else {
            TransactionId::new(self.rng.gen_range(0..TX_COUNT))
        };
        if self.rng.gen_bool(ADD_TO_DISPUTABLE_PROBA) {
            self.disputable_tx.push((client, id));
        }
        // Use exponential distribution for deposits and uniform for withdrawals
        // They have the same average here, but overall the difference tends to grow
        if self.rng.gen_bool(DEPOSIT_PROBA) {
            let amount = 1.0 + self.rng.sample(&self.deposit_amount_distr).round();
            let amount = if ((amount as u64) as f64) == amount {
                amount as u64
            } else {
                0
            };
            let amount = UnsignedAssetCount::new(FixedDecimal::from_fractions(amount));
            Transaction::Deposit(TransactionMeta { id, client, amount })
        } else {
            let amount = self.rng.gen_range(0..=2000000);
            let amount = UnsignedAssetCount::new(FixedDecimal::from_fractions(amount));
            Transaction::Withdrawal(TransactionMeta { id, client, amount })
        }
    }

    fn gen_dispute(&mut self) -> cmd::Dispute {
        let base = pick_dispute(&mut self.rng, &mut self.disputable_tx);
        let (client, tx) = self.derive_dispute(base);
        if self.rng.gen_bool(PROBA_DISPUTE_CHANGE_STATE) && Some((client, tx)) == base {
            self.open_disputes.push((client, tx));
        }
        cmd::Dispute { client, tx }
    }

    fn gen_resolve(&mut self) -> cmd::Resolve {
        let base = pick_dispute(&mut self.rng, &mut self.open_disputes);
        let (client, tx) = self.derive_dispute(base);
        if self.rng.gen_bool(PROBA_DISPUTE_CHANGE_STATE) && Some((client, tx)) == base {
            self.disputable_tx.push((client, tx));
        }
        cmd::Resolve { client, tx }
    }

    fn gen_chargeback(&mut self) -> cmd::Chargeback {
        let base = pick_dispute(&mut self.rng, &mut self.open_disputes);
        let (client, tx) = self.derive_dispute(base);
        if self.rng.gen_bool(PROBA_DISPUTE_CHANGE_STATE) && Some((client, tx)) == base {
            self.disputable_tx.push((client, tx));
        }
        cmd::Chargeback { client, tx }
    }

    fn derive_dispute(
        &mut self,
        base: Option<(ClientId, TransactionId)>,
    ) -> (ClientId, TransactionId) {
        match (
            base,
            self.rng.gen_bool(RAND_DISPUTE_ARGS),
            self.rng.gen_bool(RAND_DISPUTE_ARGS),
        ) {
            (Some(base), false, false) => base,
            (Some((_, tx)), true, false) => {
                let client = ClientId::new(self.rng.gen_range(0..CLIENT_COUNT));
                (client, tx)
            }
            (Some((client, _)), false, true) => {
                let tx = TransactionId::new(self.rng.gen_range(0..TX_COUNT));
                (client, tx)
            }
            _ => {
                let client = ClientId::new(self.rng.gen_range(0..CLIENT_COUNT));
                let tx = TransactionId::new(self.rng.gen_range(0..TX_COUNT));
                (client, tx)
            }
        }
    }
}

fn pick_dispute<Rng: rand::Rng>(
    rng: &mut Rng,
    from: &mut Vec<(ClientId, TransactionId)>,
) -> Option<(ClientId, TransactionId)> {
    if from.is_empty() {
        return None;
    }
    let idx = rng.gen_range(0..from.len());
    let dispute = from[idx];
    if !rng.gen_bool(ADD_TO_DISPUTABLE_PROBA) {
        let last = from.len() - 1;
        from.swap(idx, last);
        from.pop();
    }
    Some(dispute)
}

impl<Rng: rand::Rng> Iterator for CommandGenerator<Rng> {
    type Item = Command;

    fn next(&mut self) -> Option<Self::Item> {
        let cmd = if self.rng.gen_bool(TRANSACTION_PROBA) {
            match self.gen_tx() {
                Transaction::Deposit(tx) => Command::Deposit(cmd::Deposit(tx)),
                Transaction::Withdrawal(tx) => Command::Withdrawal(cmd::Withdrawal(tx)),
            }
        } else if self.rng.gen_bool(NEW_DISPUTE_PROBA) {
            Command::Dispute(self.gen_dispute())
        } else if self.rng.gen_bool(SETTLE_WITH_RESOLVE) {
            Command::Resolve(self.gen_resolve())
        } else {
            Command::Chargeback(self.gen_chargeback())
        };
        Some(cmd)
    }
}
