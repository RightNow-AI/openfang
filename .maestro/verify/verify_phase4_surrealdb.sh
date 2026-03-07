#!/usr/bin/env bash
# =============================================================================
# PHASE 4 VERIFICATION: SurrealDB Memory Substrate
# =============================================================================
# This script MUST pass before committing Phase 4 changes.
# It checks that the SQLite memory substrate has been fully replaced by SurrealDB.
# =============================================================================

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

PASS=0
FAIL=0
WARN=0

check() {
    local description="$1"
    local result="$2"
    if [ "$result" -eq 0 ]; then
        echo -e "  ${GREEN}PASS${NC} $description"
        ((PASS++))
    else
        echo -e "  ${RED}FAIL${NC} $description"
        ((FAIL++))
    fi
}

echo "=============================================="
echo "  Phase 4 Verification: SurrealDB Memory Substrate"
echo "=============================================="
echo ""

KERNEL_CARGO="crates/openfang-kernel/Cargo.toml"
KERNEL_RS="crates/openfang-kernel/src/kernel.rs"
NEW_CRATE_DIR="crates/maestro-surreal-memory"
NEW_CRATE_LIB="${NEW_CRATE_DIR}/src/lib.rs"

# ---------------------------------------------------------------------------
# GATE 1: Crate and Dependency Verification
# ---------------------------------------------------------------------------
echo "--- Gate 1: Crate and Dependency Verification ---"

check "New crate directory '${NEW_CRATE_DIR}' exists" $([ -d "$NEW_CRATE_DIR" ] && echo 0 || echo 1)
check "New crate lib file '${NEW_CRATE_LIB}' exists" $([ -f "$NEW_CRATE_LIB" ] && echo 0 || echo 1)

check "Kernel Cargo.toml does NOT depend on openfang-memory" \
    $(grep -q "openfang-memory" "$KERNEL_CARGO" && echo 1 || echo 0)

check "Kernel Cargo.toml DOES depend on maestro-surreal-memory" \
    $(grep -q "maestro-surreal-memory" "$KERNEL_CARGO" && echo 0 || echo 1)

check "Workspace Cargo.toml contains surrealdb dependency" \
    $(grep -q "surrealdb" "Cargo.toml" && echo 0 || echo 1)

# ---------------------------------------------------------------------------
# GATE 2: Kernel Integration Verification
# ---------------------------------------------------------------------------
echo ""
echo "--- Gate 2: Kernel Integration ---"

check "kernel.rs imports SurrealMemorySubstrate" \
    $(grep -q "SurrealMemorySubstrate" "$KERNEL_RS" && echo 0 || echo 1)

check "kernel.rs does NOT import old MemorySubstrate from openfang-memory" \
    $(grep -q "use openfang_memory::MemorySubstrate" "$KERNEL_RS" && echo 1 || echo 0)

check "Kernel struct uses SurrealMemorySubstrate for memory field" \
    $(grep -A 2 "pub memory:" "$KERNEL_RS" | grep -q "SurrealMemorySubstrate" && echo 0 || echo 1)

check "Kernel boot sequence initializes SurrealMemorySubstrate" \
    $(grep -q "SurrealMemorySubstrate::connect_sync" "$KERNEL_RS" && echo 0 || echo 1)

check "Kernel boot sequence does NOT initialize old MemorySubstrate" \
    $(grep -q "MemorySubstrate::open" "$KERNEL_RS" && echo 1 || echo 0)

# ---------------------------------------------------------------------------
# GATE 3: New Crate Implementation Verification
# ---------------------------------------------------------------------------
echo ""
echo "--- Gate 3: New Crate Implementation ---"

check "New crate implements the Memory trait" \
    $(grep -q "impl Memory for SurrealMemorySubstrate" "$NEW_CRATE_LIB" && echo 0 || echo 1)

check "New crate uses SurrealDB client (e.g., Surreal<Db>)" \
    $(grep -q "Surreal<" "$NEW_CRATE_LIB" && echo 0 || echo 1)

check "New crate uses db.query() or db.select() (interacts with DB)" \
    $(grep -qE "db\.query|db\.select|db\.create|db\.update|db\.delete" "$NEW_CRATE_LIB" && echo 0 || echo 1)

check "No todo!() or unimplemented!() in Memory trait impl" \
    $(grep -c "todo!()\|unimplemented!()" "$NEW_CRATE_LIB" | grep -q "^0$" && echo 0 || echo 1)

# ---------------------------------------------------------------------------
# GATE 4: Compilation
# ---------------------------------------------------------------------------
echo ""
echo "--- Gate 4: Compilation ---"

if cargo check --workspace --lib 2>/dev/null; then
    check "cargo check --workspace --lib passes" 0
else
    check "cargo check --workspace --lib passes" 1
fi

# ---------------------------------------------------------------------------
# SUMMARY
# ---------------------------------------------------------------------------
echo ""
echo "=============================================="
echo "  RESULTS: ${GREEN}${PASS} passed${NC}, ${RED}${FAIL} failed${NC}, ${YELLOW}${WARN} warnings${NC}"
echo "=============================================="

if [ "$FAIL" -gt 0 ]; then
    echo ""
    echo -e "${RED}BLOCKED: Do NOT commit. Fix all FAIL items first.${NC}"
    exit 1
else
    echo ""
    echo -e "${GREEN}ALL GATES PASSED. Safe to commit.${NC}"
    exit 0
fi