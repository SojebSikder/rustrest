# Contributing to RustRest

First off, thank you for considering contributing to **RustRest**! It’s contributions like yours that make RustRest a great native desktop API client.

Please take a moment to review this document to ensure a smooth contribution process.

---

## Code of Conduct

By participating in this project, you agree to abide by our Code of Conduct. Please maintain a respectful, welcoming, and constructive environment for everyone.

---

## How Can I Contribute?

### 1. Reporting Bugs

Before creating a bug report, please check the [Issue Tracker](https://github.com/sojebsikder/rustrest/issues) to see if the problem has already been reported.

When creating a bug report, please include:
* **Operating System & Architecture** (e.g., Ubuntu 24.04 x86_64, macOS Sonoma ARM64, Windows 11).
* **RustRest Version** (or commit hash).
* **Steps to Reproduce** the issue clearly.
* **Expected vs. Actual Behavior**.
* **Log Output or Screenshots** (if applicable).

### 2. Suggesting Enhancements

Feature requests are always welcome! When suggesting a new feature:
* Provide a clear and descriptive title.
* Explain **why** this feature would be useful to RustRest users.
* Describe **how** you envision the feature working (UI mockups or workflow descriptions are appreciated).

### 3. Submitting Pull Requests (PRs)

1. **Fork the repository** and create your branch from `main`.
2. Make sure your code follows the existing style and builds without warnings.
3. Add tests if you are introducing new core logic or API parsing functionality.
4. Ensure all unit and integration tests pass locally.
5. Format your code using `cargo fmt`.
6. Run `cargo clippy` to catch common code quality issues.
7. Submit your Pull Request targeting the `main` branch with a clear summary of your changes.

---

## Local Development Setup

### Prerequisites

Ensure you have the following installed on your machine:
* [Rust Toolchain](https://www.rust-lang.org/tools/install) (latest stable release recommended).
* Desktop native dependencies required by your GUI toolkit (e.g., GTK headers on Linux, Xcode command line tools on macOS).

### Building and Running

1. **Clone your fork locally:**
   ```bash
   git clone https://github.com/sojebsikder/rustrest.git
   cd rustrest
   ```

2. **Run in development mode:**
   ```bash
   cargo run
   ```

3. **Format code:**
   ```bash
   cargo fmt
   ```

4. **Run linter:**
   ```bash
   cargo clippy -- -D warnings
   ```

5. **Run test suite:**
   ```bash
   cargo test
   ```

---

## Commit Message Guidelines

We follow clear, descriptive commit messages. Please start your commit summary with an action verb or standard type prefix:

* `feat:` Add new feature or request builder functionality
* `fix:` Fix a bug or edge-case handling
* `ui:` Update or tweak user interface components
* `docs:` Update documentation or inline code comments
* `refactor:` Code improvements without functional changes
* `test:` Add or update test cases

**Example:**
```text
feat(ui): add tab key navigation for request headers table
```

---

## License

By contributing to RustRest, you agree that your contributions will be licensed under the project's [MIT License](LICENSE).
