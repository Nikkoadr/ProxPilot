.PHONY: all build run dev clean check

PORT ?= 8080

all: build run

check:
	cargo check

build:
	cargo build --release

run:
	PORT=$(PORT) ./target/release/proxmox-panel

dev:
	PORT=$(PORT) cargo run

clean:
	cargo clean
	rm -rf infra/terraform/*
