.PHONY: build release check test bench clippy fmt fmt-check clean run ci

build:
	cargo build

release:
	cargo build --release

check:
	cargo check

test:
	cargo test

bench:
	cargo bench --bench search_bench --bench evaluation_bench

clippy:
	cargo clippy -- -D warnings

fmt:
	cargo fmt

fmt-check:
	cargo fmt -- --check

clean:
	cargo clean

run:
	cargo run --release

ci: fmt-check clippy test bench
