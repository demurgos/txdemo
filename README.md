# TxDemo

_TxDemo_ is a small Rust CLI application showcasing a minimal transaction processing service.

In this model, clients can deposit and withdraw assets from their unique account. It is also possible to dispute
deposits and withdrawals (mark them as erroneous). While a transaction is disputed, the corresponding assets are held
until the dispute is settled with either a resolution (ignore the dispute) or a chargeback (cancel the disputed
transaction).

# Getting started

This is a regular Rust project. You can run it with:

```
cargo run -- [flags] [input]
```

The program takes a single optional arguments: the path to a CSV file containing
the commands to run. If the path is missing, the application reads from stdin.

The program prints the final state of all the accounts to the standard output.

Flags:
- `--sort`: Sort the output by client id.
- `--deny-withdrawal-dispute`: Prevent `dispute` commands on `withdrawal` transactions.

**Example**:

```
$ cat transactions.csv
type, client, tx, amount
deposit, 1, 1, 10.0
deposit, 1, 2, 4.0
dispute, 1, 2,
chargeback, 1, 2,
$ cargo run -- transactions.csv > accounts.csv
$ cat accounts.csv
client,available,held,total,locked
1,10.0000,0.0000,10.0000,true
```
s
# Introduction

Clients are automatically created with an empty account the first time they
are mentioned. Client accounts contain `available` and `held` assets.
`available` assets can be freely withdrawn, `held` assets are currently
dispute and can't be used until the dispute is settled. If the dispute is
settled with a chargeback (cancel the transaction), the account it locked forever
and any modification to its state is prevented (new transactions, disputes,
resolutions, or chargebacks).

To prevent abuses, both `available` and `held` client assets must always be
positive (or zero). This invariant is checked by Rust's type system.

# Commands

This section documents the five supported commands:

- `deposit`: Create a new transaction to increase the available assets of the account.
- `withdrawal`: Create a new transaction to decrease the available assets of the account.
- `dispute`: File a dispute againts a transaction, freezing its assets in the `held` state until the dispute is settled.
- `resolve`: Settle a dispute by cancelling the dispute: the assets are released back to the `available` state.
- `chargeback`: Settle a dispute by reverting the transaction. The account is locked.

## deposit

- **type**: `"deposit"`
- **client**: `ClientId`, the client performing the deposit
- **tx**: `TransactionId`, id of the transaction
- **amount**: `UnsignedAssetCount`, value to deposit, with up to 4 decimal digits

If the client account is not locked, increase its `available` assets by
the provided amount.

### Example

- Old state

  ```
  client, available, held, total, locked
       1,        10,    1,    11, false
  ```

- Commands

  ```
     type, client, tx, amount
  deposit,      1,  1,      3
  ```
  
- New state

  ```
  client, available, held, total, locked
       1,        13,    1,    14, false
  ```

## withdrawal

- **type**: `"withdrawal"`
- **client**: `ClientId`, the client performing the withdrawal
- **tx**: `TransactionId`, id of the transaction
- **amount**: `UnsignedAssetCount`, value to withdraw, with up to 4 decimal digits

If the client account is not locked and has sufficient availabl assets, decrease
its `available` assets by the provided amount.

### Example

- Old state

  ```
  client, available, held, total, locked
       1,        10,    1,    11, false
  ```

- Commands

  ```
        type, client, tx, amount
  withdrawal,      1,  1,      3
  ```

- New state

  ```
  client, available, held, total, locked
       1,         7,    1,     8, false
  ```

## dispute

- **type**: `"dispute"`
- **client**: `ClientId`, the client claiming that the transaction is erroneous
- **tx**: `TransactionId`, id of the disputed transaction
- **amount**: empty

If the transaction exist, the corresponding account is not locked and the
claimant client is the one who did the transaction, mark the transaction as
disputed.

A dispute can be filed for any passed transaction (there is no time limit)
as long as the account has enough `available` assets.

If the disputed transaction is a deposit, the situation is simple: move the
erroneous assets to the `held` state until the dispute is settled.

If the disputed transaction is a withdrawal, the situation is more complex as
a chargeback would mean refunding the client, and we want to prevent abuses.
This app provides two options, you can pick the preferred one with the
flag `--deny-withdrawal-dispute`:
- Default (no flag): Allow withdrawal disputes as long as the account still has
  at least the same amount of assets available. This means that an account
  with `4.0` remaining available assets can dispute a `3.0` withdrawal but not
  a `5.0` withdrawal. This strategy means that a refund will never pay out more
  than the available assets (so it limits the risk for the bank).
- `--deny-withdrawal-dispute`: Simply prevent any dispute regarding withdrawals.
  Safer for the bank as it is almost impossible to abuse, but it may hurt honest
  clients.

In all cases, the account is locked after the chargeback. It allows the bank
to further investigate the issue while the assets are still on the account.

### Example - Dispute deposit

- Old state

  ```
  client, available, held, total, locked
       1,        10,    1,    11, false
  ```

- Commands

  ```
        type, client, tx, amount
     deposit,      1,  1,      3
     dispute,      1,  1,
  ```

- New state

  ```
  client, available, held, total, locked
       1,        10,    4,    14, false
  ```

### Example - Dispute withdrawal

- Old state

  ```
  client, available, held, total, locked
       1,        10,    1,    11, false
  ```

- Commands

  ```
        type, client, tx, amount
  withdrawal,      1,  1,      3
     dispute,      1,  1,
  ```

