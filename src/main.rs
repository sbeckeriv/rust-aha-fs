extern crate dirs;
extern crate dotenv;
extern crate envy;
extern crate termion;
#[macro_use]
extern crate failure;
extern crate env_logger;
extern crate log;
extern crate reqwest;
#[macro_use]
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;
extern crate structopt;
#[macro_use]
extern crate prettytable;
extern crate netfuse;
extern crate notify_rust;
extern crate regex;
use fuse::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, Request,
};
use libc::ENOENT;
use netfuse::MountOptions;
use netfuse::{DirEntry, LibcError, Metadata, NetworkFilesystem};
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;
use std::path::PathBuf;
use structopt::StructOpt;
use time::Timespec;
extern crate libc;
extern crate time;
mod aha;
mod github;

use serde::Deserialize;
use serde_json::Value;

#[derive(StructOpt, Debug)]
pub struct Opt {
    #[structopt(short = "r", long = "repo", name = "repo")]
    repo: Option<String>,
    #[structopt(short = "d", long = "dryrun")]
    dry_run: bool,
    #[structopt(short = "s", long = "silent")]
    silent: bool,
    #[structopt(short = "v", long = "verbose")]
    verbose: bool,
    #[structopt(short = "c", long = "config")]
    config_file: Option<String>,
    #[structopt(short = "g", long = "generate")]
    generate: bool,
    #[structopt(short = "p", long = "prs")]
    pr_status: bool,
    #[structopt(long = "closed")]
    closed: bool,
}
#[derive(Debug, Deserialize)]
struct Config {
    aha: Option<AhaConfig>,
    global_integer: Option<u64>,
    repos: Option<Vec<RepoConfig>>,
}

#[derive(Debug, Deserialize)]
struct RepoConfig {
    name: String,
    username: String,
    labels: Option<HashMap<String, String>>,
}
#[derive(Debug, Deserialize)]
struct AhaConfig {
    domain: String,
    email: String,
}

#[derive(Deserialize, Debug)]
struct Env {
    github_api_token: String,
    aha_domain: String,
    aha_token: String,
    workflow_repo: String,
    workflow_login: String,
    workflow_email: String,
}
use lazy_static::lazy_static;
lazy_static! {
    static ref AHACONFIG: (Env, Opt) = load_config().unwrap();
}

fn load_config() -> Result<(Env, Opt), Box<dyn Error>> {
    //copied config
    let opt = Opt::from_args();
    if opt.verbose {
        println!("{:?}", opt);
    }
    let home_dir = dirs::home_dir().expect("Could not find home path");

    let path_name = match &opt.config_file {
        Some(path) => path.clone(),
        None => format!("{}/.aha_workflow", home_dir.display()),
    };

    if opt.verbose {
        println!("{:?}", path_name);
    }
    let config_path = fs::canonicalize(&path_name);
    let config_info: Option<Config> = match config_path {
        Ok(path) => {
            if opt.verbose {
                println!("found {:?}", path_name);
            }
            let display = path.display();
            let mut file = match File::open(&path) {
                Err(why) => panic!("couldn't open {}: {}", display, why.description()),
                Ok(file) => file,
            };

            // Read the file contents into a string, returns `io::Result<usize>`
            let mut s = String::new();
            match file.read_to_string(&mut s) {
                Err(why) => panic!("couldn't read {}: {}", display, why.description()),
                Ok(_) => (),
            }
            Some(toml::from_str(&s)?)
        }
        Err(e) => {
            if !opt.silent {
                println!("did not find {:?}, {}", path_name, e);
            }
            None
        }
    };

    //dotenv::dotenv().ok();
    let my_path = format!("{}/.env", home_dir.display());
    dotenv::from_path(my_path).ok();
    env_logger::init();

    let mut config: Env = envy::from_env()?;

    match config_info.as_ref() {
        Some(c) => match c.aha.as_ref() {
            Some(a) => {
                config.aha_domain = a.domain.clone();
                config.workflow_email = a.email.clone();
            }
            _ => (),
        },
        _ => (),
    }

    if opt.verbose {
        println!("config updated");
    }

    Ok((config, opt))
}
struct AhaFS {
    products: HashMap<String, String>,
    releases: HashMap<String, String>,
    features: HashMap<String, String>,
    feature_values: HashMap<String, Value>,
    epics: HashMap<String, String>,
}
impl AhaFS {
    pub fn mount(options: MountOptions) {
        let afs = AhaFS {
            products: HashMap::new(),
            releases: HashMap::new(),
            features: HashMap::new(),
            feature_values: HashMap::new(),
            epics: HashMap::new(),
        };
        netfuse::mount(afs, options);
    }
}

fn build_dir_entry(item: &Value, path_string: &str) -> DirEntry {
    dbg!(item, path_string);
    if path_string.ends_with("/features") {
        let meta = Metadata {
            size: item["description"]["body"]
                .as_str()
                .unwrap()
                .to_string()
                .as_bytes()
                .len() as u64,
            atime: DEFAULT_TIME,
            mtime: DEFAULT_TIME,
            ctime: DEFAULT_TIME,
            crtime: DEFAULT_TIME,
            kind: FileType::RegularFile,
            perm: 0o640,
        };
        DirEntry::new(item["name"].as_str().expect("file has no name"), meta)
    } else {
        let meta = Metadata {
            size: 0,
            atime: DEFAULT_TIME,
            mtime: DEFAULT_TIME,
            ctime: DEFAULT_TIME,
            crtime: DEFAULT_TIME,
            kind: FileType::Directory,
            // TODO: API should indicate if dir is listable or not
            perm: 0o750,
        };
        DirEntry::new(item["name"].as_str().expect("dir has no name"), meta)
    }
}

