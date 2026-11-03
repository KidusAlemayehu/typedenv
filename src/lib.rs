//! # typedenv
//!
//! Tiny, zero-dependency typed access to environment variables.
//!
//! * Parse into any `T: FromStr`.
//! * A set-but-invalid value is an **error**, never silently replaced by a default.
//! * Errors name the variable and say what was wrong.
//! * [`Loader`] collects *all* problems and reports them at once.
//!
//! ```
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # unsafe { std::env::set_var("TYPEDENV_DOC_PORT", "9000"); }
//! let port: u16 = typedenv::var_or("TYPEDENV_DOC_PORT", 8080)?;
//! assert_eq!(port, 9000);
//! # Ok(())
//! # }
//! ```
//!
//! Collecting every error in one go:
//!
//! ```
//! let mut cfg = typedenv::Loader::new();
//! let _port: u16 = cfg.var_or("TYPEDENV_DOC_NOPE_PORT", 8080);
//! let _db: String = cfg.var("TYPEDENV_DOC_NOPE_DB"); // missing -> recorded
//! assert!(cfg.finish().is_err());
//! ```

#![cfg_attr(not(test), forbid(unsafe_code))]
#![warn(missing_docs)]

use std::{borrow::Cow, collections::HashMap, env, error::Error, fmt, str::FromStr};

type FileVars = HashMap<String, String>;

/// An error that occurred while reading or parsing an environment variable.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EnvError {
    /// The environment variable is not set.
    Missing {
        /// The name of the missing variable.
        name: Cow<'static, str>,
    },

    /// The environment variable's value is not valid Unicode.
    NotUnicode {
        /// The name of the variable.
        name: Cow<'static, str>,
    },

    /// The environment variable value exists but could not be parsed into the target type.
    Invalid {
        /// The name of the variable.
        name: Cow<'static, str>,
        /// The raw value of the variable.
        value: String,
        /// The reason why parsing or processing failed.
        reason: String,
    },

    /// An `.env` file could not be read or contains a syntax error.
    Dotenv {
        /// Path to the file.
        path: String,
        /// 1-based line number, if the problem occurred on a specific line.
        line: Option<usize>,
        /// Reason describing what went wrong.
        reason: String,
    },
}

impl EnvError {
    /// The name of the variable this error is about, or `None` if the error
    /// is file-scoped (such as an `.env` I/O or syntax error).
    pub fn name(&self) -> Option<&str> {
        match self {
            EnvError::Missing { name }
            | EnvError::NotUnicode { name }
            | EnvError::Invalid { name, .. } => Some(name.as_ref()),
            EnvError::Dotenv { .. } => None,
        }
    }

    /// The name of the variable as a [`Cow<'static, str>`], or `None` if the error
    /// is file-scoped.
    pub fn name_cow(&self) -> Option<&Cow<'static, str>> {
        match self {
            EnvError::Missing { name }
            | EnvError::NotUnicode { name }
            | EnvError::Invalid { name, .. } => Some(name),
            EnvError::Dotenv { .. } => None,
        }
    }
}

impl fmt::Display for EnvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EnvError::Missing { name } => {
                write!(f, "environment variable `{name}` is not set")
            }
            EnvError::NotUnicode { name } => {
                write!(f, "environment variable `{name}` is not valid unicode")
            }
            EnvError::Invalid {
                name,
                value,
                reason,
            } => write!(
                f,
                "environment variable `{name}` has invalid value `{value}`: {reason}"
            ),
            EnvError::Dotenv {
                path,
                line: Some(line),
                reason,
            } => {
                write!(f, "{path}:{line}: invalid .env syntax: {reason}")
            }
            EnvError::Dotenv {
                path,
                line: None,
                reason,
            } => {
                write!(f, "could not read `{path}`: {reason}")
            }
        }
    }
}

impl Error for EnvError {}

/// Several [`EnvError`]s collected by a [`Loader`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Errors(Vec<EnvError>);

