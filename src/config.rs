use crate::{adapters::custom::CustomAdapterConfig, project_dirs};
use anyhow::{Result, Context};
use derive_more::FromStr;
use log::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::{fs::File, io::Write, iter::IntoIterator, path::PathBuf, str::FromStr};
use structopt::StructOpt;

#[derive(Debug, Deserialize, Serialize)]
struct ReadableBytesCount(i64);

fn is_default<T: Default + PartialEq>(t: &T) -> bool {
    t == &T::default()
}
#[derive(JsonSchema, Debug, Serialize, Deserialize, Copy, Clone, PartialEq, FromStr)]
pub struct CacheCompressionLevel(pub i32);

impl ToString for CacheCompressionLevel {
    fn to_string(&self) -> String {
        self.0.to_string()
    }
}
impl Default for CacheCompressionLevel {
    fn default() -> Self {
        CacheCompressionLevel(12)
    }
}
#[derive(JsonSchema, Debug, Serialize, Deserialize, Copy, Clone, PartialEq, FromStr)]
pub struct MaxArchiveRecursion(pub i32);

impl ToString for MaxArchiveRecursion {
    fn to_string(&self) -> String {
        self.0.to_string()
    }
}
impl Default for MaxArchiveRecursion {
    fn default() -> Self {
        MaxArchiveRecursion(4)
    }
}

#[derive(JsonSchema, Debug, Serialize, Deserialize, Clone, PartialEq, FromStr)]
pub struct CachePath(pub String);

impl ToString for CachePath {
    fn to_string(&self) -> String {
        self.0.to_string()
    }
}
impl Default for CachePath {
    fn default() -> Self {
        let pd = project_dirs().expect("could not get cache path");
        let app_cache = pd.cache_dir();
        CachePath(app_cache.to_str().expect("cache path not utf8").to_owned())
    }
}

#[derive(JsonSchema, Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq)]
pub struct CacheMaxBlobLen(pub usize);

impl ToString for CacheMaxBlobLen {
    fn to_string(&self) -> String {
        self.0.to_string()
    }
}
impl Default for CacheMaxBlobLen {
    fn default() -> Self {
        CacheMaxBlobLen(2000000)
    }
}

impl FromStr for CacheMaxBlobLen {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let suffix = s.chars().last();
        if let Some(suffix) = suffix {
            Ok(CacheMaxBlobLen(match suffix {
                'k' | 'M' | 'G' => usize::from_str(s.trim_end_matches(suffix))
                    .with_context(|| format!("Could not parse int"))
                    .map(|e| {
                        e * match suffix {
                            'k' => 1000,
                            'M' => 1000_000,
                            'G' => 1000_000_000,
                            _ => panic!("impossible"),
                        }
                    }),
                _ => usize::from_str(s).with_context(|| format!("Could not parse int")),
            }?))
        } else {
            Err(anyhow::format_err!("empty byte input"))
        }
    }
}

/// # rga configuration
///
/// this is kind of a "polyglot" struct, since it serves three functions
///
/// 1. describing the command line arguments using structopt+clap and for man page / readme generation
/// 2. describing the config file format (output as JSON schema via schemars)
#[derive(StructOpt, Debug, Deserialize, Serialize, JsonSchema, Default, Clone)]
#[structopt(
    name = "ripgrep-all",
    rename_all = "kebab-case",
    about = env!("CARGO_PKG_DESCRIPTION"),
    author = env!("CARGO_PKG_HOMEPAGE"),
    // TODO: long_about does not seem to work to only show this on short help
    after_help = "-h shows a concise overview, --help shows more detail and advanced options.\n\nAll other options not shown here are passed directly to rg, especially [PATTERN] and [PATH ...]",
    usage = "rga [RGA OPTIONS] [RG OPTIONS] PATTERN [PATH ...]"
)]
pub struct RgaConfig {
    /// Use more accurate but slower matching by mime type
    ///
    /// By default, rga will match files using file extensions.
    /// Some programs, such as sqlite3, don't care about the file extension at all,
    /// so users sometimes use any or no extension at all. With this flag, rga
    /// will try to detect the mime type of input files using the magic bytes
    /// (similar to the `file` utility), and use that to choose the adapter.
    /// Detection is only done on the first 8KiB of the file, since we can't always seek on the input (in archives).
    #[serde(default, skip_serializing_if = "is_default")]
    #[structopt(long = "--rga-accurate")]
    pub accurate: bool,

