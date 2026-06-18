ud := "./target/debug/ud"
fixture := "tests/fixtures/large_project"

# Build the project
build:
    cargo build

# Run all unit and integration tests
test:
    cargo test

# Check a manifest in check mode (dry-run)
preview path=fixture: build
    {{ud}} {{path}}

# Update a manifest losslessly
update path=fixture: build
    {{ud}} {{path}} -u

# Show the entire dependency tree
tree path=fixture: build
    {{ud}} tree {{path}}


fmt:
    cargo fmt
