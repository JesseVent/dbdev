use crate::util;

use anyhow::Context;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct ControlFileRef {
    pub filename: String,
    entries: HashMap<String, String>,
}

#[derive(Debug)]
pub struct Metadata {
    pub extension_name: String,
    pub default_version: String,
    pub comment: Option<String>,
    pub schema: Option<String>,
    pub relocatable: bool,
    pub requires: Vec<String>,
    pub repository: Option<String>,
}

impl Metadata {
    fn from_control_file_ref(control_file_ref: &ControlFileRef) -> anyhow::Result<Self> {
        Ok(Self {
            extension_name: control_file_ref.extension_name()?,
            default_version: control_file_ref.default_version()?,
            comment: control_file_ref.comment(),
            relocatable: control_file_ref.relocatable()?,
            requires: control_file_ref.requires(),
            schema: control_file_ref.schema(),
            repository: control_file_ref.repository(),
        })
    }
}

#[derive(Debug)]
pub struct InstallFile {
    pub filename: String,
    pub version: String,
    pub body: String,
}

#[derive(Debug)]
pub struct UpgradeFile {
    pub filename: String,
    pub from_version: String,
    pub to_version: String,
    pub body: String,
}

#[derive(Debug)]
pub struct ReadmeFile {
    pub body: String,
}

impl ReadmeFile {
    pub(crate) fn from_path(path: &Path) -> anyhow::Result<ReadmeFile> {
        let file_name = path
            .file_name()
            .and_then(OsStr::to_str)
            .context("Failed to read file name")?;
        let body =
            fs::read_to_string(path).context(format!("Failed to read file {}", &file_name))?;
        Ok(ReadmeFile { body })
    }

    pub(crate) fn body(&self) -> &str {
        &self.body
    }
}

#[derive(sqlx::FromRow)]
pub(crate) struct ExtensionVersion {
    pub(crate) version: String,
}

#[derive(sqlx::FromRow, PartialEq, Eq, Hash)]
pub(crate) struct UpdatePath {
    pub(crate) source: String,
    pub(crate) target: String,
}

#[derive(Debug)]
pub struct Payload {
    pub metadata: Metadata,
    pub install_files: Vec<InstallFile>,
    pub upgrade_files: Vec<UpgradeFile>,
    pub readme_file: Option<ReadmeFile>,
}

impl Payload {
    pub fn from_path(path: &Path) -> anyhow::Result<Self> {
        // Install from Path
        let abs_path = match fs::canonicalize(path) {
            Ok(abs_path) => abs_path,
            Err(e) => {
                return Err(anyhow::anyhow!("Error: {:?}", e));
            }
        };

        if !abs_path.is_dir() {
            return Err(anyhow::anyhow!("Error: *path* is not a directory"));
        }

        let mut control_files = vec![];
        let mut sql_files = vec![];
        let mut readme_file: Option<PathBuf> = None;

        for entry in fs::read_dir(&abs_path).unwrap() {
            match entry {
                Ok(dir_entry) => {
                    let entry_path = dir_entry.path();
                    if entry_path.is_dir() {
                        continue;
                    }
                    if let Some("README.md") = entry_path.file_name().and_then(OsStr::to_str) {
                        readme_file = Some(entry_path);
                        continue;
                    }
                    let extension: Option<&str> = entry_path.extension().and_then(OsStr::to_str);
                    match extension {
                        Some("control") => control_files.push(entry_path),
                        Some("sql") => sql_files.push(entry_path),
                        _ => continue,
                    }
                }
                Err(_) => continue,
            }
        }

        let readme_file = readme_file
            .map(|path| ReadmeFile::from_path(&path))
            .transpose()?;

        // /User/<abridge>/some_ext/some_ext.control
        let control_file_path = match control_files.len() {
            0 => return Err(anyhow::anyhow!("no control file detected")),
            1 => control_files
                .pop()
                .context("failed to reference control file")?,
            _ => return Err(anyhow::anyhow!("multiple control files detected")),
        };

        // some_ext
        let control_file = ControlFileRef::from_pathbuf(&control_file_path)?;

        let extension_name = control_file.extension_name()?;

        if !util::is_valid_extension_name(&extension_name) {
            return Err(anyhow::anyhow!(
                "Invalid extension name detected: {}. It must begin with an alphabet, contain only alphanumeric characters or `_` and should be between 2 and 32 characters long.",
                extension_name
            ));
        }

        // TODO: follow the some_ext.control `directory` parameter allowing sql scripts to
        // be somewhere other than the repo root
        let mut install_files = vec![];
        let mut upgrade_files = vec![];

        for path in sql_files {
            let file_name = path.file_name().and_then(OsStr::to_str).unwrap();
            let parts: Vec<&str> = file_name
                .strip_suffix(".sql")
                .unwrap()
                .split("--")
                .collect();
            match &parts[..] {
                [file_ext_name, ver] => {
                    // Make sure the file's extension name matches the control file
                    if file_ext_name != &extension_name {
                        println!("Warning: file `{file_name}` will be skipped because its extension name(`{file_ext_name}`) doesn't match `{extension_name}`");
                        continue;
                    }
                    if !util::is_valid_version(ver) {
                        println!("Warning: file `{file_name}` will be skipped because its version (`{ver}`) is invalid. It should be have the format `major.minor.patch`.");
                        continue;
                    }

                    let ifile = InstallFile {
                        filename: file_name.to_string(),
                        version: ver.to_string(),
                        body: fs::read_to_string(&path)
                            .context(format!("Failed to read file {}", &file_name))?,
                    };
                    install_files.push(ifile);
                }
                [file_ext_name, from_ver, to_ver] => {
                    // Make sure the file's extension name matches the control file
                    if file_ext_name != &extension_name {
                        println!("Warning: file `{file_name}` will be skipped because its extension name(`{file_ext_name}`) doesn't match `{extension_name}`");
                        continue;
                    }
                    if !util::is_valid_version(from_ver) {
                        println!("Warning: file `{file_name}` will be skipped because its from version(`{from_ver}`) is invalid. It should be have the format `major.minor.patch`.");
                        continue;
                    }
                    if !util::is_valid_version(to_ver) {
                        println!("Warning: file `{file_name}` will be skipped because its from version(`{to_ver}`) is invalid. It should be have the format `major.minor.patch`.");
                        continue;
                    }

                    let ufile = UpgradeFile {
                        filename: file_name.to_string(),
                        from_version: from_ver.to_string(),
                        to_version: to_ver.to_string(),
                        body: fs::read_to_string(&path)
                            .context(format!("Failed to read file {}", &file_name))?,
                    };
                    upgrade_files.push(ufile);
                }
                _ => (),
            }
        }

        let payload = Payload {
            metadata: Metadata::from_control_file_ref(&control_file)?,
            install_files,
            upgrade_files,
            readme_file,
        };
        Ok(payload)
    }
}


