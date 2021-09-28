use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::fs;
use std::fs::File;
use std::path::PathBuf;
use txdemo::cli::run;

fn criterion_benchmark(c: &mut Criterion) {
    let generated_dir = PathBuf::from("./generated");
    let mut bench_items = Vec::<BenchItem>::new();
    for sample_dir in generated_dir
        .read_dir()
        .expect("failed to read ./generated dir")
    {
        let sample_dir = sample_dir.expect("failed to read dir entry");
        if !sample_dir.file_type().unwrap().is_dir() {
            continue;
        }
        let sample_dir = sample_dir.path();
        let input = sample_dir.join("input.csv");
        let output = sample_dir.join("actual.csv");
        bench_items.push(BenchItem { input, output });
    }

    if bench_items.is_empty() {
        panic!("No benchmark samples found in the `./generated` dir. Try running `cargo run --package txgenerator`");
    }

    let bench_items = bench_items.as_slice();

    c.bench_function("generated", |b| b.iter(|| run_all(bench_items)));
}

struct BenchItem {
    input: PathBuf,
    output: PathBuf,
}

impl BenchItem {
    fn run(&self) {
        let input = File::open(self.input.as_path()).expect("FailedToOpenInputFile");
        let output = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(self.output.as_path())
            .expect("FailedToOpenOutputFile");
        black_box(run(vec!["txdemo"], input, output, DevNull));
    }
}

fn run_all(items: &[BenchItem]) {
    for bench_item in items.iter() {
        bench_item.run()
    }
}

/// Fake `/dev/null` output
struct DevNull;

impl std::io::Write for DevNull {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
