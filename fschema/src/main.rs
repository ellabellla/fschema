use std::{path::PathBuf, str::FromStr, process::exit, env};

use clap::Parser;
use fschema_lib::FSchema;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Schema
    schema: String,

    /// Output Directory
    output: Option<String>
}

pub fn main() {
    let args = Args::parse();

    let schema_path = match PathBuf::from_str(&args.schema) {
        Ok(path) => path,
        Err(e) => {
            println!("Invalid schema path, {}", e);
            exit(1);
        },
    };

    if !schema_path.is_file() {
        println!("Schema must be a file");
        exit(1);
    }

    let creation_path = match args.output {
        Some(path) => match PathBuf::from_str(&path) {
            Ok(path) => path,
            Err(e) => {
                println!("Invalid output path, {}", e);
                exit(1);
            },
        },
        None => match env::current_dir() {
            Ok(path) => path,
            Err(e) => {
                println!("Couldn't get output directory, {}", e);
                exit(1);
            },
        },
    };

    if !creation_path.is_dir() {
        println!("Output directory must be a directory");
        exit(1);
    }

    let schema = match FSchema::from_file(&schema_path) {
        Ok(schema) => schema,
        Err(e) => {
            println!("Couldn't parse schema, {}", e);
            exit(1);
        },
    };

    if let Err(e) =  schema.create(creation_path) {
        println!("Error creating directory tree from schema, {}", e);
        exit(1);
    }
    
}