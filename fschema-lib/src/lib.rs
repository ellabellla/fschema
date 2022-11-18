use std::{
    collections::HashMap,
    fmt::Display,
    fs::{self, File},
    io,
    os::unix::{self, prelude::PermissionsExt},
    path::PathBuf,
    process::Command,
};

use itertools::Itertools;
use serde::{Deserialize, Serialize};

pub mod parse;

#[derive(Debug)]
pub enum Error {
    IO(io::Error, String),
    Command(i32, String),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::IO(e, data) => f.write_fmt(format_args!("An IO error occurred with '{}': {}", data, e)),
            Error::Command(exit, data) => f.write_fmt(format_args!("Command, '{}', exited with code {}", data, exit)),
        }
    }
}

#[derive(Debug, Default)]
pub struct FSchema {
    root: HashMap<String, Node>,
    prebuild: Vec<String>,
    postbuild: Vec<String>,
}


#[derive(Debug)]
pub enum Node {
    File{data: String, options: FileOptions},
    Directory(HashMap<String, Node>),
}

#[derive(Serialize, Deserialize, Debug)]
pub enum FileType {
    Text,
    Copy,
    Piped,
    Link,
    Bytes,
}

impl Default for FileType {
    fn default() -> Self {
        FileType::Text
    }
}

#[derive(Debug, Default)]
pub struct FileOptions {
    ftype: FileType,
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

        for command in &self.prebuild {
            run(command)?;
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
                        
                        match options.ftype {
                            FileType::Text => fs::write(&path, data).map_err(|e| Error::IO(e, format!("{}:{}", inner_path, data)))?,
                            FileType::Copy => fs::copy(resolve_data_path(data, options.internal, &root), &path)
                                .map(|_| ())
                                .map_err(|e| Error::IO(e, format!("{}:{}", inner_path, data)))?,
                            FileType::Link => {
                                unix::fs::symlink(resolve_data_path(data, options.internal, &root), &path)
                                    .map_err(|e| Error::IO(e, format!("{}:{}", inner_path, data)))?
                            }
                            FileType::Piped => fs::write(&path, &pipe(data)?).map_err(|e| Error::IO(e, format!("{}:{}", inner_path, data)))?,
                            FileType::Bytes => {
                                fs::write(&path, data.chars()
                                    .chunks(2)
                                    .into_iter()
                                    .map(|byte| u8::from_str_radix(&byte.collect::<String>(), 16).unwrap())
                                    .collect::<Vec<u8>>()
                                ).map_err(|e| Error::IO(e, format!("{}:{}", inner_path, data)))?
                            },
                        }

                        if let Some(mode) = options.mode {
                            let f = File::options()
                                .read(true)
                                .write(true)
                                .open("foo.txt")
                                .map_err(|e| Error::IO(e, format!("{}:{}", inner_path, data)))?;
                            let metadata = f.metadata().map_err(|e| Error::IO(e, format!("{}:{}", inner_path, data)))?;
                            metadata.permissions().set_mode(mode);
                        }
                    }
                    Node::Directory(contents) => {
                        fs::create_dir_all(&path).map_err(|e| Error::IO(e, format!("{:?}", path)))?;

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

        for command in &self.postbuild {
            run(command)?;
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
        .map_err(|e| Error::IO(e, command.to_string()))
        .and_then(|mut child| child.wait().map_err(|e| Error::IO(e, command.to_string())))
        .map(|status| status.code().unwrap_or(0))
}

fn pipe(command: &str) -> Result<String, Error> {
    Command::new("bash")
        .args(["-c", &command])
        .output()
        .map_err(|e| Error::IO(e, command.to_string()))
        .map(|output|  String::from_utf8_lossy(&output.stdout).to_string())
}
