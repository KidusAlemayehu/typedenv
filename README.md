# typedenv

Tiny, zero-dependency typed access to environment variables in Rust with built-in `.env` file support and batch error reporting.

## Features

- **Typed access**: Parse into any type `T: FromStr`.
- **Delimited list parsing**: Parse separated values into `Vec<T>` with trimming, automatic empty-item filtering, and indexed error reporting (`var_list`, `var_list_or`).
- **Built-in `.env` file support**: Load `.env` files directly into a `Loader` with zero external dependencies (`Loader::with_dotenv`).
- **Thread-safe**: Does not mutate process-global environment variables (`std::env::set_var` is never called), preventing data races in multi-threaded programs.
- **Strict defaults**: An environment variable that is set but invalid returns an error—it is never silently replaced by a default.
- **Clear errors**: Identifies which variable, list index, or `.env` line number failed and why.
- **Batch validation**: Collect all configuration errors in one go with `Loader`, instead of failing on the first one.
- **Zero-allocation keys**: Key lookups take `impl Into<Cow<'static, str>>`, completely eliminating heap allocations for static string literals on both success and error paths.
- **Zero dependencies**: Pure standard library.
- **Safe**: Built with `#![forbid(unsafe_code)]`.

## Installation

Add `typedenv` to your `Cargo.toml`:

```toml
[dependencies]
typedenv = "0.2"
```

## Quick Start

### Basic Lookups

```rust
use typedenv::{var, var_or, var_opt, var_bool, var_list};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Required variable
    let database_url: String = var("DATABASE_URL")?;

    // Variable with default (used only when unset; malformed value is an error)
    let port: u16 = var_or("PORT", 8080)?;

    // Optional variable: Ok(None) when unset
    let sentry_dsn: Option<String> = var_opt("SENTRY_DSN")?;

    // Boolean (supports true/false/1/0/yes/no/y/n/on/off, case-insensitive)
    let debug: bool = var_bool("DEBUG")?;

    // Delimited list (splits on sep, trims items, skips empty entries)
    let allowed_hosts: Vec<String> = var_list("ALLOWED_HOSTS", ',')?;

    Ok(())
}
```

### Reporting All Errors at Once with `Loader`

Instead of stopping on the first missing or invalid environment variable, use `Loader` to collect every error across lookups and report them together:

```rust
use typedenv::Loader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut loader = Loader::new();

    let host: String = loader.var_or("HOST", "127.0.0.1".into());
    let port: u16 = loader.var("PORT");
    let pool_size: usize = loader.var_or("POOL_SIZE", 10);
    let secret_key: String = loader.var("SECRET_KEY");
    let metrics: bool = loader.var_bool_or("METRICS_ENABLED", true);
    let cors_origins: Vec<String> = loader.var_list("CORS_ORIGINS", ',');

    // Validate all lookups at once
    loader.finish()?;

    // Values are validated and safe to use here
    println!("Server starting on {host}:{port}");
    Ok(())
}
```

---

### Loading `.env` Files (Zero-Dependency)

`typedenv` includes an in-tree `.env` file parser enabled by default. You can load `.env` files into a `Loader` without adding external crates:

```rust
use typedenv::Loader;

let mut loader = Loader::new()
    .with_dotenv(".env")
    .with_dotenv(".env.local"); // earlier files or real env take precedence

let database_url: String = loader.var("DATABASE_URL");
let port: u16 = loader.var_or("PORT", 8080);
loader.finish()?;
```

#### `.env` Precedence & Safety Rules

1. **Real environment variables always win**: Real OS/container variables take precedence over values in `.env` files.
2. **First file wins**: If multiple files define the same key, the file loaded first wins.
3. **Missing files are ignored**: A missing file (e.g. in staging/production environments) is ignored silently.
4. **Syntax errors are collected**: Files with syntax errors are recorded and reported in `Loader::finish()`.
5. **No process mutation**: The process-global environment table is never modified, making it safe in multi-threaded applications.

