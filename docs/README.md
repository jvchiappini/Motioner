# Motioner Documentation

Welcome to the Motioner documentation source!

## 📚 Building the Documentation

### Prerequisites

```powershell
# Install mdBook
cargo install mdbook
```

### Build & Serve

```powershell
# Build documentation
mdbook build

# Serve locally with live reload
mdbook serve --open
```

The documentation will be available at `http://localhost:3000`

## 📁 Documentation Structure

```
docs/
├── book.toml              # mdBook configuration
├── src/                   # Documentation source
│   ├── SUMMARY.md        # Table of contents
│   ├── introduction.md
│   ├── user-guide/       # User documentation
│   ├── developer-guide/  # Developer documentation
│   ├── advanced/         # Advanced topics
│   ├── examples/         # Code examples
│   └── reference/        # Reference materials
└── book/                 # Generated output (gitignored)
```

## ✍️ Contributing to Docs

### Adding a New Page

1. Create a new `.md` file in the appropriate directory
2. Add it to `src/SUMMARY.md`
3. Write content using Markdown
4. Test locally with `mdbook serve`

### Markdown Guidelines

- Use proper headings hierarchy (# → ## → ###)
- Include code blocks with language hints:
  ````markdown
  ```rust
  fn example() {}
  ```
  ````
- Add links to related pages
- Include examples where helpful

### Code Examples

When showing Rust code:
- Keep examples short and focused
- Show complete, runnable code when possible
- Explain non-obvious parts
- Follow project style guidelines

## 🚀 Deployment

Documentation is automatically deployed to GitHub Pages when changes are pushed to `main`:

```yaml
# .github/workflows/docs.yml
# Automatically builds and deploys docs
```

## 📖 Documentation Categories

### User Guide
For end users of Motioner:
- Getting started
- Interface overview
- Creating animations
- Exporting projects

### Developer Guide
For contributors and developers:
- Architecture
- Building from source
- API reference
- Contributing guidelines

### Advanced Topics
For advanced users:
- GPU rendering
- Custom animations
- Performance optimization

### Examples
Practical code examples:
- Basic animations
- Frame export
- FFmpeg integration

### Reference
Quick reference materials:
- Keyboard shortcuts
- Configuration options
- Troubleshooting
- FAQ

## 🛠️ Tools & Extensions

### Recommended VS Code Extensions

- **Markdown All in One** — Markdown editing
- **markdownlint** — Markdown linting
- **Code Spell Checker** — Catch typos

### mdBook Plugins

```powershell
# Optional: Add more features
cargo install mdbook-linkcheck    # Check for broken links
cargo install mdbook-toc          # Generate TOCs
```

## 📝 Style Guide

### Writing Style
- Use clear, concise language
- Write in present tense
- Use active voice
- Be specific and accurate

### Formatting
- Use **bold** for UI elements
- Use `code` for file names, functions, and commands
- Use > for callouts and notes
- Use tables for structured data

### Code Blocks
```markdown
```rust
// Good: Clear, commented code
fn create_scene() -> Scene {
    let scene = Scene::new();
    scene.width = 1920;
    scene
}
```
```

## 🔗 Useful Links

- [mdBook Documentation](https://rust-lang.github.io/mdBook/)
- [Markdown Guide](https://www.markdownguide.org/)
- [Motioner Repository](https://github.com/jvchiappini/Motioner)

## 📞 Questions?

- Open an issue on GitHub
- Check existing documentation
- Ask in Discussions

---

**Happy documenting! 📚**