impl Errors {
    /// Multiple [`EnvError`]s collected by a [`Loader`] as a slice.
    pub fn as_slice(&self) -> &[EnvError] {
        &self.0
    }

    /// Iterate over the collected [`EnvError`]s.
    pub fn iter(&self) -> std::slice::Iter<'_, EnvError> {
        self.0.iter()
    }

    /// Number of collected errors.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if there are no errors.
    ///
    /// Always `false` for errors returned by [`Loader::finish`].
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Consumes the container and returns the inner vector of [`EnvError`]s.
    pub fn into_vec(self) -> Vec<EnvError> {
        self.0
    }
}

impl std::ops::Deref for Errors {
    type Target = [EnvError];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl IntoIterator for Errors {
    type Item = EnvError;
    type IntoIter = std::vec::IntoIter<EnvError>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Errors {
    type Item = &'a EnvError;
    type IntoIter = std::slice::Iter<'a, EnvError>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl fmt::Display for Errors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = self.0.len();
        write!(
            f,
            "{n} configuration error{}:",
            if n == 1 { "" } else { "s" }
        )?;
        for e in &self.0 {
            write!(f, "\n - {e}")?;
        }
        Ok(())
    }
}

impl Error for Errors {}

fn parse_str<T>(s: &str) -> Result<T, String>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    s.parse::<T>().map_err(|e| e.to_string())
}

fn parse_bool(s: &str) -> Result<bool, String> {
    match s.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "y" | "on" => Ok(true),
        "0" | "false" | "no" | "n" | "off" => Ok(false),
        _ => Err("expected one of true/false/1/0/yes/no/y/n/on/off".to_string()),
    }
}

/// Splits on `sep`, trims items, and skips empty items.
fn parse_list<T>(s: &str, sep: char) -> Result<Vec<T>, String>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    let mut out = Vec::new();
    let items = s.split(sep).map(str::trim).filter(|x| !x.is_empty());

    for (i, item) in items.enumerate() {
        let v = item
            .parse::<T>()
            .map_err(|e| format!("item {} (`{item}`): {e}", i + 1))?;
        out.push(v);
    }
    Ok(out)
}

/// Reads and parses an optional variable. `Ok(None)` means "not set".
fn read_opt<T>(
    name: Cow<'static, str>,
    file: &FileVars,
    parse: impl FnOnce(&str) -> Result<T, String>,
) -> Result<Option<T>, EnvError> {
    match env::var(name.as_ref()) {
        Ok(raw) => match parse(raw.trim()) {
            Ok(v) => Ok(Some(v)),
            Err(reason) => Err(EnvError::Invalid {
                name,
                value: raw,
                reason,
            }),
        },
        Err(env::VarError::NotPresent) => match file.get(name.as_ref()) {
            Some(raw) => match parse(raw.trim()) {
                Ok(v) => Ok(Some(v)),
                Err(reason) => Err(EnvError::Invalid {
                    name,
                    value: raw.clone(),
                    reason,
                }),
            },
            None => Ok(None),
        },
        Err(env::VarError::NotUnicode(_)) => Err(EnvError::NotUnicode { name }),
    }
}

/// Reads and parses a required variable. Unset returns `Err(EnvError::Missing)`.
fn read_req<T>(
    name: Cow<'static, str>,
    file: &FileVars,
    parse: impl FnOnce(&str) -> Result<T, String>,
) -> Result<T, EnvError> {
    match env::var(name.as_ref()) {
        Ok(raw) => match parse(raw.trim()) {
            Ok(v) => Ok(v),
            Err(reason) => Err(EnvError::Invalid {
                name,
                value: raw,
                reason,
            }),
        },
        Err(env::VarError::NotPresent) => match file.get(name.as_ref()) {
            Some(raw) => match parse(raw.trim()) {
                Ok(v) => Ok(v),
                Err(reason) => Err(EnvError::Invalid {
                    name,
                    value: raw.clone(),
                    reason,
                }),
            },
            None => Err(EnvError::Missing { name }),
        },
        Err(env::VarError::NotUnicode(_)) => Err(EnvError::NotUnicode { name }),
    }
}

