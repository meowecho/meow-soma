# Installation Guide (macOS/Linux)

`meow` can be installed from source or from a release artifact.

## Option A: Build From Source

Requirements:

- Rust stable toolchain (`rustup` + `cargo`)
- Git

Commands:

```bash
git clone <YOUR_REPO_URL>
cd meow-soma
cargo build --release
install -m 0755 target/release/meow /usr/local/bin/meow
meow --help
```

## Option B: Install From Release Artifact

1. Download release artifact for your platform from GitHub Releases:
   - `meow-v<version>-linux-x86_64.tar.gz`
   - `meow-v<version>-darwin-arm64.tar.gz`
   - `meow-v<version>-darwin-x86_64.tar.gz`
2. Verify checksum from matching `.sha256` file.
3. Extract and install.

Example:

```bash
tar -xzf meow-v0.1.0-linux-x86_64.tar.gz
install -m 0755 meow /usr/local/bin/meow
meow --help
```

## First-Run Setup

OpenAI:

```bash
meow config setup --provider openai
export OPENAI_API_KEY=<your_openai_key>
meow config validate
meow ask "health check"
```

Anthropic:

```bash
meow config setup --provider anthropic
export ANTHROPIC_API_KEY=<your_anthropic_key>
meow config validate
meow ask "health check"
```

Ollama:

```bash
meow config setup --provider ollama
ollama serve
meow config validate
meow ask "health check"
```
