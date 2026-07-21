SHELL := /bin/bash

APP_NAME := pt-reseeder
SERVER_BIN := pt-reseeder-server
PROFILE ?= release
DIST_DIR ?= dist
CARGO ?= cargo
WINDOWS_TARGET ?= x86_64-pc-windows-gnu
CARGO_ZIGBUILD ?= cargo zigbuild

ifeq ($(PROFILE),release)
	CARGO_PROFILE_FLAGS := --release
	TARGET_PROFILE_DIR := release
else
	CARGO_PROFILE_FLAGS :=
	TARGET_PROFILE_DIR := debug
endif

SERVER_BIN_PATH := target/$(TARGET_PROFILE_DIR)/$(SERVER_BIN)
WINDOWS_SERVER_BIN_PATH := target/$(WINDOWS_TARGET)/$(TARGET_PROFILE_DIR)/$(SERVER_BIN).exe
SITE_DIR := target/site
SERVER_DIST_DIR := $(DIST_DIR)/server
SERVER_WINDOWS_DIST_DIR := $(DIST_DIR)/server-windows
DESKTOP_DIST_DIR := $(DIST_DIR)/desktop

.PHONY: help all check fmt clippy test build build-server build-server-windows build-desktop artifacts clean distclean install-tools run-server

help:
	@echo "PT-Reseeder build targets"
	@echo ""
	@echo "Usage:"
	@echo "  make build                 Build release server and Leptos site"
	@echo "  make build-server          Build server binary and frontend site"
	@echo "  make build-server-windows  Cross-compile Windows server .exe (+ site)"
	@echo "  make build-desktop         Build Tauri desktop bundle"
	@echo "  make artifacts             Copy build outputs into $(DIST_DIR)/"
	@echo "  make check                 Run cargo check for the workspace"
	@echo "  make test                  Compile workspace tests"
	@echo "  make clippy                Run clippy for the workspace"
	@echo "  make fmt                   Format Rust code"
	@echo "  make clean                 Remove build output from target/"
	@echo "  make distclean             Remove target/ and $(DIST_DIR)/"
	@echo ""
	@echo "Variables:"
	@echo "  PROFILE=release|debug              Build profile, default: release"
	@echo "  DIST_DIR=dist                      Artifact output directory"
	@echo "  WINDOWS_TARGET=x86_64-pc-windows-gnu  Windows rustc target"

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
	cp crates/frontend/index.html "$(SITE_DIR)/index.html"
	@# wasm-bindgen JS references pt-reseeder_bg.wasm but cargo-leptos
	@# outputs pt-reseeder.wasm — create a copy so the browser can find it.
	@if [ -f "$(SITE_DIR)/pkg/$(APP_NAME).wasm" ] && [ ! -f "$(SITE_DIR)/pkg/$(APP_NAME)_bg.wasm" ]; then \
		cp "$(SITE_DIR)/pkg/$(APP_NAME).wasm" "$(SITE_DIR)/pkg/$(APP_NAME)_bg.wasm"; \
		echo "Created $(SITE_DIR)/pkg/$(APP_NAME)_bg.wasm"; \
	fi

build-server-windows: build-server
	command -v cargo-zigbuild >/dev/null || { echo "cargo-zigbuild is required. Install with: cargo install cargo-zigbuild"; exit 1; }
	command -v zig >/dev/null || { echo "zig is required for cargo-zigbuild. Install zig first."; exit 1; }
	rustup target list --installed | grep -qx '$(WINDOWS_TARGET)' || rustup target add '$(WINDOWS_TARGET)'
	$(CARGO_ZIGBUILD) $(CARGO_PROFILE_FLAGS) -p pt-reseeder-server --target '$(WINDOWS_TARGET)' --features headless-browser
	rm -rf "$(SERVER_WINDOWS_DIST_DIR)"
	mkdir -p "$(SERVER_WINDOWS_DIST_DIR)"
	cp "$(WINDOWS_SERVER_BIN_PATH)" "$(SERVER_WINDOWS_DIST_DIR)/"
	cp -R "$(SITE_DIR)" "$(SERVER_WINDOWS_DIST_DIR)/site"
	@echo "Windows server artifacts written to $(SERVER_WINDOWS_DIST_DIR)/"

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
	$(CARGO) install cargo-zigbuild

run-server: build-server
	"$(SERVER_BIN_PATH)"

clean:
	$(CARGO) clean

distclean: clean
	rm -rf "$(DIST_DIR)"
