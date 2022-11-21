# FSchema

Define a file system structure using json. Only works on linux.

## The Schema
```json
{
    "prebuild": [],
    "root": {},
    "postbuild": []
}
```

The schema at at it's most basic level is made up of a json object with 3 properties; "prebuild", "root", and "postbuild". "prebuild" and "postbuild" are arrays of commands to execute before and after the file system structure has been constructed. "root" is an object containing the directories and files to be created.

A directory is an object where the keys are the names of the files/directories and the values are the files/directories data.

```json
{
    "root": {
        "directory": {

        }
    }
}
```

A file is an array containing the data of the file, as a sting, and an optional object containing file properties.
```json
{
    "root": {
        "file" : [ "data", { "ftype": "Text" } ]
    }
}
```

Files can be supplied with 4 different properties:
- "mode" defines what permissions a file should be created with as an octal. 
- "defer" defines when the file should be created. Files with lower "defer" properties will be created before files with higher "defer" properties.  The default "defer" value is 0
- "ftype" defines how the file data should be treated.  The default "ftype" is "Text".
  - "Text" type treats the file data as the text inside the file. 
  - "Copy" type will treat the file data as the path of a file to be copied for this file. 
  - "Piped" type treats the file data as a command and will pipe the output of the command into the file. 
  - "Link" type will treat the file data as a path of a file to be symbolically linked for this file.
- "internal" will defines whether the path given by the files data should be treated as a relative path to the filesystem's root path or not (only works with "ftype"s that treat file data as paths)
```json
{
    "ftype": "",
    "mode": "777",
    "defer": 0,
    "internal": false,
}
```

## The library
Loading a schema
```rust
let file = File::open("schema.json").unwrap();
let schema = FSchema::from_reader(file).unwrap();
```
```rust
let schema - FSchema::from_str(json_string).unwrap();
```

Creating a filesystem structure based on a schema
```rust
let root_path = PathBuf::from_str("/path/to/output/directory").unwrap();
schema.create(root_path).unwrap();
```
## The Binary
```bash
Usage: fschema <SCHEMA> [OUTPUT]

Arguments:
  <SCHEMA>  Schema
  [OUTPUT]  Output Directory

Options:
  -h, --help     Print help information
  -V, --version  Print version information
```

## License
This software is provided under the MIT license. Click [here](./LICENSE) to view.