/// Parses a `.control` file body following the same rules as Postgres's GUC
/// file parser (`src/backend/utils/misc/guc-file.l`):
///
/// - `name = value` and `name value` are both accepted; the `=` is optional.
/// - `#` starts a comment, except inside a quoted value.
/// - Values may be single-quoted. `''` is a literal quote, and a backslash
///   emits the following character verbatim (`\n` is `n`, not a newline).
/// - An unterminated quote is an error.
fn parse_control_entries(contents: &str) -> anyhow::Result<HashMap<String, String>> {
    let mut entries = HashMap::new();

    for (lineno, line) in contents.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // The name ends at the first `=` or whitespace, whichever comes first.
        let Some(name_end) = trimmed.find(|c: char| c == '=' || c.is_whitespace()) else {
            return Err(anyhow::anyhow!(
                "line {} of control file has a parameter name with no value: `{}`",
                lineno + 1,
                trimmed
            ));
        };

        let key = trimmed[..name_end].trim().to_lowercase();
        let rest = trimmed[name_end..].trim_start();
        let rest = match rest.strip_prefix('=') {
            Some(after_eq) => after_eq.trim_start(),
            None => rest,
        };

        let value = if let Some(quoted) = rest.strip_prefix('\'') {
            let mut result = String::new();
            let mut chars = quoted.chars();
            let mut closed = false;

            while let Some(ch) = chars.next() {
                match ch {
                    '\'' => {
                        // A doubled quote is a literal quote; a lone one closes.
                        let mut lookahead = chars.clone();
                        if lookahead.next() == Some('\'') {
                            chars = lookahead;
                            result.push('\'');
                        } else {
                            closed = true;
                            break;
                        }
                    }
                    // xqescape: the escaped character is taken literally, so
                    // `\n` is the letter n rather than a newline.
                    '\\' => match chars.next() {
                        Some(escaped) => result.push(escaped),
                        None => break,
                    },
                    other => result.push(other),
                }
            }

            if !closed {
                return Err(anyhow::anyhow!(
                    "line {} of control file has an unterminated quoted value: `{}`",
                    lineno + 1,
                    trimmed
                ));
            }

            result
        } else {
            // Unquoted values run to the end of the line or the first comment.
            match rest.split_once('#') {
                Some((before_comment, _)) => before_comment.trim().to_string(),
                None => rest.trim().to_string(),
            }
        };

        entries.insert(key, value);
    }

    Ok(entries)
}

impl ControlFileRef {
    fn new(filename: String, contents: &str) -> anyhow::Result<Self> {
        Ok(Self {
            filename,
            entries: parse_control_entries(contents)?,
        })
    }

    fn from_pathbuf(path: &Path) -> anyhow::Result<Self> {
        let control_file_name = path
            .file_name()
            .and_then(OsStr::to_str)
            .context("failed to read control file name")?
            .to_string();

        let control_file_body = fs::read_to_string(path).context("failed to read control file")?;

        Self::new(control_file_name, &control_file_body)
    }

