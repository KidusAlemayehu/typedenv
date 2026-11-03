# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.1] - 2026-11-03

### Added
- **Zero-allocation key lookups (`Cow<'static, str>`)**: Public standalone functions (`var`, `var_or`, `var_opt`, `var_bool`, `var_bool_or`, `var_list`, `var_list_or`) and all `Loader` methods now accept `impl Into<Cow<'static, str>>`.
  - Static string literal keys (`"DATABASE_URL"`) produce `Cow::Borrowed` with zero heap allocations on both success and error paths.
  - Dynamically constructed strings (`String`) are accepted seamlessly as `Cow::Owned`.
- **Structured Cow error inspection**: Added [`EnvError::name_cow`] method returning `Option<&Cow<'static, str>>` alongside existing [`EnvError::name`] (`Option<&str>`).

### Performance
- Eliminated all heap allocations for static string literal keys when constructing `EnvError::Missing`, `EnvError::Invalid`, and `EnvError::NotUnicode`.

---

## [0.2.0] - 2026-10-29

### Added
- **Built-in `.env` file support**: Added [`Loader::with_dotenv`] to load variables from `.env` files with zero external dependencies (enabled by default via the `dotenv` feature).
- **Delimited list parsing**: Added [`var_list`], [`var_list_or`], [`Loader::var_list`], and [`Loader::var_list_or`] to parse separated values into `Vec<T>` with automatic trimming and empty-item filtering.
- **Indexed list error messages**: When parsing a list, failure on a specific element reports the 1-based index and raw value (e.g. `item 2 ('http'): invalid digit found in string`).
- **Structured `.env` error reporting**: Added [`EnvError::Dotenv`] variant to capture `.env` I/O and syntax errors with 1-based line numbers.
- **In-tree `.env` parser**: Supports comments (`#`), `export` statements, double-quoted strings with escapes (`\n`, `\t`, `\r`, `\"`, `\\`), and single-quoted raw literals.
- **Precedence guarantees**: Real OS environment variables always take precedence over values in `.env` files; the process-global environment is never mutated.

### Changed
- Changed [`EnvError::name`] return type from `&str` to `Option<&str>` (returns `None` for file-scoped `.env` errors).
- Enabled `dotenv` feature flag by default in `Cargo.toml`.

---

## [0.1.0] - 2026-10-27

### Added
- Initial release under crate name `typedenv`.
- Typed environment variable lookups: [`var`], [`var_or`], [`var_opt`], [`var_bool`], and [`var_bool_or`].
- Batch error reporting with [`Loader`] and structured [`Errors`] container.
- Case-insensitive boolean parsing (`1/0`, `true/false`, `yes/no`, `y/n`, `on/off`).
- Full standard library error trait integration for [`EnvError`] and [`Errors`].
- Slices, dereferencing, and `IntoIterator` implementations for [`Errors`].
- `#![forbid(unsafe_code)]` safety compliance.
