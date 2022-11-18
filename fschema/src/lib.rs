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
}

#[derive(Debug, Default)]
pub struct FileOptions {
    ftype: Option<FileType>,
    mode: Option<u32>,
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

        while stack.len() != 0 {
            while let Some((inner_path, node)) = stack.pop() {
                let path = root.join(&inner_path);

                match node {
                    Node::File { data, options } => {
                        match options.ftype.as_ref().unwrap_or(&FileType::Text) {
                            FileType::Text => fs::write(&path, data).map_err(|e| Error::IO(e))?,
                            FileType::Copy => fs::copy(data, &path)
                                .map(|_| ())
                                .map_err(|e| Error::IO(e))?,
                            FileType::Link => {
                                unix::fs::symlink(data, &path).map_err(|e| Error::IO(e))?
                            }
                            FileType::Pipe => fs::write(&path, &pipe(data)?).map_err(|e| Error::IO(e))?,
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
        }

        if let Some(postbuild) = &self.postbuild {
            run(postbuild)?;
        }
        Ok(())
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
