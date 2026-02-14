# Contributing to Motioner

Thank you for considering contributing to Motioner! This document provides guidelines and instructions for contributing.

## 📋 Table of Contents

- [Code of Conduct](#code-of-conduct)
- [How Can I Contribute?](#how-can-i-contribute)
- [Development Setup](#development-setup)
- [Pull Request Process](#pull-request-process)
- [Style Guidelines](#style-guidelines)
- [Community](#community)

## 🤝 Code of Conduct

### Our Pledge

We are committed to providing a welcoming and inclusive experience for everyone. We pledge to:

- Use welcoming and inclusive language
- Be respectful of differing viewpoints and experiences
- Gracefully accept constructive criticism
- Focus on what is best for the community
- Show empathy towards other community members

### Unacceptable Behavior

- Harassment, trolling, or discriminatory comments
- Publishing others' private information
- Other conduct which could reasonably be considered inappropriate

## 🎯 How Can I Contribute?

### Reporting Bugs

Before creating bug reports, please check existing issues. When creating a bug report, include:

- **Clear title** — Descriptive and specific
- **Description** — Detailed explanation of the issue
- **Steps to reproduce** — Numbered list of steps
- **Expected behavior** — What should happen
- **Actual behavior** — What actually happens
- **Environment** — OS, Rust version, Motioner version
- **Screenshots** — If applicable

**Use the bug report template** when opening an issue.

### Suggesting Features

Feature suggestions are welcome! Please:

1. Check if the feature already exists or is planned
2. Open an issue with the `enhancement` label
3. Provide clear use cases and benefits
4. Explain how it fits Motioner's goals

**Use the feature request template** when opening an issue.

### Contributing Code

1. **Find or create an issue** — Discuss the change
2. **Fork the repository**
3. **Create a branch** — Use naming conventions
4. **Make changes** — Follow style guidelines
5. **Test thoroughly** — Add tests if applicable
6. **Submit a pull request** — Use the PR template

### Improving Documentation

Documentation improvements are always appreciated:

- Fix typos or unclear explanations
- Add examples and tutorials
- Improve API documentation
- Translate documentation (future)

## 🛠️ Development Setup

### Prerequisites

```powershell
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Verify installation
rustc --version
cargo --version

# Install FFmpeg
# Download from https://ffmpeg.org/download.html
# Add to system PATH
```

### Clone and Build

```bash
# Fork the repository on GitHub first
git clone https://github.com/YOUR_USERNAME/Motioner.git
cd Motioner

# Build the project
cargo build

# Run tests
cargo test

# Run the application
cargo run --release
```

### Development Tools

```powershell
# Install helpful tools
cargo install cargo-watch    # Auto-rebuild on changes
cargo install cargo-edit     # Manage dependencies
cargo install mdbook        # Build documentation

# Use during development
cargo watch -x run          # Auto-reload
```

## 🔄 Pull Request Process

### Branch Naming

Use descriptive branch names:

- `feat/feature-name` — New features
- `fix/bug-description` — Bug fixes
- `docs/what-changed` — Documentation
- `refactor/component` — Code refactoring
- `test/what-tested` — Test additions

**Examples:**
- `feat/gpu-rendering-pipeline`
- `fix/timeline-playback-crash`
- `docs/update-api-reference`

### Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

**Types:**
- `feat` — New feature
- `fix` — Bug fix
- `docs` — Documentation only
- `style` — Code style (formatting, etc.)
- `refactor` — Code restructuring
- `perf` — Performance improvement
- `test` — Adding tests
- `chore` — Maintenance

**Examples:**
```
feat(timeline): add keyframe interpolation support

Implemented cubic bezier interpolation for smoother
animations between keyframes.

Closes #42
```

```
fix(export): resolve FFmpeg path issue on Windows

FFmpeg path was not properly escaped on Windows,
causing export failures.

Fixes #38
```

### Before Submitting

Ensure your PR meets these requirements:

- [ ] Code compiles without errors: `cargo build`
- [ ] Tests pass: `cargo test`
- [ ] Code is formatted: `cargo fmt`
- [ ] No Clippy warnings: `cargo clippy`
- [ ] Documentation updated (if applicable)
- [ ] Commits follow convention
- [ ] PR description is clear and complete

### PR Checklist

```markdown
## Description
Clear description of what changed and why

## Type of Change
- [ ] Bug fix (non-breaking change fixing an issue)
- [ ] New feature (non-breaking change adding functionality)
- [ ] Breaking change (fix or feature causing existing functionality to change)
- [ ] Documentation update

## Related Issues
Fixes #(issue number)

## Testing
- [ ] Tested locally
- [ ] Added new tests
- [ ] All tests pass

## Screenshots (if applicable)
Add screenshots for UI changes

## Checklist
- [ ] My code follows the project's style guidelines
- [ ] I have performed a self-review
- [ ] I have commented complex code
- [ ] I have updated documentation
- [ ] My changes generate no new warnings
- [ ] I have added tests that prove my fix/feature works
```

### Review Process

1. **Automated checks** — CI runs tests and linting
2. **Maintainer review** — Code review and feedback
3. **Address feedback** — Make requested changes
4. **Approval** — Maintainer approves PR
5. **Merge** — Your contribution is merged!

**Response times:**
- Initial review: Within 7 days
- Follow-up: Within 3-5 days

## 📝 Style Guidelines

### Rust Code Style

```rust
// Use rustfmt default configuration
cargo fmt

// Follow Rust naming conventions
struct SceneObject { }  // PascalCase for types
fn create_scene() { }   // snake_case for functions
const MAX_FPS: u32 = 120;  // SCREAMING_SNAKE_CASE for constants

// Prefer explicit types for public APIs
pub fn render_frame(scene: &Scene, frame: usize) -> Result<FrameBuffer> {
    // Implementation
}

// Document public APIs
/// Renders a single frame from the scene.
///
/// # Arguments
/// * `scene` - The scene to render
/// * `frame` - Frame number to render
///
/// # Returns
/// A `FrameBuffer` containing the rendered image
///
/// # Errors
/// Returns an error if rendering fails
pub fn render_frame(scene: &Scene, frame: usize) -> Result<FrameBuffer> {
    // Implementation
}
```

### Code Quality

**Do:**
- ✅ Use descriptive variable names
- ✅ Keep functions small and focused
- ✅ Handle errors properly (no unwrap in library code)
- ✅ Add comments for complex logic
- ✅ Write tests for new functionality

**Don't:**
- ❌ Use `.unwrap()` or `.expect()` without good reason
- ❌ Ignore compiler warnings
- ❌ Commit commented-out code
- ❌ Use magic numbers (define constants)
- ❌ Create deeply nested code

### Documentation Style

```rust
/// Brief one-line summary.
///
/// More detailed explanation of what this does,
/// including any important details.
///
/// # Examples
///
/// ```
/// let scene = Scene::new();
/// scene.add_object(obj);
/// ```
///
/// # Errors
///
/// Returns an error if...
///
/// # Panics
///
/// Panics if... (if applicable)
```

### Git Practices

```bash
# Keep commits atomic and focused
git add specific_file.rs
git commit -m "feat(renderer): add frame caching"

# Rebase before submitting PR
git fetch upstream
git rebase upstream/main

# Keep history clean
git rebase -i HEAD~3  # Squash related commits
```

## 🌟 Recognition

Contributors will be:

- ✨ Listed in release notes
- 📝 Mentioned in the project README (for significant contributions)
- 🎉 Thanked publicly on GitHub

### Significant Contributions

Major contributions (new features, major refactors) may result in:
- Co-authorship credit
- Special recognition in documentation
- Invitation to become a maintainer

## 🆘 Getting Help

### Resources

- 📖 [Documentation](https://github.com/jvchiappini/Motioner/tree/main/docs)
- 💬 [GitHub Discussions](https://github.com/jvchiappini/Motioner/discussions) — Ask questions
- 🐛 [Issues](https://github.com/jvchiappini/Motioner/issues) — Report bugs
- 📧 Maintainer: [@jvchiappini](https://github.com/jvchiappini)

### Questions?

Don't hesitate to:
- Open a discussion thread
- Comment on relevant issues
- Reach out to maintainers

We're here to help! 🙂

## 📄 License

By contributing to Motioner, you agree that your contributions will be licensed under the [Apache License 2.0](LICENSE).

---

<div align="center">

**Thank you for contributing to Motioner!** 🎬

Together we're building something amazing! 🚀

</div>
