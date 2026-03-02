# ============================================================================
#  OpenFang — Agent Operating System
#  https://github.com/RightNow-AI/openfang
# ============================================================================
#  One binary. 14 crates. 137K LOC. This Makefile is the developer's
#  front door — every target maps to a documented workflow.
# ============================================================================

SHELL       := /bin/bash
.DEFAULT_GOAL := help

# ---------------------------------------------------------------------------
#  Constants
# ---------------------------------------------------------------------------

BIN         := openfang
RELEASE_BIN := target/release/$(BIN)
DEBUG_BIN   := target/debug/$(BIN)
CRATES      := $(shell find crates -maxdepth 1 -mindepth 1 -type d | wc -l)
BANNER      := public/assets/ascii/banner.txt

# Colors (only when stdout is a terminal)
ifneq ($(TERM),)
  BOLD   := \033[1m
  CYAN   := \033[36m
  GREEN  := \033[32m
  YELLOW := \033[33m
  RED    := \033[31m
  DIM    := \033[2m
  RESET  := \033[0m
else
  BOLD   :=
  CYAN   :=
  GREEN  :=
  YELLOW :=
  RED    :=
  DIM    :=
  RESET  :=
endif

# ---------------------------------------------------------------------------
#  Build
# ---------------------------------------------------------------------------

.PHONY: build
build: ## Build the workspace (debug)
	@printf "$(CYAN)>>$(RESET) $(BOLD)Building workspace (debug)…$(RESET)\n"
	@cargo build --workspace --lib
	@cargo build -p openfang-cli
	@printf "$(GREEN)✔$(RESET) Debug binary: $(DEBUG_BIN)\n"

.PHONY: release
release: ## Build optimized release binary (LTO + strip)
	@printf "$(CYAN)>>$(RESET) $(BOLD)Building release binary…$(RESET)\n"
	@cargo build --release -p openfang-cli
	@SIZE=$$(du -h $(RELEASE_BIN) | cut -f1); \
	 printf "$(GREEN)✔$(RESET) $(RELEASE_BIN)  $$SIZE\n"

.PHONY: install
install: release ## Build release and install to ~/.cargo/bin
	@printf "$(CYAN)>>$(RESET) $(BOLD)Installing $(BIN) to ~/.cargo/bin…$(RESET)\n"
	@cp $(RELEASE_BIN) "$$HOME/.cargo/bin/$(BIN)"
	@printf "$(GREEN)✔$(RESET) Installed: $$(which $(BIN) 2>/dev/null || echo '~/.cargo/bin/$(BIN)')\n"

# ---------------------------------------------------------------------------
#  Quality Gates
# ---------------------------------------------------------------------------

.PHONY: check
check: ## Fast workspace-wide type check (no codegen)
	@printf "$(CYAN)>>$(RESET) $(BOLD)Type-checking workspace…$(RESET)\n"
	@cargo check --workspace --all-targets
	@printf "$(GREEN)✔$(RESET) Type check passed\n"

.PHONY: test
test: ## Run the full test suite
	@printf "$(CYAN)>>$(RESET) $(BOLD)Running test suite…$(RESET)\n"
	@cargo test --workspace
	@printf "$(GREEN)✔$(RESET) All tests passed\n"

.PHONY: clippy
clippy: ## Lint — zero warnings required
	@printf "$(CYAN)>>$(RESET) $(BOLD)Running clippy (deny warnings)…$(RESET)\n"
	@cargo clippy --workspace --all-targets -- -D warnings
	@printf "$(GREEN)✔$(RESET) Clippy clean\n"

.PHONY: fmt
fmt: ## Check formatting (does not modify files)
	@printf "$(CYAN)>>$(RESET) $(BOLD)Checking formatting…$(RESET)\n"
	@cargo fmt --all -- --check
	@printf "$(GREEN)✔$(RESET) Formatting OK\n"

.PHONY: fmt-fix
fmt-fix: ## Auto-fix formatting
	@cargo fmt --all
	@printf "$(GREEN)✔$(RESET) Formatted\n"

.PHONY: lint
lint: fmt clippy ## Run all lints (fmt + clippy)

.PHONY: ci
ci: check lint test ## Full CI pipeline: check → lint → test
	@printf "\n$(GREEN)✔ CI pipeline passed$(RESET)\n"

# ---------------------------------------------------------------------------
#  Benchmarks
# ---------------------------------------------------------------------------

.PHONY: bench
bench: ## Run criterion benchmarks (openfang-channels)
	@printf "$(CYAN)>>$(RESET) $(BOLD)Running benchmarks…$(RESET)\n"
	@cargo bench -p openfang-channels
	@printf "$(GREEN)✔$(RESET) Benchmarks complete — reports in target/criterion/\n"

# ---------------------------------------------------------------------------
#  Getting Started  (mirrors README Quick Start)
# ---------------------------------------------------------------------------

.PHONY: init
init: build ## Initialize OpenFang (interactive provider setup)
	@printf "$(CYAN)>>$(RESET) $(BOLD)Running openfang init…$(RESET)\n"
	@$(DEBUG_BIN) init

.PHONY: start
start: build ## Start the OpenFang daemon
	@printf "$(CYAN)>>$(RESET) $(BOLD)Starting daemon…$(RESET)\n"
	@$(DEBUG_BIN) start

.PHONY: stop
stop: build ## Stop the running daemon
	@printf "$(CYAN)>>$(RESET) $(BOLD)Stopping daemon…$(RESET)\n"
	@$(DEBUG_BIN) stop
	@printf "$(GREEN)✔$(RESET) Daemon stopped\n"

.PHONY: doctor
doctor: build ## Run system diagnostics
	@printf "$(CYAN)>>$(RESET) $(BOLD)Running doctor…$(RESET)\n"
	@$(DEBUG_BIN) doctor

.PHONY: status
status: build ## Show daemon and agent status
	@$(DEBUG_BIN) status

# ---------------------------------------------------------------------------
#  Development Workflow
# ---------------------------------------------------------------------------

.PHONY: dev
dev: check clippy test ## Quick dev loop: check → clippy → test
	@printf "\n$(GREEN)✔ Dev checks passed$(RESET)\n"

.PHONY: loc
loc: ## Count lines of code across the workspace
	@printf "$(CYAN)>>$(RESET) $(BOLD)Lines of code$(RESET)\n"
	@if command -v tokei &>/dev/null; then \
	    tokei crates/ xtask/; \
	elif command -v cloc &>/dev/null; then \
	    cloc crates/ xtask/ --quiet; \
	else \
	    find crates xtask -name '*.rs' | xargs wc -l | tail -1 | \
	        awk '{printf "  %s lines of Rust\n", $$1}'; \
	fi

.PHONY: tree
tree: ## Show workspace crate structure
	@printf "$(CYAN)>>$(RESET) $(BOLD)Workspace ($(CRATES) crates + xtask)$(RESET)\n"
	@printf "$(DIM)"; \
	 for dir in crates/*/; do \
	    name=$$(basename "$$dir"); \
	    printf "  ├── %s\n" "$$name"; \
	 done; \
	 printf "  └── xtask\n"; \
	 printf "$(RESET)"

.PHONY: deps
deps: ## Show dependency tree (top-level only)
	@cargo tree --workspace --depth 1

# ---------------------------------------------------------------------------
#  Cleanup
# ---------------------------------------------------------------------------

.PHONY: clean
clean: ## Remove build artifacts
	@printf "$(CYAN)>>$(RESET) $(BOLD)Cleaning…$(RESET)\n"
	@cargo clean
	@printf "$(GREEN)✔$(RESET) Clean\n"

# ---------------------------------------------------------------------------
#  Help
# ---------------------------------------------------------------------------

.PHONY: help
help: ## Show this help
	@if [ -t 1 ] && [ "$$(tput cols 2>/dev/null || echo 0)" -ge 80 ] && [ -f $(BANNER) ]; then \
	    printf "\n"; \
	    sed 's/\x1b\[?25[lh]//g' $(BANNER); \
	    printf "\n"; \
	else \
	    printf "\n"; \
	    printf "  $(BOLD)OpenFang$(RESET) — Agent Operating System\n"; \
	    printf "  $(DIM)https://github.com/RightNow-AI/openfang$(RESET)\n"; \
	    printf "\n"; \
	fi
	@printf "  $(BOLD)Usage:$(RESET)  make $(CYAN)<target>$(RESET)\n\n"
	@awk 'BEGIN {FS = ":.*##"} \
	    /^[a-zA-Z_-]+:.*##/ { \
	        printf "  $(CYAN)%-14s$(RESET) %s\n", $$1, $$2 \
	    }' $(MAKEFILE_LIST)
	@printf "\n"
	@printf "  $(BOLD)Quick Start:$(RESET)\n"
	@printf "    make build        $(DIM)# compile debug binary$(RESET)\n"
	@printf "    make init         $(DIM)# interactive setup$(RESET)\n"
	@printf "    make start        $(DIM)# launch the daemon$(RESET)\n"
	@printf "\n"
	@printf "  $(BOLD)Development:$(RESET)\n"
	@printf "    make dev          $(DIM)# check + clippy + test$(RESET)\n"
	@printf "    make ci           $(DIM)# full CI pipeline$(RESET)\n"
	@printf "    make release      $(DIM)# optimized single binary$(RESET)\n"
	@printf "\n"
