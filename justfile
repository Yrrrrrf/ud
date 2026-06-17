ud := "./target/debug/ud"
fixture := "tests/fixtures/large_project"

# Build the project
build:
    cargo build

# Run all unit and integration tests
test:
    cargo test

# Check a manifest in preview mode (dry-run)
preview path=fixture: build
    {{ud}} {{path}} -y

# Update a manifest losslessly (default behavior)
update path=fixture: build
    {{ud}} {{path}}

# Show the entire dependency tree
tree path=fixture: build
    {{ud}} tree {{path}}