#### Supported `.env` Syntax

- `KEY=value` and `export KEY=value`
- Line comments (`# comment`) and trailing comments (`KEY=value # comment`)
- Double quotes (`"..."`) with escape sequences (`\n`, `\t`, `\r`, `\"`, `\\`)
- Single quotes (`'...'`) for raw string literals
- Whitespace trimming around keys and unquoted values

---

### Real-World Web Application Example

In production web applications (e.g. using Axum, Actix Web, or Rocket), configuration is typically structured into a typed `AppConfig` struct:

```rust
use typedenv::{Loader, Errors};

pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub jwt_secret: String,
    pub max_connections: u32,
    pub debug: bool,
    pub sentry_dsn: Option<String>,
    pub cors_origins: Vec<String>,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, Errors> {
        let mut loader = Loader::new()
            .with_dotenv(".env")
            .with_dotenv(".env.local");

        let config = Self {
            host: loader.var_or("HOST", "127.0.0.1".into()),
            port: loader.var_or("PORT", 8080),
            database_url: loader.var("DATABASE_URL"),
            jwt_secret: loader.var("JWT_SECRET"),
            max_connections: loader.var_or("MAX_CONNECTIONS", 10),
            debug: loader.var_bool_or("DEBUG", false),
            sentry_dsn: loader.var_opt("SENTRY_DSN"),
            cors_origins: loader.var_list_or("CORS_ORIGINS", ',', vec!["*".into()]),
        };

        // Validate all variables at once
        loader.finish()?;

        Ok(config)
    }
}

fn main() {
    let config = match AppConfig::from_env() {
        Ok(cfg) => cfg,
        Err(errs) => {
            eprintln!("Configuration error:\n{errs}");
            std::process::exit(1);
        }
    };

    println!("Starting server on {}:{}", config.host, config.port);
}
```

If multiple variables are missing or malformed, the application halts immediately at startup with an exhaustive batch report:

```text
Configuration error:
3 configuration errors:
 - environment variable `DATABASE_URL` is not set
 - environment variable `JWT_SECRET` is not set
 - environment variable `PORT` has invalid value `eight-thousand`: invalid digit found in string
```

---

### Global Shared Config with `std::sync::LazyLock`

Because `typedenv` operates through clean, runtime function evaluation rather than heavy procedural derive macros, it fits naturally with modern static initialization using Rust's standard library `std::sync::LazyLock`:

```rust
use std::sync::LazyLock;
use typedenv::{Loader, Errors};

pub struct AppConfig {
    pub database_url: String,
    pub port: u16,
    pub debug: bool,
}

impl AppConfig {
    pub fn load() -> Result<Self, Errors> {
        let mut cfg = Loader::new().with_dotenv(".env");
        let config = Self {
            database_url: cfg.var("DATABASE_URL"),
            port: cfg.var_or("PORT", 8080),
            debug: cfg.var_bool_or("DEBUG", false),
        };
        cfg.finish()?;
        Ok(config)
    }
}

/// Global, thread-safe configuration initialized on first access.
/// Halts startup with an exhaustive batch report if configuration is invalid.
pub static CONFIG: LazyLock<AppConfig> = LazyLock::new(|| {
    AppConfig::load().unwrap_or_else(|errs| {
        eprintln!("Configuration error:\n{errs}");
        std::process::exit(1);
    })
});

fn main() {
    println!("Connecting to database: {}", CONFIG.database_url);
    println!("Server running on port: {}", CONFIG.port);
}
```

---

### Zero-Allocation Key Lookups

All lookup signatures accept `impl Into<Cow<'static, str>>`:

```rust
// 1. Static string literals use Cow::Borrowed: ZERO heap allocations
let port: u16 = typedenv::var("PORT")?;

// 2. Dynamic keys accept owned Strings seamlessly without manual conversion
let prefix = "AUTH";
let secret: String = typedenv::var(format!("{prefix}_SECRET_KEY"))?;
```

