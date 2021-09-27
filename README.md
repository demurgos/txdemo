# TxDemo

_TxDemo_ is a small Rust CLI application showcasing a minimal transaction processing service.

In this model, clients can deposit and withdraw assets from their unique account. It is also possible to dispute
deposits and withdrawals (mark them as erroneous). While a transaction is disputed, the corresponding assets are held
until the dispute is settled with either a resolution (ignore the dispute) or a chargeback (cancel the disputed
transaction).

## Getting started

This is a regular project. You can run it with:

```
cargo run -- [flags] [input]
```

The program takes a single optional arguments: the path to a CSV file containing the transaction commands. If the path
is missing, the application reads stdin.

Flags:
- `--sort`: Sort the output by clien id.

It prints the final state of all the accounts.

**Example**:

```
$ cat transactions.csv
$ cargo run -- transactions.csv > accounts.csv
$ cat accounts.csv
```

## Commands

### deposit

### withdrawal

### dispute

### resolve

### chargeback

# License


