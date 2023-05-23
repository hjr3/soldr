# Contributing

## Setup

- Install Rust from https://rustup.rs/
- clone this repo
- Run `cargo test` to verify everything is working as expected

## Development

- Make changes
- Use `cargo check --tests` to get quick compiler feedback
   - Using `cargo check` will only check non-test files
- Use `cargo test` to test functionality
   - Use the `enable_tracing()` helper function to get test output

### Linting

- Run `cargo fmt --check` to see if there are formatting issues
- Run `cargo clippy` to get best practice feedback
