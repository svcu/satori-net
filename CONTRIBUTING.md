# Contributing to satori-net

Thank you for your interest in contributing!

## Getting Started

1. Fork the repository and clone your fork.
2. Install the Rust toolchain via [rustup](https://rustup.rs/).
3. Build the workspace:
   ```sh
   cargo build --workspace
   ```
4. Run the tests:
   ```sh
   cargo test --workspace
   ```

## Development Workflow

- Create a feature branch from `main`.
- Keep commits focused and well-described.
- Run `cargo clippy --workspace` and resolve any warnings before opening a PR.
- Run `cargo fmt --all` to ensure consistent formatting.

## Submitting a Pull Request

1. Open a PR against `main` with a clear title and description of the change.
2. Reference any related issues.
3. Ensure CI passes.

## Reporting Issues

Please use GitHub Issues for bug reports and feature requests. Include:
- Rust version (`rustc --version`)
- Operating system
- Steps to reproduce
- Expected vs actual behavior

## Code Style

- Follow standard Rust idioms and the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/).
- Document all public items with doc comments (`///`).
- Prefer `?` over `.unwrap()` in library code.

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE).
