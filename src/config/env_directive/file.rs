use crate::config::env_directive::EnvResults;
use crate::file::display_path;
use crate::{file, sops, Result};
use eyre::{eyre, WrapErr};
use indexmap::IndexMap;
use rops::file::format::{JsonFileFormat, YamlFileFormat};
use std::path::{Path, PathBuf};

// use indexmap so source is after value for `mise env --json` output
type EnvMap = IndexMap<String, String>;

#[derive(serde::Serialize, serde::Deserialize)]
struct Env<V> {
    #[serde(default)]
    sops: IndexMap<String, V>,
    #[serde(flatten)]
    env: EnvMap,
}

impl EnvResults {
    pub fn file(
        ctx: &mut tera::Context,
        env: &mut IndexMap<String, (String, Option<PathBuf>)>,
        r: &mut EnvResults,
        normalize_path: fn(&Path, PathBuf) -> PathBuf,
        source: &Path,
        config_root: &Path,
        input: PathBuf,
    ) -> Result<()> {
        let s = r.parse_template(ctx, source, input.to_string_lossy().as_ref())?;
        for p in xx::file::glob(normalize_path(config_root, s.into())).unwrap_or_default() {
            r.env_files.push(p.clone());
            let parse_template = |s: String| r.parse_template(ctx, source, &s);
            let ext = p
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_default();
            let new_vars = match ext.as_str() {
                "json" => Self::json(&p, parse_template)?,
                "yaml" => Self::yaml(&p, parse_template)?,
                _ => Self::dotenv(&p)?,
            };
            for (k, v) in new_vars {
                r.env_remove.remove(&k);
                env.insert(k, (v, Some(p.clone())));
            }
        }
        Ok(())
    }

    fn json<PT>(p: &Path, parse_template: PT) -> Result<EnvMap>
    where
        PT: Fn(String) -> Result<String>,
    {
        let errfn = || eyre!("failed to parse json file: {}", display_path(p));
        if let Ok(raw) = file::read_to_string(p) {
            let mut f: Env<serde_json::Value> = serde_json::from_str(&raw).wrap_err_with(errfn)?;
            if !f.sops.is_empty() {
                let raw = sops::decrypt::<_, JsonFileFormat>(&raw, parse_template)?;
                f = serde_json::from_str(&raw).wrap_err_with(errfn)?;
            }
            Ok(f.env)
        } else {
            Ok(EnvMap::new())
        }
    }

    fn yaml<PT>(p: &Path, parse_template: PT) -> Result<EnvMap>
    where
        PT: Fn(String) -> Result<String>,
    {
        let errfn = || eyre!("failed to parse yaml file: {}", display_path(p));
        if let Ok(raw) = file::read_to_string(p) {
            let mut f: Env<serde_yaml::Value> = serde_yaml::from_str(&raw).wrap_err_with(errfn)?;
            if !f.sops.is_empty() {
                let raw = sops::decrypt::<_, YamlFileFormat>(&raw, parse_template)?;
                f = serde_yaml::from_str(&raw).wrap_err_with(errfn)?;
            }
            Ok(f.env)
        } else {
            Ok(EnvMap::new())
        }
    }

    fn dotenv(p: &Path) -> Result<EnvMap> {
        let errfn = || eyre!("failed to parse dotenv file: {}", display_path(p));
        let mut env = EnvMap::new();
        if let Ok(dotenv) = dotenvy::from_path_iter(p) {
            for item in dotenv {
                let (k, v) = item.wrap_err_with(errfn)?;
                env.insert(k, v);
            }
        }
        Ok(env)
    }
}