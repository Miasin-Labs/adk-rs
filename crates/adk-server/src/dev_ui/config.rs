use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{env, fs};

const DEFAULT_MODEL: &str = "gpt-4o-mini";
const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

#[derive(Debug, Clone)]
pub struct OpenAiConfig {
    pub api_key: String,
    pub model: String,
    pub base_url: String,
}

impl OpenAiConfig {
    pub fn load() -> Option<Self> {
        let values = EnvValues::load();
        let api_key = values.get("OPENAI_API_KEY")?;
        let model = values
            .get("ADK_OPENAI_MODEL")
            .or_else(|| values.get("OPENAI_MODEL"))
            .unwrap_or_else(|| DEFAULT_MODEL.to_owned());
        let base_url = values
            .get("OPENAI_BASE_URL")
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_owned());
        Some(Self {
            api_key,
            model: model.strip_prefix("openai/").unwrap_or(&model).to_owned(),
            base_url: base_url.trim_end_matches('/').to_owned(),
        })
    }
}

struct EnvValues {
    values: BTreeMap<String, String>,
}

impl EnvValues {
    fn load() -> Self {
        let mut values = BTreeMap::new();
        for path in env_paths() {
            merge_env_file(&mut values, &path);
        }
        for key in [
            "OPENAI_API_KEY",
            "ADK_OPENAI_MODEL",
            "OPENAI_MODEL",
            "OPENAI_BASE_URL",
        ] {
            if let Ok(value) = env::var(key) {
                values.insert(key.to_owned(), value);
            }
        }
        Self { values }
    }

    fn get(&self, key: &str) -> Option<String> {
        self.values
            .get(key)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    }
}

fn env_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(path) = env::var("ADK_ENV_FILE") {
        paths.push(PathBuf::from(path));
    }
    if let Ok(cwd) = env::current_dir() {
        paths.push(cwd.join(".env"));
    }
    paths
}

fn merge_env_file(values: &mut BTreeMap<String, String>, path: &Path) {
    let Ok(contents) = fs::read_to_string(path) else {
        return;
    };
    for line in contents.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        values.insert(key.trim().to_owned(), clean_value(value));
    }
}

fn clean_value(value: &str) -> String {
    value.trim().trim_matches('"').trim_matches('\'').to_owned()
}