fn read_env_opt<T>(
    name: Cow<'static, str>,
    parse: impl FnOnce(&str) -> Result<T, String>,
) -> Result<Option<T>, EnvError> {
    let empty = FileVars::new();
    read_opt(name, &empty, parse)
}

fn read_env_req<T>(
    name: Cow<'static, str>,
    parse: impl FnOnce(&str) -> Result<T, String>,
) -> Result<T, EnvError> {
    let empty = FileVars::new();
    read_req(name, &empty, parse)
}

/// Reads a required variable.
pub fn var<T>(name: impl Into<Cow<'static, str>>) -> Result<T, EnvError>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    read_env_req(name.into(), parse_str::<T>)
}

/// Reads a variable, using `default` only when it is **unset**.
///
/// A value that is set but malformed is still an error.
pub fn var_or<T>(name: impl Into<Cow<'static, str>>, default: T) -> Result<T, EnvError>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    Ok(read_env_opt(name.into(), parse_str::<T>)?.unwrap_or(default))
}

/// Reads an optional variable: `Ok(None)` when unset.
pub fn var_opt<T>(name: impl Into<Cow<'static, str>>) -> Result<Option<T>, EnvError>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    read_env_opt(name.into(), parse_str::<T>)
}

/// Reads a required boolean. Accepts `true/false/1/0/yes/no/y/n/on/off`
/// (case-insensitive).
pub fn var_bool(name: impl Into<Cow<'static, str>>) -> Result<bool, EnvError> {
    read_env_req(name.into(), parse_bool)
}

/// Reads a boolean, using `default` only when unset.
pub fn var_bool_or(name: impl Into<Cow<'static, str>>, default: bool) -> Result<bool, EnvError> {
    Ok(read_env_opt(name.into(), parse_bool)?.unwrap_or(default))
}

/// Reads a required list, split on `sep`. Items are trimmed and empty items
/// are skipped, so `"a, b,"` is `["a", "b"]`. A set-but-empty variable is an
/// empty list.
///
/// ```
/// # fn main() -> Result<(), typedenv::EnvError> {
/// let ports = typedenv::var_list_or("TYPEDENV_DOC_UNSET_PORTS", ',', vec![80u16, 443])?;
/// assert_eq!(ports, [80, 443]);
/// # Ok(())
/// # }
/// ```
pub fn var_list<T>(name: impl Into<Cow<'static, str>>, sep: char) -> Result<Vec<T>, EnvError>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    read_env_req(name.into(), |s| parse_list::<T>(s, sep))
}

/// Like [`var_list`], using `default` only when the variable is unset.
pub fn var_list_or<T>(
    name: impl Into<Cow<'static, str>>,
    sep: char,
    default: Vec<T>,
) -> Result<Vec<T>, EnvError>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    Ok(read_env_opt(name.into(), |s| parse_list::<T>(s, sep))?.unwrap_or(default))
}

// Loader: collect every problem, report once

/// Collects errors across many lookups so you can report them all at once.
///
/// Each method returns a usable placeholder on failure (the default, or
/// `T::default()`), records the error, and lets you write straight-line code.
/// Call [`Loader::finish`] once at the end and **do not use the values if it
/// returns `Err`**.
#[derive(Debug, Default)]
pub struct Loader {
    errors: Vec<EnvError>,
    file: FileVars,
}

impl Loader {
    /// Creates an empty loader.
    pub fn new() -> Self {
        Self::default()
    }

    /// `None` if unset or invalid (invalid is recorded).
    fn lookup_opt<T>(
        &mut self,
        name: Cow<'static, str>,
        parse: impl FnOnce(&str) -> Result<T, String>,
    ) -> Option<T> {
        match read_opt(name, &self.file, parse) {
            Ok(v) => v,
            Err(e) => {
                self.errors.push(e);
                None
            }
        }
    }