    // Name of the extension. Used in the `create extension <extension_name>`
    fn extension_name(&self) -> anyhow::Result<String> {
        self.filename
            .strip_suffix(".control")
            .context("failed to read extension name from control file")
            .map(str::to_string)
    }

    // A comment (any string) about the extension.
    fn comment(&self) -> Option<String> {
        self.entries.get("comment").cloned()
    }

    // A list of names of extensions that this extension depends on
    fn requires(&self) -> Vec<String> {
        match self.entries.get("requires") {
            Some(val) => val
                .split(',')
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect(),
            None => vec![],
        }
    }

    // The schema the extension wants to be installed in, if any
    fn schema(&self) -> Option<String> {
        self.entries.get("schema").cloned()
    }

    // The home repository or homepage URL for the extension
    fn repository(&self) -> Option<String> {
        self.entries
            .get("repository")
            .or_else(|| self.entries.get("homepage"))
            .or_else(|| self.entries.get("repository_url"))
            .cloned()
    }

    fn relocatable(&self) -> anyhow::Result<bool> {
        match self.entries.get("relocatable") {
            Some(val) => match val.to_lowercase().as_str() {
                "true" | "yes" | "on" | "1" => Ok(true),
                "false" | "no" | "off" | "0" => Ok(false),
                other => other
                    .parse::<bool>()
                    .context("invalid boolean for relocatable"),
            },
            None => Ok(false),
        }
    }

    fn default_version(&self) -> anyhow::Result<String> {
        match self.entries.get("default_version") {
            Some(val) if !val.is_empty() => Ok(val.clone()),
            _ => Err(anyhow::anyhow!(
                "`default_version` in control file is required"
            )),
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn control_file(contents: &str) -> anyhow::Result<ControlFileRef> {
        ControlFileRef::new("my_ext.control".to_string(), contents)
    }

    #[test]
    fn test_control_file_parsing_robust() {
        let control_content = r#"
            # PostgreSQL extension control file
            # Comment line with leading spaces
            comment = 'A great extension with ''escaped'' quotes' # inline comment
            default_version = '1.2.3'
            relocatable = true
            requires = 'pg_net,  supabase_vault , pg_graphql'
            schema = 'public'
            repository = 'https://github.com/supabase/my_ext'
        "#;

        let control_file = control_file(control_content).unwrap();

        assert_eq!(control_file.extension_name().unwrap(), "my_ext");
        assert_eq!(
            control_file.comment().unwrap(),
            "A great extension with 'escaped' quotes"
        );
        assert_eq!(control_file.default_version().unwrap(), "1.2.3");
        assert_eq!(control_file.relocatable().unwrap(), true);
        assert_eq!(
            control_file.requires(),
            vec!["pg_net", "supabase_vault", "pg_graphql"]
        );
        assert_eq!(control_file.schema().unwrap(), "public");
        assert_eq!(
            control_file.repository().unwrap(),
            "https://github.com/supabase/my_ext"
        );
    }

    #[test]
    fn test_control_file_boolean_variants() {
        let bool_tests = [
            ("relocatable = yes", true),
            ("relocatable = ON", true),
            ("relocatable = 1", true),
            ("relocatable = 'true'", true),
            ("relocatable = no", false),
            ("relocatable = off", false),
            ("relocatable = 0", false),
            ("relocatable = 'false'", false),
        ];

        for (line, expected) in bool_tests {
            let cf = control_file(&format!("default_version = '1.0.0'\n{}", line)).unwrap();
            assert_eq!(cf.relocatable().unwrap(), expected, "Failed for {}", line);
        }
    }

    // guc-file.l treats `=` as optional between a parameter and its value.
    #[test]
    fn test_equals_sign_is_optional() {
        let cf = control_file("default_version '1.0.0'\nschema 'public'").unwrap();

        assert_eq!(cf.default_version().unwrap(), "1.0.0");
        assert_eq!(cf.schema().unwrap(), "public");
    }

    // guc-file.l's xqescape emits the character after the backslash verbatim,
    // so `\n` is the letter n rather than a newline.
    #[test]
    fn test_backslash_escapes_are_literal() {
        let cf = control_file(r#"comment = 'line\none\ttwo\\three\'four'"#).unwrap();

        assert_eq!(cf.comment().unwrap(), r#"linenonettwo\three'four"#);
    }

    // An unterminated quote is an error in guc-file.l, not a value to salvage.
    #[test]
    fn test_unterminated_quote_is_an_error() {
        let err = control_file("comment = 'never closed").unwrap_err();

        assert!(
            err.to_string().contains("unterminated quoted value"),
            "unexpected error: {}",
            err
        );
    }

    // A `#` inside a quoted value is content; outside one it starts a comment.
    #[test]
    fn test_comment_handling() {
        let cf = control_file("comment = 'has # inside'\nschema = public # trailing").unwrap();

        assert_eq!(cf.comment().unwrap(), "has # inside");
        assert_eq!(cf.schema().unwrap(), "public");
    }
}