fn basic_dir_entry(path: &str, perm: u16) -> DirEntry {
    let meta = Metadata {
        size: 0,
        atime: DEFAULT_TIME,
        mtime: DEFAULT_TIME,
        ctime: DEFAULT_TIME,
        crtime: DEFAULT_TIME,
        kind: FileType::Directory,
        perm,
    };
    DirEntry::new(path, meta)
}

// 2015-03-12 00:00 PST Algorithmia Launch
pub const DEFAULT_TIME: Timespec = Timespec {
    sec: 1426147200,
    nsec: 0,
};

macro_rules! eio {
    ($fmt:expr) => {{
        println!($fmt);
        Err(libc::EIO)
    }};
    ($fmt:expr, $($arg:tt)*) => {{
        println!($fmt, $($arg)*);
        Err(libc::EIO)
    }};
}

impl NetworkFilesystem for AhaFS {
    fn readdir(&mut self, path: &Path) -> Box<dyn Iterator<Item = Result<DirEntry, LibcError>>> {
        let uri = match path_to_uri(&path) {
            Ok(u) => u,
            Err(_) => {
                // The default root listing
                return Box::new(vec![Ok(basic_dir_entry("/data", 0o550))].into_iter());
            }
        };

        println!("AFS readdir:  {} -> {}", path.display(), uri);
        let aha = aha::Aha::new(
            AHACONFIG.0.aha_domain.clone(),
            AHACONFIG.0.aha_token.clone(),
            AHACONFIG.0.workflow_email.clone(),
            &AHACONFIG.1,
        );
        let path_string = path.display().to_string();
        let mut count = path_string.matches("/").count();
        if count == 3 {
            return Box::new(
                vec![
                    Ok(basic_dir_entry("epics", 0o750)),
                    Ok(basic_dir_entry("features", 0o750)),
                ]
                .into_iter(),
            );
        }
        let mut parent_dir = path_string.clone().to_string();
        if count > 2 {
            parent_dir = path_string.rsplitn(2, "/").last().unwrap().to_string();
        }
        dbg!(count, &parent_dir);
        let parent = match count {
            2 => self.products.get(&parent_dir.to_string()),
            4 => self.releases.get(&parent_dir.to_string()),
            _ => None,
        };
        let dir = aha.get_uri(&path_string, parent);
        for x in &dir {
            dbg!(&x);
            let key = format!("{}/{}", path_string, x["name"].as_str().unwrap());
            let value = x["id"].as_str().unwrap().to_string();
            dbg!(&key, &value);
            match count {
                1 => {
                    self.products.insert(key, value);
                }

                2 => {
                    self.releases.insert(key, value);
                }

                4 => {
                    self.features.insert(key, value.clone());
                    self.feature_values.insert(value, x.clone());
                }
                _ => (),
            };
        }
        let iter = dir
            .iter()
            .map(|child| Ok(build_dir_entry(&child, &path_string)));
        let hack = iter.collect::<Vec<_>>().into_iter();
        Box::new(hack)
    }

    fn lookup(&mut self, path: &Path) -> Result<Metadata, LibcError> {
        if valid_connector(&path) {
            let uri = path_to_uri(&path)?;
            println!("AFS lookup: {} -> {}", path.display(), uri);
            /*
            match self.client.data(&uri).into_type() {
                Ok(data_item) => Ok(build_dir_entry(&data_item).metadata),
                Err(err) => eio!("AFS lookup error: {}", err),
            }
            */

            Err(ENOENT)
        } else {
            Err(ENOENT)
        }
    }

    fn read(&mut self, path: &Path, buffer: &mut Vec<u8>) -> Result<usize, LibcError> {
        let uri = path_to_uri(&path)?;
        println!("AFS read: {} -> {}", path.display(), uri);
        match self.features.get(&path.display().to_string()) {
            Some(feature_id) => {
                let feature = self.feature_values.get(feature_id).unwrap();
                let bytes = feature["description"]["body"]
                    .as_str()
                    .unwrap()
                    .as_bytes()
                    .read_to_end(buffer)
                    .expect("failed to read response bytes");
                Ok(bytes as usize)
            }
            None => eio!("AFS read error: {}", libc::EPERM),
        }
    }
}

pub fn valid_connector(path: &Path) -> bool {
    let mut iter = path.components();
    if path.has_root() {
        let _ = iter.next();
    }

    match iter.next().map(|c| c.as_os_str().to_string_lossy()) {
        Some(p) => p == "data" || p.starts_with("dropbox") || p.starts_with("s3"),
        _ => false,
    }
}

pub fn path_to_uri(path: &Path) -> Result<String, LibcError> {
    let mut iter = path.components();
    if path.has_root() {
        let _ = iter.next();
    }

    let protocol = match iter.next() {
        Some(p) => p.as_os_str(),
        None => {
            return Err(libc::EPERM);
        }
    };
    let uri_path = iter.as_path();
    Ok(format!(
        "{}://{}",
        protocol.to_string_lossy(),
        uri_path.to_string_lossy()
    ))
}

pub fn uri_to_path(uri: &str) -> PathBuf {
    uri.splitn(2, "://")
        .fold(Path::new("/").to_owned(), |acc, p| acc.join(Path::new(p)))
}

fn main() -> Result<(), Box<dyn Error>> {
    let mnt = "/tmp/ahafs".to_string();
    let options = netfuse::MountOptions::new(&mnt);
    AhaFS::mount(options);
    Ok(())
}