When string literals are used, no heap allocations occur during lookup, and any resulting [`EnvError::Missing`], [`EnvError::Invalid`], or [`EnvError::NotUnicode`] variant stores the borrowed `&'static str` directly inside `Cow::Borrowed`.

---

### API Reference

| Function / Method | Signature | Unset Behavior | Invalid Value Behavior |
| :--- | :--- | :--- | :--- |
| `var<T>` | `name -> Result<T, EnvError>` | Returns `Err(EnvError::Missing)` | Returns `Err(EnvError::Invalid)` |
| `var_or<T>` | `name, default: T -> Result<T, EnvError>` | Returns `Ok(default)` | Returns `Err(EnvError::Invalid)` |
| `var_opt<T>` | `name -> Result<Option<T>, EnvError>` | Returns `Ok(None)` | Returns `Err(EnvError::Invalid)` |
| `var_bool` | `name -> Result<bool, EnvError>` | Returns `Err(EnvError::Missing)` | Returns `Err(EnvError::Invalid)` |
| `var_bool_or` | `name, default: bool -> Result<bool, EnvError>` | Returns `Ok(default)` | Returns `Err(EnvError::Invalid)` |
| `var_list<T>` | `name, sep: char -> Result<Vec<T>, EnvError>` | Returns `Err(EnvError::Missing)` | Returns `Err(EnvError::Invalid)` (indexed) |
| `var_list_or<T>` | `name, sep: char, default: Vec<T> -> Result<Vec<T>, EnvError>` | Returns `Ok(default)` | Returns `Err(EnvError::Invalid)` (indexed) |
| `Loader::var<T>` | `&mut self, name -> T` | Records `Missing`, returns `T::default()` | Records `Invalid`, returns `T::default()` |
| `Loader::var_or<T>` | `&mut self, name, default: T -> T` | Returns `default` | Records `Invalid`, returns `default` |
| `Loader::var_opt<T>` | `&mut self, name -> Option<T>` | Returns `None` | Records `Invalid`, returns `None` |
| `Loader::var_bool` | `&mut self, name -> bool` | Records `Missing`, returns `false` | Records `Invalid`, returns `false` |
| `Loader::var_bool_or` | `&mut self, name, default: bool -> bool` | Returns `default` | Records `Invalid`, returns `default` |
| `Loader::var_list<T>` | `&mut self, name, sep: char -> Vec<T>` | Records `Missing`, returns `vec![]` | Records `Invalid`, returns `vec![]` |
| `Loader::var_list_or<T>` | `&mut self, name, sep: char, default: Vec<T> -> Vec<T>` | Returns `default` | Records `Invalid`, returns `default` |
| `Loader::with_dotenv` | `self, path -> Self` | Missing file ignored silently | Records `Dotenv` with file & line |
| `Loader::finish` | `self -> Result<(), Errors>` | Returns `Ok(())` | Returns `Err(Errors)` with all problems |

---

### Why typedenv?

| Feature | `typedenv` | `envy` | `config-rs` | Manual `std::env::var` |
| :--- | :---: | :---: | :---: | :---: |
| **Dependencies** | **0 (pure stdlib)** | Serde ecosystem | 15+ crates | 0 |
| **Compile Overhead** | **< 0.5s cold build** | Heavy (proc-macros) | Heavy | None |
| **Zero-Allocation Keys** | **Yes (`Cow<'static, str>`)** | No | No | No (allocates `String`) |
| **Batch Error Reporting** | **Yes (`Loader` reports all)** | No (fails on 1st) | No | No (stops on 1st `?`) |
| **Strict Defaults** | **Yes (invalid value is error)** | Derives defaults | Often silent | Verbose manual checks |
| **Thread-Safe (No Env Mutation)** | **Yes (read-only)** | Yes | Mutates / depends | Safe |
| **In-Tree `.env` Support** | **Yes (zero deps)** | No (needs `dotenvy`) | Multiple formats | No |

---

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT) at your option.
