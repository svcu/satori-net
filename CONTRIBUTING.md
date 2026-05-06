# Contributing to satori-net

Thank you for your interest in contributing! This project is in early development, so please open an issue to discuss any significant change before submitting a pull request.

## Getting Started

1. Fork the repository and clone your fork.
2. Make sure you have Rust 1.85+ installed (`rustup show`).
3. Build the workspace: `cargo build --workspace`
4. Run `cargo clippy --workspace` and `cargo fmt --check` before submitting.

## Pull Request Guidelines

- Keep PRs focused — one logical change per PR.
- Write clear commit messages that explain *why*, not just *what*.
- If your change touches the public API of `net_core`, update the doc comments and the API table in the README.
- All CI checks must pass.

## Reporting Issues

Please include:
- Your OS and Rust version (`rustc --version`).
- The exact command you ran and the full error output.
- Steps to reproduce.

## Code Style

- Format with `cargo fmt`.
- Lint with `cargo clippy -- -D warnings`.
- No `unwrap()` in library code (`net_core`) — propagate errors with `?`.
- No debug `println!` — use `tracing` macros.
- No comments that explain *what* the code does; only add one when the *why* is non-obvious.

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE).
