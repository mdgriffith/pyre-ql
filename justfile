set shell := ["bash", "-cu"]

# Show available recipes with descriptions
default:
  @just --list

# Human/AI-friendly command guide with examples
help:
  @echo "Pyre just recipes"
  @echo ""
  @echo "Core workflows:"
  @echo "  just check                 # check both core and wasm"
  @echo "  just check core            # check only core crate"
  @echo "  just check wasm            # check only wasm crate"
  @echo "  just test                  # run core tests"
  @echo "  just test unit             # alias for core tests"
  @echo "  just test wasm             # run wasm tests"
  @echo "  just build                 # build core + wasm artifacts"
  @echo "  just verify                # check + test"
  @echo ""
  @echo "Allowed values:"
  @echo "  check <target>: core | wasm | all"
  @echo "  test <target>:  unit | wasm | all"

# Run checks; target must be one of core|wasm|all
check target="all":
  case "{{target}}" in \
    core) cargo check ;; \
    wasm) cargo check --manifest-path wasm/Cargo.toml ;; \
    all) cargo check && cargo check --manifest-path wasm/Cargo.toml ;; \
    *) echo "invalid check target: {{target}} (expected core|wasm|all)" >&2; exit 1 ;; \
  esac


# Run tests; target must be one of unit|wasm|all
test target="unit":
  case "{{target}}" in \
    unit) cargo test ;; \
    wasm) cargo test --manifest-path wasm/Cargo.toml ;; \
    all) cargo test && cargo test --manifest-path wasm/Cargo.toml ;; \
    *) echo "invalid test target: {{target}} (expected unit|wasm|all)" >&2; exit 1 ;; \
  esac

# Build both core + WASM and copy server artifacts
build:
  ./scripts/build

# Full local validation loop
verify:
  @just check all
  @just test all
