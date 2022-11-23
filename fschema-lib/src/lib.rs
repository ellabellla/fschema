
use std::{
    collections::HashMap,
    fmt::Display,
    fs::{self, File},
    io,
    os::unix::{self, prelude::PermissionsExt},
    path::PathBuf,
    process::Command, str::FromStr,
};

use itertools::Itertools;
use serde::{Deserialize, Serialize};

pub mod parse;

#[derive(Debug)]
/// FSchema Errors
pub enum Error {
    /// An IO error occurred
    IO(io::Error, String),
    /// An Error occurred whilst running a command
    Command(i32, String),
    /// An Error occurred converting a string to a path
    Path(std::convert::Infallible, String),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::IO(e, data) => f.write_fmt(format_args!("An IO error occurred with '{}': {}", data, e)),
            Error::Command(exit, data) => f.write_fmt(format_args!("Command, '{}', exited with code {}", data, exit)),
            Error::Path(e, data) => f.write_fmt(format_args!("Could not create path from '{}': {}", data, e)),
        }
    }
}

#[derive(Debug, Default)]
/// FSchema
/// A file system structure schema. Used to create nested directories and files.
pub struct FSchema {
    root: HashMap<String, Node>,
    prebuild: Vec<String>,
    postbuild: Vec<String>,
}


#[derive(Debug)]
/// Node in file system structure tree
pub enum Node {
    File{data: String, options: FileOptions},
    Directory(HashMap<String, Node>),
}

#[derive(Serialize, Deserialize, Debug)]
/// File Data Type
pub enum FileType {
    /// Text
    Text,
    /// Copy of existing file
    Copy,
    /// Data dynamically created from command
    Piped,
    /// Symbolic link to file 
    Link,
    /// Create from hex representation of bytes
    Hex,
    /// Create from bits
    Bits,
}

impl Default for FileType {
    fn default() -> Self {
        FileType::Text
    }
}

#[derive(Debug, Default)]
/// File options
pub struct FileOptions {
    /// Type of file data
    ftype: FileType,
    /// Permissions (octal)
    mode: Option<u32>,
    /// At what stage should this file be created
    defer: u64,
    /// Is the path stored in the file data relative to the root of the file system structure
    internal: bool,
}

impl FSchema {
    /// Create from reader, Must implement io::Read.
    pub fn from_reader<R>(reader: &mut R) -> io::Result<FSchema> 
    where
        R: io::Read
    {
        Ok(serde_json::from_reader(reader)?)
    }

    /// Create from string containing json
    pub fn from_str(json: &str) -> io::Result<FSchema> {
        Ok(serde_json::from_str(json)?)
    }

    /// Create file system structure from schema. Takes the location of where to place root as an argument 
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
                            FileType::Text => if data.len() == 0 {
                                File::create(&path).map_err(|e| Error::IO(e, format!("{}: [{}, {:?}]", inner_path, data, options.ftype)))?;
                            } else {
                                fs::write(&path, data).map_err(|e| Error::IO(e, format!("{}: [{}, {:?}]", inner_path, data, options.ftype)))?
                            },
                            FileType::Copy => fs::copy(resolve_data_path(data, options.internal, &root)?, &path)
                                .map(|_| ())
                                .map_err(|e| Error::IO(e, format!("{}: [{}, {:?}]", inner_path, data, options.ftype)))?,
                            FileType::Link => {
                                unix::fs::symlink(resolve_data_path(data, options.internal, &root)?, &path)
                                    .map_err(|e| Error::IO(e, format!("{}: [{}, {:?}]", inner_path, data, options.ftype)))?
                            }
                            FileType::Piped => fs::write(&path, &pipe(data)?).map_err(|e| Error::IO(e, format!("{}: [{}, {:?}]", inner_path, data, options.ftype)))?,
                            FileType::Hex => {
                                fs::write(&path, data.chars()
                                    .chunks(2)
                                    .into_iter()
                                    .map(|byte| u8::from_str_radix(&byte.collect::<String>(), 16).unwrap())
                                    .collect::<Vec<u8>>()
                                ).map_err(|e| Error::IO(e, format!("{}: [{}, {:?}]", inner_path, data, options.ftype)))?
                            },
                            FileType::Bits => fs::write(&path, data.chars()
                                .chunks(8)
                                .into_iter()
                                .map(|byte| u8::from_str_radix(&byte.collect::<String>(), 2).unwrap())
                                .collect::<Vec<u8>>()
                            ).map_err(|e| Error::IO(e, format!("{}: [{}, {:?}]", inner_path, data, options.ftype)))?,
                        }

                        if let Some(mode) = options.mode {
                            let f = File::options()
                                .read(true)
                                .write(true)
                                .open(&path)
                                .map_err(|e| Error::IO(e, format!("{}: [{}, {:?}]", inner_path, data, options.ftype)))?;
                            let metadata = f.metadata().map_err(|e| Error::IO(e, format!("{}: [{}, {:?}]", inner_path, data, options.ftype)))?;
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

/// Resolve path stored in data string
fn resolve_data_path(data: &str, internal: bool, root: &PathBuf) -> Result<PathBuf, Error> {
    if internal {
        Ok(root.join(data))
    } else {
        PathBuf::from_str(data).map_err(|e| Error::Path(e, data.to_string()))
    }
}

/// Run a command in bash
fn run(command: &str) -> Result<(), Error> {
    Command::new("bash")
        .args(["-c", &command])
        .spawn()
        .map_err(|e| Error::IO(e, command.to_string()))
        .and_then(|mut child| child.wait().map_err(|e| Error::IO(e, command.to_string())))
        .and_then(|status| {
            let status = status.code().unwrap_or(0);
            if status == 0 {
                Ok(())
            } else {
                Err(Error::Command(status, command.to_string()))
            }
        })
}


/// Capture the output of a command run in bash
fn pipe(command: &str) -> Result<String, Error> {
    Command::new("bash")
        .args(["-c", &command])
        .output()
        .map_err(|e| Error::IO(e, command.to_string()))
        .and_then(|output| {
            let status = output.status.code().unwrap_or(0);
            if status == 0 {
                Ok(String::from_utf8_lossy(&output.stdout).to_string())
            } else {
                Err(Error::Command(status, command.to_string()))
            }
        })
}