    /// Like `lookup_opt`, but unset is also an error. Returns `T::default()` on failure.
    fn lookup_req<T: Default>(
        &mut self,
        name: Cow<'static, str>,
        parse: impl FnOnce(&str) -> Result<T, String>,
    ) -> T {
        match read_req(name, &self.file, parse) {
            Ok(v) => v,
            Err(e) => {
                self.errors.push(e);
                T::default()
            }
        }
    }

    /// Loads a `.env` file as a fallback source.
    ///
    /// * Real environment variables always win over the file.
    /// * If several files define a key, the one loaded **first** wins.
    /// * A file that does not exist is ignored; one that cannot be read or has
    ///   syntax errors is recorded and reported by [`Loader::finish`].
    /// * The process environment is never modified.
    ///
    /// Supported syntax: `KEY=value`, `export KEY=value`, `#` comments (full
    /// line or after whitespace), `"double quoted"` values with `\n \t \" \\`
    /// escapes, and `'single quoted'` literals. There is no `$VAR` expansion.
    ///
    /// ```
    /// let mut cfg = typedenv::Loader::new().with_dotenv(".env.does-not-exist");
    /// let port: u16 = cfg.var_or("TYPEDENV_DOC_DOTENV_PORT", 8080);
    /// assert_eq!(port, 8080);
    /// assert!(cfg.finish().is_ok());
    /// ```
    #[cfg(feature = "dotenv")]
    pub fn with_dotenv(mut self, path: impl AsRef<std::path::Path>) -> Self {
        let path = path.as_ref();
        let shown = path.display().to_string();
        match std::fs::read_to_string(path) {
            Ok(src) => {
                let (vars, errs) = parse_dotenv(&src);
                for (line, reason) in errs {
                    self.errors.push(EnvError::Dotenv {
                        path: shown.clone(),
                        line: Some(line),
                        reason,
                    });
                }
                for (k, v) in vars {
                    self.file.entry(k).or_insert(v);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => self.errors.push(EnvError::Dotenv {
                path: shown,
                line: None,
                reason: e.to_string(),
            }),
        }
        self
    }

    /// Required variable. Returns `T::default()` and records an error on failure.
    ///
    /// For types that do not implement [`Default`], consider [`Loader::var_or`]
    /// or [`Loader::var_opt`].
    pub fn var<T>(&mut self, name: impl Into<Cow<'static, str>>) -> T
    where
        T: FromStr + Default,
        T::Err: fmt::Display,
    {
        self.lookup_req(name.into(), parse_str::<T>)
    }

    /// Variable with a default used only when unset. Malformed values are
    /// recorded as errors (and `default` is returned as a placeholder).
    pub fn var_or<T>(&mut self, name: impl Into<Cow<'static, str>>, default: T) -> T
    where
        T: FromStr,
        T::Err: fmt::Display,
    {
        match read_opt(name.into(), &self.file, parse_str::<T>) {
            Ok(Some(v)) => v,
            Ok(None) => default,
            Err(e) => {
                self.errors.push(e);
                default
            }
        }
    }

    /// Optional variable. `None` when unset or when malformed (the latter is
    /// also recorded as an error).
    pub fn var_opt<T>(&mut self, name: impl Into<Cow<'static, str>>) -> Option<T>
    where
        T: FromStr,
        T::Err: fmt::Display,
    {
        self.lookup_opt(name.into(), parse_str::<T>)
    }

    /// Required boolean (`true/false/1/0/yes/no/y/n/on/off`).
    pub fn var_bool(&mut self, name: impl Into<Cow<'static, str>>) -> bool {
        self.lookup_req(name.into(), parse_bool)
    }

    /// Boolean with a default used only when unset.
    pub fn var_bool_or(&mut self, name: impl Into<Cow<'static, str>>, default: bool) -> bool {
        match read_opt(name.into(), &self.file, parse_bool) {
            Ok(Some(v)) => v,
            Ok(None) => default,
            Err(e) => {
                self.errors.push(e);
                default
            }
        }
    }

    /// Required list split on `sep`. Returns an empty vector on failure and records an error.
    pub fn var_list<T>(&mut self, name: impl Into<Cow<'static, str>>, sep: char) -> Vec<T>
    where
        T: FromStr,
        T::Err: fmt::Display,
    {
        self.lookup_req(name.into(), |s| parse_list::<T>(s, sep))
    }

    /// List with a default used only when unset.
    pub fn var_list_or<T>(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        sep: char,
        default: Vec<T>,
    ) -> Vec<T>
    where
        T: FromStr,
        T::Err: fmt::Display,
    {
        match read_opt(name.into(), &self.file, |s| parse_list::<T>(s, sep)) {
            Ok(Some(v)) => v,
            Ok(None) => default,
            Err(e) => {
                self.errors.push(e);
                default
            }
        }
    }

    /// Finishes loading. Returns `Err` listing every problem found, or `Ok(())` if no errors occurred.
    #[must_use = "configuration errors are recorded in the returned Result and must be handled"]
    pub fn finish(self) -> Result<(), Errors> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(Errors(self.errors))
        }
    }
}

// .env parsing

#[cfg(feature = "dotenv")]
fn parse_dotenv(src: &str) -> (FileVars, Vec<(usize, String)>) {
    let mut vars = FileVars::new();
    let mut errs = Vec::new();
    for (i, line) in src.lines().enumerate() {
        let n = i + 1;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line
            .strip_prefix("export ")
            .map(str::trim_start)
            .unwrap_or(line);
        let Some((key, rest)) = line.split_once('=') else {
            errs.push((n, "expected KEY=VALUE".to_string()));
            continue;
        };
        let key = key.trim();
        if key.is_empty() || key.chars().any(char::is_whitespace) {
            errs.push((n, format!("invalid key `{key}`")));
            continue;
        }
        match parse_value(rest.trim()) {
            Ok(v) => {
                vars.insert(key.to_string(), v);
            }
            Err(reason) => errs.push((n, reason)),
        }
    }
    (vars, errs)
}

#[cfg(feature = "dotenv")]
fn check_trailing(t: &str) -> Result<(), String> {
    let t = t.trim();
    if t.is_empty() || t.starts_with('#') {
        Ok(())
    } else {
        Err(format!("unexpected text after closing quote: `{t}`"))
    }
}

#[cfg(feature = "dotenv")]
fn parse_value(s: &str) -> Result<String, String> {
    if s.starts_with('#') {
        return Ok(String::new());
    }
    match s.chars().next() {
        Some('"') => {
            let mut out = String::new();
            let mut escaped = false;
            let mut end = None;
            for (i, c) in s.char_indices().skip(1) {
                if escaped {
                    out.push(match c {
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        other => other,
                    });
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    end = Some(i + 1);
                    break;
                } else {
                    out.push(c);
                }
            }
            let end = end.ok_or_else(|| "unterminated double quote".to_string())?;
            check_trailing(&s[end..])?;
            Ok(out)
        }
        Some('\'') => {
            let rest = &s[1..];
            let close = rest
                .find('\'')
                .ok_or_else(|| "unterminated single quote".to_string())?;
            check_trailing(&rest[close + 1..])?;
            Ok(rest[..close].to_string())
        }
        _ => {
            let mut cut = s.len();
            let mut prev_ws = false;
            for (i, c) in s.char_indices() {
                if c == '#' && prev_ws {
                    cut = i;
                    break;
                }
                prev_ws = c.is_whitespace();
            }
            Ok(s[..cut].trim_end().to_string())
        }
    }
}

// tests

#[cfg(test)]
mod tests;