    /// Change which adapters to use and in which priority order (descending)
    ///
    /// "foo,bar" means use only adapters foo and bar.
    /// "-bar,baz" means use all default adapters except for bar and baz.
    /// "+bar,baz" means use all default adapters and also bar and baz.
    #[serde(default, skip_serializing_if = "is_default")]
    #[structopt(
        long = "--rga-adapters",
        require_equals = true,
        require_delimiter = true
    )]
    pub adapters: Vec<String>,

    #[serde(default, skip_serializing_if = "is_default")]
    #[structopt(flatten)]
    pub cache: CacheConfig,

    /// Maximum nestedness of archives to recurse into
    #[serde(default, skip_serializing_if = "is_default")]
    #[structopt(
        default_value,
        long = "--rga-max-archive-recursion",
        require_equals = true,
        hidden_short_help = true
    )]
    pub max_archive_recursion: MaxArchiveRecursion,

    //////////////////////////////////////////
    //////////////////////////// Config file only
    //////////////////////////////////////////
    #[serde(default, skip_serializing_if = "is_default")]
    #[structopt(skip)]
    pub custom_adapters: Option<Vec<CustomAdapterConfig>>,
    //////////////////////////////////////////
    //////////////////////////// CMD line only
    //////////////////////////////////////////
    #[serde(skip)]
    #[structopt(long = "--rga-config-file", require_equals = true)]
    pub config_file_path: Option<String>,

    /// same as passing path directly, except if argument is empty
    /// kinda hacky, but if no file is found, fzf calls rga with empty string as path, which causes No such file or directory from rg. So filter those cases and return specially
    #[serde(skip)]
    #[structopt(long = "--rga-fzf-path", require_equals = true, hidden = true)]
    pub fzf_path: Option<String>,

    // these arguments are basically "subcommands" that stop the process, so don't serialize them
    #[serde(skip)]
    #[structopt(long = "--rga-list-adapters", help = "List all known adapters")]
    pub list_adapters: bool,

    #[serde(skip)]
    #[structopt(
        long = "--rga-print-config-schema",
        help = "Print the JSON Schema of the configuration file"
    )]
    pub print_config_schema: bool,

    #[serde(skip)]
    #[structopt(long, help = "Show help for ripgrep itself")]
    pub rg_help: bool,

    #[serde(skip)]
    #[structopt(long, help = "Show version of ripgrep itself")]
    pub rg_version: bool,
}

#[derive(StructOpt, Debug, Deserialize, Serialize, JsonSchema, Default, Clone, PartialEq)]
pub struct CacheConfig {
    /// Disable caching of results
    ///
    /// By default, rga caches the extracted text, if it is small enough,
    /// to a database in ~/.cache/rga on Linux,
    /// ~/Library/Caches/rga on macOS,
    /// or C:\Users\username\AppData\Local\rga on Windows.
    /// This way, repeated searches on the same set of files will be much faster.
    /// If you pass this flag, all caching will be disabled.
    #[serde(default, skip_serializing_if = "is_default")]
    #[structopt(long = "--rga-no-cache")]
    pub disabled: bool,

    /// Max compressed size to cache
    ///
    /// Longest byte length (after compression) to store in cache. Longer adapter outputs will not be cached and recomputed every time. Allowed suffixes: k M G
    #[serde(default, skip_serializing_if = "is_default")]
    #[structopt(
        default_value,
        long = "--rga-cache-max-blob-len",
        hidden_short_help = true,
        require_equals = true,
        // parse(try_from_str = parse_readable_bytes_str)
    )]
    pub max_blob_len: CacheMaxBlobLen,

    /// ZSTD compression level to apply to adapter outputs before storing in cache db
    ///
    ///  Ranges from 1 - 22
    #[serde(default, skip_serializing_if = "is_default")]
    #[structopt(
        default_value,
        long = "--rga-cache-compression-level",
        hidden_short_help = true,
        require_equals = true,
        help = ""
    )]
    pub compression_level: CacheCompressionLevel,

    /// Path to store cache db
    #[serde(default, skip_serializing_if = "is_default")]
    #[structopt(
        default_value,
        long = "--rga-cache-path",
        hidden_short_help = true,
        require_equals = true,
    )]
    pub path: CachePath,
}

static RGA_CONFIG: &str = "RGA_CONFIG";

