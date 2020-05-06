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
extern crate notify_rust;
extern crate regex;
use fuse::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, Request,
};
use libc::{ENOENT, ENOSYS};
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;
use structopt::StructOpt;
use time::Timespec;
extern crate libc;
extern crate time;
mod aha;
mod github;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::env;
use std::ffi::OsStr;

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
    tree: Value,
    attrs: BTreeMap<u64, FileAttr>,
    inodes: BTreeMap<String, u64>,
    loads: BTreeMap<u64, i8>,
    types: BTreeMap<u64, String>,
}
impl AhaFS {
    fn load_releases(&mut self, ino: u64) {}
    fn load_features(&mut self, ino: u64) {}
    fn load_inodes(&mut self, ino: &u64) {}
    fn new() -> AhaFS {
        // figure out lifetimes
        let aha = aha::Aha::new(
            AHACONFIG.0.aha_domain.clone(),
            AHACONFIG.0.aha_token.clone(),
            AHACONFIG.0.workflow_email.clone(),
            &AHACONFIG.1,
        );
        let mut attrs = BTreeMap::new();
        let mut inodes = BTreeMap::new();
        let mut loads = BTreeMap::new();
        let mut types = BTreeMap::new();
        let ts = time::now().to_timespec();
        let attr = FileAttr {
            ino: 1,
            size: 0,
            blocks: 0,
            atime: ts,
            mtime: ts,
            ctime: ts,
            crtime: ts,
            kind: FileType::Directory,
            perm: 0o755,
            nlink: 0,
            uid: 0,
            gid: 0,
            rdev: 0,
            flags: 0,
        };
        attrs.insert(1, attr);
        inodes.insert("/".to_string(), 1);
        let tree = aha.projects();
        for (y, x) in tree.get("products").unwrap().as_array().iter().enumerate() {
            for (i, key) in x.iter().enumerate() {
                dbg!(key, i);
                let attr = FileAttr {
                    ino: key["id"].as_str().unwrap().parse::<u64>().unwrap(),
                    size: 0,
                    blocks: 0,
                    atime: ts,
                    mtime: ts,
                    ctime: ts,
                    crtime: ts,
                    kind: FileType::Directory,
                    perm: 0o755,
                    nlink: 0,
                    uid: 0,
                    gid: 0,
                    rdev: 0,
                    flags: 0,
                };
                attrs.insert(attr.ino, attr);
                types.insert(attr.ino, "project".to_string());
                //loads.insert(attr.ino, 1);
                let key_name = key["name"].as_str().unwrap().to_string();
                inodes.insert(key_name, attr.ino);

                let release = aha.releases(key["id"].as_str().unwrap().to_string());
                dbg!(&release);
                for (i2, key2) in release.iter().enumerate() {
                    dbg!(key2, i2);
                    let attr = FileAttr {
                        ino: key2["id"].as_str().unwrap().parse::<u64>().unwrap(),
                        size: 0,
                        blocks: 0,
                        atime: ts,
                        mtime: ts,
                        ctime: ts,
                        crtime: ts,
                        kind: FileType::Directory,
                        perm: 0o755,
                        nlink: 0,
                        uid: 0,
                        gid: 0,
                        rdev: 0,
                        flags: 0,
                    };
                    attrs.insert(attr.ino, attr);
                    types.insert(attr.ino, "release".to_string());
                    //loads.insert(attr.ino, 1);
                    let key_name = key2["name"].as_str().unwrap().to_string();
                    inodes.insert(key_name, attr.ino);
                }
            }
        }
        AhaFS {
            tree: tree.clone(),
            attrs: attrs,
            inodes: inodes,
            loads: loads,
            types: types,
        }
    }
}

