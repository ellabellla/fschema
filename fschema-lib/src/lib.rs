use std::{
    collections::HashMap,
    fmt::Display,
    fs::{self, File},
    io,
    os::unix::{self, prelude::PermissionsExt},
    path::PathBuf,
    process::Command,
};

use parse::Node;
use itertools::Itertools;
use serde::{Deserialize, Serialize};

pub mod parse;

#[derive(Debug)]
pub enum Error {
    IO(io::Error),
    Command(i32),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::IO(e) => f.write_fmt(format_args!("An IO error occurred: {}", e)),
            Error::Command(exit) => f.write_fmt(format_args!("Command exited with code {}", exit)),
        }
    }
}

#[derive(Debug, Default)]
pub struct FSchema {
    root: HashMap<String, Node>,
    prebuild: Option<String>,
    postbuild: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum FileType {
    Text,
    Copy,
    Pipe,
    Link,
    Bytes,
}

#[derive(Debug, Default)]
pub struct FileOptions {
    ftype: Option<FileType>,
    mode: Option<u32>,
    defer: u64,
    internal: bool,
}

impl FSchema {
    pub fn from_file(path: &PathBuf) -> io::Result<FSchema> {
        Ok(serde_json::from_reader(File::open(path)?)?)
    }

    pub fn from_str(json: &str) -> io::Result<FSchema> {
        Ok(serde_json::from_str(json)?)
    }

    pub fn create(&self, root: PathBuf) -> Result<(), Error> {

        if let Some(prebuild) = &self.prebuild {
            run(prebuild)?;
        }

        let mut stack = self
            .root
            .iter()
            .map(|(name, node)| (name.to_string(), node))
            .collect::<Vec<(String, &Node)>>();
        let mut backstack = vec![];
        let mut defered = vec![];
        let mut deferal_level = 0;

        while stack.len() != 0 {
            while let Some((inner_path, node)) = stack.pop() {
                let path = root.join(&inner_path);

                match node {
                    Node::File { data, options } => {
                        if options.defer > deferal_level{
                            defered.push((inner_path, node));
                            continue;
                        }
                        
                        match options.ftype.as_ref().unwrap_or(&FileType::Text) {
                            FileType::Text => fs::write(&path, data).map_err(|e| Error::IO(e))?,
                            FileType::Copy => fs::copy(resolve_data_path(data, options.internal, &root), &path)
                                .map(|_| ())
                                .map_err(|e| Error::IO(e))?,
                            FileType::Link => {
                                unix::fs::symlink(resolve_data_path(data, options.internal, &root), &path)
                                    .map_err(|e| Error::IO(e))?
                            }
                            FileType::Pipe => fs::write(&path, &pipe(data)?).map_err(|e| Error::IO(e))?,
                            FileType::Bytes => {
                                fs::write(&path, data.chars()
                                    .chunks(2)
                                    .into_iter()
                                    .map(|byte| u8::from_str_radix(&byte.collect::<String>(), 16).unwrap())
                                    .collect::<Vec<u8>>()
                                ).map_err(|e| Error::IO(e))?
                            },
                        }

                        if let Some(mode) = options.mode {
                            let f = File::options()
                                .read(true)
                                .write(true)
                                .open("foo.txt")
                                .map_err(|e| Error::IO(e))?;
                            let metadata = f.metadata().map_err(|e| Error::IO(e))?;
                            metadata.permissions().set_mode(mode);
                        }
                    }
                    Node::Directory(contents) => {
                        fs::create_dir(path).map_err(|e| Error::IO(e))?;

                        backstack.extend(
                            contents
                                .iter()
                                .map(|(name, node)| (inner_path.to_string() + "/" + name, node)),
                        );
                    }
                }
            }

            (stack, backstack) = (backstack, stack);
            if stack.len() == 0 {
                (stack, defered) = (defered, stack);
                deferal_level += 1;
            }
        }

        if let Some(postbuild) = &self.postbuild {
            run(postbuild)?;
        }
        Ok(())
    }
}

fn resolve_data_path(data: &str, internal: bool, root: &PathBuf) -> String {
    if internal {
        root.join(data).as_os_str().to_string_lossy().to_string()
    } else {
        data.to_string()
    }
}

fn run(command: &str) -> Result<i32, Error> {
    Command::new("bash")
        .args(["-c", &command])
        .spawn()
        .map_err(|e| Error::IO(e))
        .and_then(|mut child| child.wait().map_err(|e| Error::IO(e)))
        .map(|status| status.code().unwrap_or(0))
}

fn pipe(command: &str) -> Result<String, Error> {
    Command::new("bash")
        .args(["-c", &command])
        .output()
        .map_err(|e| Error::IO(e))
        .map(|output|  String::from_utf8_lossy(&output.stdout).to_string())
}