use serde_json::Value;
fn json_merge(a: &mut Value, b: &Value) {
    match (a, b) {
        (&mut Value::Object(ref mut a), &Value::Object(ref b)) => {
            for (k, v) in b {
                json_merge(a.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
        (a, b) => {
            *a = b.clone();
        }
    }
}

fn read_config_file(path_override: Option<String>) -> Result<(String, Value)> {
    let proj = project_dirs()?;
    let config_dir = proj.config_dir();
    let config_filename = path_override
        .as_ref()
        .map(|e| PathBuf::from(e))
        .unwrap_or(config_dir.join("config.jsonc"));
    let config_filename_str = config_filename.to_string_lossy().into_owned();
    if config_filename.exists() {
        let config_file_contents = std::fs::read_to_string(config_filename)
            .with_context(|| format!("Could not read config file json {}", config_filename_str))?;
        {
            // just for error messages
            serde_json::from_str::<RgaConfig>(&config_file_contents).with_context(|| {
                format!(
                    "Error in config file {}: {}",
                    config_filename_str, config_file_contents
                )
            })?;
        }
        let config_json: serde_json::Value =
            serde_json::from_str(&config_file_contents).context("Could not parse config json")?;
        Ok((config_filename_str, config_json))
    } else if let Some(p) = path_override.as_ref() {
        Err(anyhow::anyhow!("Config file not found: {}", p))?
    } else {
        // write default config
        std::fs::create_dir_all(config_dir)?;
        let mut schemafile = File::create(config_dir.join("config.schema.json"))?;

        schemafile
            .write(serde_json::to_string_pretty(&schemars::schema_for!(RgaConfig))?.as_bytes())?;

        let mut config_json = serde_json::to_value(&RgaConfig::default())?;
        match &mut config_json {
            serde_json::Value::Object(o) => {
                o.insert(
                    "$schema".to_string(),
                    serde_json::Value::String("./config.schema.json".to_string()),
                );
            }
            _ => panic!("impos"),
        }
        let mut configfile = File::create(config_filename)?;
        configfile.write(serde_json::to_string_pretty(&config_json)?.as_bytes())?;
        Ok((config_filename_str, config_json))
    }
}
fn read_config_env() -> Result<Value> {
    let val = std::env::var(RGA_CONFIG).ok();
    if let Some(val) = val {
        serde_json::from_str(&val).context("could not parse config from env RGA_CONFIG")
    } else {
        serde_json::to_value(&RgaConfig::default()).context("could not create default config")
    }
}
pub fn parse_args<I>(args: I, is_rga_preproc: bool) -> Result<RgaConfig>
where
    I: IntoIterator,
    I::Item: Into<OsString> + Clone,
{
    // TODO: don't read config file in rga-preproc for performance (called for every file)

    let arg_matches: RgaConfig = RgaConfig::from_iter(args);
    let args_config = serde_json::to_value(&arg_matches)?;

    let merged_config = {
        if is_rga_preproc {
            // only read from env and args
            let mut merged_config = read_config_env()?;
            json_merge(&mut merged_config, &args_config);
            log::debug!("Config: {}", serde_json::to_string(&merged_config)?);
            merged_config
        } else {
            // read from config file, env and args
            let (config_filename, config_file_config) =
                read_config_file(arg_matches.config_file_path)?;
            let env_var_config = read_config_env()?;
            let mut merged_config = config_file_config.clone();
            json_merge(&mut merged_config, &env_var_config);
            json_merge(&mut merged_config, &args_config);
            log::debug!(
                "Configs:\n{}: {}\n{}: {}\nArgs: {}\nMerged: {}",
                config_filename,
                serde_json::to_string_pretty(&config_file_config)?,
                RGA_CONFIG,
                serde_json::to_string_pretty(&env_var_config)?,
                serde_json::to_string_pretty(&args_config)?,
                serde_json::to_string_pretty(&merged_config)?
            );
            // pass to child processes
            std::env::set_var(RGA_CONFIG, &merged_config.to_string());
            merged_config
        }
    };

    let mut res: RgaConfig = serde_json::from_value(merged_config.clone())
        .map_err(|e| {
            println!("{:?}", e);
            e
        })
        .with_context(|| {
            format!(
                "Error parsing merged config: {}",
                serde_json::to_string_pretty(&merged_config).expect("no tostring")
            )
        })?;
    {
        // readd values with [serde(skip)]
        res.fzf_path = arg_matches.fzf_path;
        res.list_adapters = arg_matches.list_adapters;
        res.print_config_schema = arg_matches.print_config_schema;
        res.rg_help = arg_matches.rg_help;
        res.rg_version = arg_matches.rg_version;
    }
    Ok(res)
}

/// Split arguments into the ones we care about and the ones rg cares about
pub fn split_args(is_rga_preproc: bool) -> Result<(RgaConfig, Vec<OsString>)> {
    let mut app = RgaConfig::clap();

    app.p.create_help_and_version();
    let mut firstarg = true;
    // debug!("{:#?}", app.p.flags);
    let (our_args, mut passthrough_args): (Vec<OsString>, Vec<OsString>) = std::env::args_os()
        .partition(|os_arg| {
            if firstarg {
                // hacky, but .enumerate() would be ugly because partition is too simplistic
                firstarg = false;
                return true;
            }
            if let Some(arg) = os_arg.to_str() {
                arg.starts_with("--rga-")
                    || arg.starts_with("--rg-")
                    || arg == "--help"
                    || arg == "-h"
                    || arg == "--version"
                    || arg == "-V"
            } else {
                // args that are not unicode can only be filenames, pass them to rg
                false
            }
        });
    debug!("rga (our) args: {:?}", our_args);
    let matches = parse_args(our_args, is_rga_preproc).context("Could not parse config")?;
    if matches.rg_help {
        passthrough_args.insert(0, "--help".into());
    }
    if matches.rg_version {
        passthrough_args.insert(0, "--version".into());
    }
    debug!("rga (passthrough) args: {:?}", passthrough_args);
    Ok((matches, passthrough_args))
}