impl Filesystem for AhaFS {
    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
        println!("getattr(ino={})", ino);
        match self.attrs.get(&ino) {
            Some(attr) => {
                let ttl = Timespec::new(1, 0);
                reply.attr(&ttl, attr);
            }
            None => reply.error(ENOENT),
        };
    }

    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        println!("lookup(parent={}, name={})", parent, name.to_str().unwrap());
        let inode = match self.inodes.get(name.to_str().unwrap()) {
            Some(inode) => inode,
            None => {
                println!("lookup no inode)");
                reply.error(ENOENT);
                return;
            }
        };
        match self.attrs.get(inode) {
            Some(attr) => {
                println!("looku)found att");
                let ttl = Timespec::new(1, 0);
                reply.entry(&ttl, attr, 0);
            }
            None => reply.error(ENOENT),
        };
    }

    fn read(
        &mut self,
        _req: &Request,
        ino: u64,
        fh: u64,
        offset: i64,
        size: u32,
        reply: ReplyData,
    ) {
        println!(
            "read(ino={}, fh={}, offset={}, size={})",
            ino, fh, offset, size
        );
        for (key, &inode) in &self.inodes {
            if inode == ino {
                let value = &self.tree[key];
                reply.data(value.as_str().unwrap().to_string().as_bytes());
                return;
            }
        }
        reply.error(ENOENT);
    }

    fn readdir(
        &mut self,
        _req: &Request,
        ino: u64,
        fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        println!("readdir(ino={}, fh={}, offset={})", ino, fh, offset);
        if ino == 1 {
            if offset == 0 {
                reply.add(1, 0, FileType::Directory, ".");
                reply.add(1, 1, FileType::Directory, "..");
                for (key, &inode) in &self.inodes {
                    if inode == 1 {
                        continue;
                    }
                    let offset = inode as i64; // hack
                    println!("\tkey={}, inode={}, offset={}", key, inode, offset);
                    reply.add(inode, offset, FileType::RegularFile, key);
                }
            }
            reply.ok();
        } else {
            if self.loads.get(&ino).is_none() {
                self.loads.insert(ino, 1);
                let aha = aha::Aha::new(
                    AHACONFIG.0.aha_domain.clone(),
                    AHACONFIG.0.aha_token.clone(),
                    AHACONFIG.0.workflow_email.clone(),
                    &AHACONFIG.1,
                );

                dbg!(self.types.get(&ino));
                if self.types.get(&ino).is_some() {
                    let type_name = self.types.get(&ino).unwrap().clone();
                    if type_name == "project".to_string() {
                        let release = aha.releases(ino.to_string());
                        dbg!(&release);
                        let mut a: Vec<(u64, i64, String)> = vec![];
                        for (i, key) in release.iter().enumerate() {
                            let ts = time::now().to_timespec();
                            let attr = FileAttr {
                                ino: key["id"].as_str().unwrap().parse::<u64>().unwrap(),
                                size: 0,
                                blocks: 0,
                                atime: ts,
                                mtime: ts,
                                ctime: ts,
                                crtime: ts,
                                kind: FileType::Directory,
                                perm: 0o755,
                                nlink: 0,
                                uid: 0,
                                gid: 0,
                                rdev: 0,
                                flags: 0,
                            };
                            self.attrs.insert(attr.ino, attr);
                            self.types.insert(attr.ino, "release".to_string());
                            let key_name = key["name"].as_str().unwrap().to_string();
                            self.inodes.insert(key_name.clone(), attr.ino);
                            a.push((attr.ino, i as i64, key_name.clone()));
                        }
                        dbg!(&a);
                        for (y, x, z) in a {
                            println!("\tkey={}, inode={}, offset={}", z, y, x);
                            reply.add(y, x, FileType::Directory, z);
                        }
                    }
                    if type_name == "release".to_string() {}
                }
            }
            reply.ok();
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let data: Value = serde_json::from_str("{}").unwrap();
    let mut fs = AhaFS::new();
    let options = ["-o", "ro", "-o", "fsname=ahafs", "-o", "auto_unmount"]
        .iter()
        .map(|o| o.as_ref())
        .collect::<Vec<&OsStr>>();
    fuse::mount(fs, &"/tmp/ahafs3".to_string(), &options).unwrap();
    Ok(())
}
