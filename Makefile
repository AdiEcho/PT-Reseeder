SHELL := /bin/bash

APP_NAME := pt-reseeder
SERVER_BIN := pt-reseeder-server
PROFILE ?= release
DIST_DIR ?= dist
CARGO ?= cargo

ifeq ($(PROFILE),release)
	CARGO_PROFILE_FLAGS := --release
	TARGET_PROFILE_DIR := release
else
	CARGO_PROFILE_FLAGS :=
	TARGET_PROFILE_DIR := debug
endif

SERVER_BIN_PATH := target/$(TARGET_PROFILE_DIR)/$(SERVER_BIN)
SITE_DIR := target/site
SERVER_DIST_DIR := $(DIST_DIR)/server
DESKTOP_DIST_DIR := $(DIST_DIR)/desktop

.PHONY: help all check fmt clippy test build build-server build-desktop artifacts clean distclean install-tools run-server

help:
	@echo "PT-Reseeder build targets"
	@echo ""
	@echo "Usage:"
	@echo "  make build          Build release server and Leptos site"
	@echo "  make build-server   Build server binary and frontend site"
	@echo "  make build-desktop  Build Tauri desktop bundle"
	@echo "  make artifacts      Copy build outputs into $(DIST_DIR)/"
	@echo "  make check          Run cargo check for the workspace"
	@echo "  make test           Compile workspace tests"
	@echo "  make clippy         Run clippy for the workspace"
	@echo "  make fmt            Format Rust code"
	@echo "  make clean          Remove build output from target/"
	@echo "  make distclean      Remove target/ and $(DIST_DIR)/"
	@echo ""
	@echo "Variables:"
	@echo "  PROFILE=release|debug  Build profile, default: release"
	@echo "  DIST_DIR=dist          Artifact output directory"

all: build artifacts

check:
	$(CARGO) check --workspace

fmt:
	$(CARGO) fmt --all

clippy:
	$(CARGO) clippy --workspace --all-targets -- -D warnings

test:
	$(CARGO) test --workspace --no-run

build: build-server

build-server:
	command -v cargo-leptos >/dev/null || { echo "cargo-leptos is required. Install with: cargo install cargo-leptos"; exit 1; }
	$(CARGO) leptos build $(CARGO_PROFILE_FLAGS)

build-desktop: build-server
	command -v cargo-tauri >/dev/null || { echo "cargo-tauri is required. Install with: cargo install tauri-cli --version '^2'"; exit 1; }
	cd crates/desktop && $(CARGO) tauri build --bundles app

artifacts: build-server
	rm -rf "$(SERVER_DIST_DIR)"
	mkdir -p "$(SERVER_DIST_DIR)"
	cp "$(SERVER_BIN_PATH)" "$(SERVER_DIST_DIR)/"
	cp -R "$(SITE_DIR)" "$(SERVER_DIST_DIR)/site"
	@echo "Server artifacts written to $(SERVER_DIST_DIR)/"
	@if [ -d target/release/bundle ]; then \
		rm -rf "$(DESKTOP_DIST_DIR)"; \
		mkdir -p "$(DESKTOP_DIST_DIR)"; \
		cp -R target/release/bundle/. "$(DESKTOP_DIST_DIR)/"; \
		echo "Desktop bundle artifacts written to $(DESKTOP_DIST_DIR)/"; \
	elif [ -x target/release/pt-reseeder-desktop ]; then \
		rm -rf "$(DESKTOP_DIST_DIR)"; \
		mkdir -p "$(DESKTOP_DIST_DIR)"; \
		cp target/release/pt-reseeder-desktop "$(DESKTOP_DIST_DIR)/"; \
		echo "Desktop executable written to $(DESKTOP_DIST_DIR)/"; \
	else \
		echo "Desktop artifact not found. Run 'make build-desktop' to create it."; \
	fi

install-tools:
	$(CARGO) install cargo-leptos
	$(CARGO) install tauri-cli --version '^2'

run-server: build-server
	"$(SERVER_BIN_PATH)"

clean:
	$(CARGO) clean

distclean: clean
	rm -rf "$(DIST_DIR)"
