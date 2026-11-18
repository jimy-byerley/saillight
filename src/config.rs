//! settings remembered between runs: depth of the chart, current folder.
//!
//! kept in `~/.config/saillight/config.ini`, made of `[section]` lines followed by
//! `key=value` lines

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct Config {
    /// file the values are saved to, none to keep them in memory only
    path: Option<PathBuf>,
    sections: BTreeMap<String, BTreeMap<String, String>>,
}

/// folder holding the configuration of the app
pub fn default_folder() -> PathBuf {
    let folder = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|folder| folder.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    folder.join("saillight")
}

/// where the configuration of the user is
pub fn default_path() -> PathBuf {
    default_folder().join("config.ini")
}

fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\n', "\\n")
}

fn unescape(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut characters = value.chars();
    while let Some(character) = characters.next() {
        match character {
            '\\' => match characters.next() {
                Some('n') => result.push('\n'),
                Some(other) => result.push(other),
                None => {}
            },
            other => result.push(other),
        }
    }
    result
}

impl Config {
    /// read the file, starting empty when it is missing or unreadable
    pub fn load(path: &Path) -> Self {
        let text = fs::read_to_string(path).unwrap_or_default();
        Config { path: Some(path.to_owned()), sections: parse(&text) }
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections.get(section)?.get(key).map(String::as_str)
    }

    /// set a value and save. failures to save are ignored: the app works without memory
    pub fn set(&mut self, section: &str, key: &str, value: &str) {
        let values = self.sections.entry(section.to_owned()).or_default();
        if values.get(key).map(String::as_str) != Some(value) {
            values.insert(key.to_owned(), value.to_owned());
            self.save();
        }
    }

    fn text(&self) -> String {
        let mut text = String::new();
        for (name, values) in &self.sections {
            text.push_str(&format!("[{name}]\n"));
            for (key, value) in values {
                text.push_str(&format!("{key}={}\n", escape(value)));
            }
            text.push('\n');
        }
        text
    }

    fn save(&self) {
        let Some(path) = &self.path else { return };
        let write = || -> std::io::Result<()> {
            if let Some(folder) = path.parent() {
                fs::create_dir_all(folder)?;
            }
            // written aside then renamed, so a crash never leaves half a file
            let aside = path.with_extension("ini.new");
            let _ = fs::remove_file(&aside);
            let mut file =
                fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&aside)?;
            file.write_all(self.text().as_bytes())?;
            fs::rename(&aside, path)
        };
        let _ = write();
    }
}

fn parse(text: &str) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut sections: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut current = None;
    for line in text.lines() {
        if let Some(name) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
            sections.entry(name.to_owned()).or_default();
            current = Some(name.to_owned());
        } else if let (Some(section), Some((key, value))) = (&current, line.split_once('=')) {
            sections.entry(section.clone()).or_default().insert(key.to_owned(), unescape(value));
        }
    }
    sections
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    #[test]
    fn values_survive_a_reload() {
        let temp = TempDir::new();
        let path = temp.path().join("deep/config.ini");
        let mut config = Config::load(&path);
        assert_eq!(config.get("chart", "depth"), None);
        config.set("chart", "depth", "4");
        config.set("chart", "folder", "/home/user/odd = name\nwith \\ things");

        let config = Config::load(&path);
        assert_eq!(config.get("chart", "depth"), Some("4"));
        assert_eq!(config.get("chart", "folder"), Some("/home/user/odd = name\nwith \\ things"));
        assert_eq!(config.get("chart", "missing"), None);
        assert_eq!(config.get("missing", "depth"), None);
    }

    #[test]
    fn a_value_can_be_changed() {
        let temp = TempDir::new();
        let path = temp.path().join("config.ini");
        let mut config = Config::load(&path);
        config.set("chart", "depth", "4");
        config.set("chart", "depth", "2");
        assert_eq!(Config::load(&path).get("chart", "depth"), Some("2"));
    }

    #[test]
    fn a_broken_file_is_an_empty_configuration() {
        let temp = TempDir::new();
        let path = temp.path().join("config.ini");
        fs::write(&path, b"no section\n=\n[chart\ndepth\n\xff\xfe").unwrap();
        let config = Config::load(&path);
        assert_eq!(config.get("chart", "depth"), None);
    }

    #[test]
    fn without_a_file_values_stay_in_memory() {
        let mut config = Config::default();
        config.set("chart", "depth", "5");
        assert_eq!(config.get("chart", "depth"), Some("5"));
    }
}
