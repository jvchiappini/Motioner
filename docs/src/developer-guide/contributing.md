# Contributing to Motioner

Thank you for your interest in contributing to Motioner! This guide will help you get started.

## Code of Conduct

Be respectful, inclusive, and constructive. We're building a welcoming community for all contributors.

## Ways to Contribute

### 🐛 Report Bugs
Found a bug? [Open an issue](https://github.com/jvchiappini/Motioner/issues/new?template=bug_report.md)

### 💡 Suggest Features
Have an idea? [Open a feature request](https://github.com/jvchiappini/Motioner/issues/new?template=feature_request.md)

### 📖 Improve Documentation
Documentation improvements are always welcome!

### 💻 Submit Code
Ready to code? Follow the guide below.

## Getting Started

### 1. Fork and Clone

```bash
# Fork on GitHub, then clone your fork
git clone https://github.com/YOUR_USERNAME/Motioner.git
cd Motioner
```

### 2. Set Up Development Environment

```powershell
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install dependencies
cargo build

# Run tests
cargo test

# Format and lint
cargo fmt
cargo clippy
```

### 3. Create a Branch

```bash
git checkout -b feat/your-feature-name
# or
git checkout -b fix/bug-description
```

## Development Workflow

### Branch Naming

- `feat/` — New features
- `fix/` — Bug fixes
- `docs/` — Documentation
- `refactor/` — Code refactoring
- `test/` — Test additions/fixes
- `chore/` — Maintenance tasks

**Examples:**
- `feat/gpu-rendering`
- `fix/timeline-crash`
- `docs/api-reference`

### Commit Messages

Use [Conventional Commits](https://www.conventionalcommits.org/):

```
type(scope): description

[optional body]

[optional footer]
```

**Types:**
- `feat` — New feature
- `fix` — Bug fix
- `docs` — Documentation
- `style` — Formatting
- `refactor` — Code restructuring
- `test` — Tests
- `chore` — Maintenance

**Examples:**
```
feat(timeline): add keyframe interpolation
fix(export): resolve ffmpeg path issue on Windows
docs(readme): update installation instructions
```

### Code Style

```powershell
# Format code (required before PR)
cargo fmt

# Check style
cargo fmt -- --check

# Run linter
cargo clippy

# Fix clippy warnings
cargo clippy --fix
```

### Writing Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scene_creation() {
        let scene = Scene::new();
        assert_eq!(scene.objects.len(), 0);
    }

    #[test]
    fn test_add_object() {
        let mut scene = Scene::new();
        scene.add_object(SceneObject::default());
        assert_eq!(scene.objects.len(), 1);
    }
}
```

Run tests:
```powershell
cargo test
```

### Documentation

Document public APIs:

```rust
/// Creates a new scene with default settings.
///
/// # Examples
///
/// ```
/// let scene = Scene::new();
/// assert!(scene.objects.is_empty());
/// ```
pub fn new() -> Self {
    // ...
}
```

Generate docs:
```powershell
cargo doc --open
```

## Pull Request Process

### Before Submitting

- ✅ Code compiles without errors
- ✅ Tests pass: `cargo test`
- ✅ Code is formatted: `cargo fmt`
- ✅ No clippy warnings: `cargo clippy`
- ✅ Documentation updated if needed
- ✅ Commit messages follow conventions

### Submitting PR

1. **Push your branch**
   ```bash
   git push origin feat/your-feature
   ```

2. **Create Pull Request** on GitHub

3. **Fill PR template** with:
   - Description of changes
   - Related issues
   - Testing performed
   - Screenshots (if UI changes)

4. **Wait for review** — maintainers will review and provide feedback

### PR Template

```markdown
## Description
Brief description of changes

## Related Issues
Fixes #123

## Type of Change
- [ ] Bug fix
- [ ] New feature
- [ ] Breaking change
- [ ] Documentation update

## Testing
- [ ] Tests pass locally
- [ ] Added new tests
- [ ] Manual testing performed

## Checklist
- [ ] Code formatted with `cargo fmt`
- [ ] No clippy warnings
- [ ] Documentation updated
- [ ] Commit messages follow conventions
```

## Development Guidelines

### Code Quality

**Prefer:**
- ✅ Clear, descriptive names
- ✅ Small, focused functions
- ✅ Comprehensive error handling
- ✅ Comments for complex logic
- ✅ Type safety over stringly-typed code

**Avoid:**
- ❌ Unwrapping without checking (`unwrap()`, `expect()`)
- ❌ Panicking in library code
- ❌ Large functions (>50 lines)
- ❌ Deep nesting (>3 levels)
- ❌ Magic numbers without constants

### Performance

- Use release builds for benchmarking
- Profile before optimizing
- Document performance-critical code
- Consider memory allocations

### Dependencies

Before adding dependencies:
1. Check if existing dependencies can solve the problem
2. Verify license compatibility (Apache 2.0)
3. Consider maintenance status
4. Evaluate bundle size impact

Add to `Cargo.toml`:
```toml
[dependencies]
new_crate = "1.0"  # Add with justification in PR
```

## Architecture Guidelines

### Adding New Features

1. **Plan** — Open an issue to discuss
2. **Design** — Consider architecture impact
3. **Implement** — Follow existing patterns
4. **Test** — Add comprehensive tests
5. **Document** — Update relevant docs

### Module Organization

```
src/
├── feature/
│   ├── mod.rs       # Public interface
│   ├── types.rs     # Type definitions
│   ├── impl.rs      # Implementation
│   └── tests.rs     # Tests
```

## Testing Guidelines

### Unit Tests

```rust
// In src/module.rs or src/module/tests.rs
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_basic_functionality() {
        // Arrange
        let input = create_test_data();
        
        // Act
        let result = function_under_test(input);
        
        // Assert
        assert_eq!(result, expected);
    }
}
```

### Integration Tests

```rust
// In tests/integration_test.rs
use motioner_ui::*;

#[test]
fn test_end_to_end_workflow() {
    // Test complete workflows
}
```

## Getting Help

- 💬 [GitHub Discussions](https://github.com/jvchiappini/Motioner/discussions)
- 🐛 [Issues](https://github.com/jvchiappini/Motioner/issues)
- 📧 Contact maintainer: @jvchiappini

## Recognition

Contributors will be:
- Listed in release notes
- Mentioned in project documentation
- Thanked in commit messages

## License

By contributing, you agree that your contributions will be licensed under the Apache License 2.0.

---

**Thank you for contributing to Motioner! 🎬**