- New state

  ```
  client, available, held, total, locked
       1,         4,    4,     8, false
  ```

## resolve

- **type**: `"resolve"`
- **client**: `ClientId`, the client claiming that the dispute is resolved
- **tx**: `TransactionId`, id of the disputed transaction
- **amount**: empty

Cancel a previous dispute and restore the corresponding held assets to the
`available` state.

Only the account owner can claim a dispute is resolved.

## chargeback

- **type**: `"chargeback"`
- **client**: `ClientId`, the client claiming that the disputed transaction should be cancelled
- **tx**: `TransactionId`, id of the disputed transaction
- **amount**: empty

Cancel the dispute transactions (refunding the account if needed).
The account is immediately locked following a chargeback, allowing the bank
to further investigate the situation. If it turns out that this was a fraud,
the account will still have the refunded assets so no abuse is possible
this way.

# Project management

Besides `cargo run`, the following commands are relevant to this project.
This repository is configured to run most checks in CI.

## Test

```
cargo test
```

## Test with coverage

On Linux, install [Tarpaulin](https://github.com/xd009642/tarpaulin) and run the
following command:

```
cargo tarpaulin --out html --output-dir ./target/debug/coverage/
```

You may also use [Grcov](https://github.com/mozilla/grcov) to measure code coverage.
It requires Nightly but is more acurate in my experience:

```
rustup run nightly cargo install grcov
rustup run nightly rustup component add llvm-tools-preview
rustup run nightly cargo build
rm -rf target/debug/profile/
mkdir -p target/debug/profile/
LLVM_PROFILE_FILE="target/debug/profile/%p-%m.profraw" RUSTFLAGS="-Zinstrument-coverage" rustup run nightly cargo test
rustup run nightly grcov ./target/debug/profile/ -s . --binary-path ./target/debug/ -t html --branch --ignore-not-existing -o ./target/debug/coverage/
```

The report will be in `./target/debug/coverage/`.


## Format code

Make sure you enabled the `rustmt` component on your toolchain, then run:

```
cargo fmt --all
```

To only check the code, use:

```
cargo fmt --all -- --check
```

## Lint

Make sure you enabled the `clippy` component on your toolchain, then run:

```
cargo clippy --all-targets --all-features -- -D warnings
```

## Audit

This project is checked with [cargo-audit](https://github.com/RustSec/rustsec/tree/main/cargo-audit).
You can run the check yourself with:

```
cargo audit
```

## Generate random test samples

```
cargo run --package txgenerator --release
```

The test samples will be located in `./generated`.

## Benchmark

First generate the random test samples with the command above, then run:

```
cargo bench
```

## Profile

You can use [Flamegraph](https://github.com/flamegraph-rs/flamegraph) to profile
the execution.

```
cargo flamegraph --bin txdemo -- ./generated/sample0/input.csv > /dev/null 2> /dev/null
```

# Correctness

Two of the most invariants maintained by this crate are:
- Both the `available` and `held` assets of an account must always be positive.
- There should be exactly 4 digits of precision, without precision loss.

These invariants are enforced by the type system so they can never be broken
if the code contains a business logic error.

This is solved by representing assets amount with a fixed-point decimal number.
There are some existing libraries to handle such cases, but they were missing
some guarantees I wanted so I wrote my own module. It lets me define the type
`FixedDecimal<u64, 4>`: a fixed-point decimal number representing a `u64` amount
of fractions each corresponding to `1-e4`. Using this type naturally prevents
negative asset counts.

An important aspect of my module is that it requires checked arithmetic and
prevents rounding. It means that parsing an input value of `0.12345` is rejected
because representing it would require rounding (and rounding requires more
information). Enforced checked arithmetic means that a business logic error
causing an underflow is impossible; for example subtracting more assets than
available will not cause the balance to get really high.

Apart from this, the code should be readable and commented enough to help spot
mistakes.

This project does not use any unsafe code itself, and relies on a small
number of established dependencies.

# Reliability

This project tries its best to avoid panics and handle errors gracefully.
In particular, it recovers from errors caused by invalid commands and continues
to handle valid commands.

This projects uses Github Actions as a CI service to check commits and prevent
regressions. It also uses `rustfmt` and `clippy` to keep the code readable and
prevent common mistakes.

The project is extensively tested and has a high code coverage ratio. Rust's
code coverage ecosystem is still immature but grcov reports at least 90%
of code coverage (98% for the module containing the business logic).

The tests are defined as directories in `./test-ressources`. Each test
contains an input file, an expected output file and an optional `flags.txt` to
run the program with extra flags. The program executed against the input file
and the actual output and errors saved (so they can be easily reviewed). The
test passes if the actual output exactly matches the expected output.
Note that the tests enforce the `--sort` flag for determinism.

# Performance

Profiling the code reveals that most of the time is spent deserializing the
CSV input. Only about 20% of the time is spent actually handling commands.

This means that better performance can be obtained by changing the input format
or the CSV parser, but the account service is performant enough.

There are no special tricks: the code is single threaded a fairly simple.
The memory requirements are proportional to the unique clients and unique
transactions ids. The time to run should grow linearly with the size of the
input.

# Security

The repo is configured to run security audits automatically. The number of
dependencies is small and they either established or verified by myself.
All the commits on this repository are signed and verified by GitHub to match
my GPG key.

# License

[AGPL-3.0-or-later](./LICENSE.md)
