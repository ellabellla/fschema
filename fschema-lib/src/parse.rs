use std::{collections::HashMap};

use serde::{ser::{SerializeSeq, SerializeMap}, Deserialize, Serialize, de::{Visitor, Error}, Deserializer};

use crate::{FSchema, FileOptions, FileType};

impl Serialize for FSchema {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer 
    {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("root", &self.root)?;
        
        map.serialize_entry("prebuild",  &self.prebuild)?;
        map.serialize_entry("postbuild",  &self.postbuild)?;

        map.end()
    }
}

impl<'de> Deserialize<'de> for FSchema {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de> 
    {  
        deserializer.deserialize_map(FSchemaVisitor)
    }
}

struct FSchemaVisitor;

impl<'de> Visitor<'de> for FSchemaVisitor {
    type Value = FSchema;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a fschema")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::MapAccess<'de>, 
    {
        let mut schema = FSchema::default();
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "root" => schema.root = map.next_value::<HashMap<String, Node>>()?,
                "prebuild" => schema.prebuild = map.next_value::<Vec<String>>()?,
                "postbuild" => schema.postbuild = map.next_value::<Vec<String>>()?,
                _ => return Err(Error::unknown_field(&key, &["root", "prebuild", "postbuild"]))
            }
        }
        Ok(schema)
    }
}

impl Serialize for FileOptions {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer 
    {   
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("ftype", &self.ftype)?;
        map.serialize_entry("defer", &self.defer)?;
        map.serialize_entry("internal", &self.internal)?;
        if let Some(mode) = &self.mode {
            map.serialize_entry("mode", mode)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for FileOptions {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de> {
            deserializer.deserialize_map(FileOptionsVisitor)
    }
}

struct FileOptionsVisitor;

impl<'de> Visitor<'de> for FileOptionsVisitor {
    type Value = FileOptions;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("File Options")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::MapAccess<'de>, 
    {
        let mut options = FileOptions::default();
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "ftype" => options.ftype = map.next_value::<FileType>()?,
                "mode" => options.mode = Some(u32::from_str_radix(&map.next_value::<String>()?, 8).map_err(|_| Error::custom("expected octal number"))?),
                "defer" => options.defer = map.next_value::<u64>()?,
                "internal" => options.internal = map.next_value::<bool>()?,
                _ => return Err(Error::unknown_field(&key, &["ftype", "mode"]))
            }
        }
        Ok(options)
    }
}

#[derive(Debug)]
pub enum Node {
    File{data: String, options: FileOptions},
    Directory(HashMap<String, Node>),
}

impl Serialize for Node {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer 
    {
        match self {
            Node::File { data, options } => {
                let mut seq = serializer.serialize_seq(Some(2))?;
                seq.serialize_element(options)?;
                seq.serialize_element(data)?;
                seq.end()
            },
            Node::Directory(dir) => {
                let mut map = serializer.serialize_map(Some(dir.len()))?;
                for (key, value) in dir {
                    map.serialize_entry(key, value)?;
                }
                map.end()
            },
        }
    }
}

impl<'de> Deserialize<'de> for Node {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de> 
    {   
        deserializer.deserialize_any(NodeVisitor)
    }
}

pub enum InnerFileNode {
    FileOptions(FileOptions),
    Data(String)
}

impl<'de> Deserialize<'de> for InnerFileNode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de> {
        deserializer.deserialize_any(InnerFileNodeVisitor)
    }
}

struct InnerFileNodeVisitor;

impl<'de> Visitor<'de> for InnerFileNodeVisitor {
    type Value = InnerFileNode;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("either file options or file data")
    }

    fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::MapAccess<'de>, 
    {
        FileOptionsVisitor.visit_map(map).map(|o| InnerFileNode::FileOptions(o))  
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
        where
            E: Error, 
    {
        Ok(InnerFileNode::Data(v))
    }

    fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
        where
            E: Error, 
    {
        Ok(InnerFileNode::Data(v.to_string()))
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: Error,
    {
        Ok(InnerFileNode::Data(v.to_string()))     
    }
}

struct NodeVisitor;

impl<'de> Visitor<'de> for NodeVisitor {
    type Value = Node;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a file or directory")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
    {
        
        let mut options = None; 
        let mut data = None;
        
        for _ in 0..2 {
            match seq.next_element::<InnerFileNode>()? {
                Some(inner_node) => match inner_node {
                    InnerFileNode::FileOptions(found_options) => options = Some(found_options),
                    InnerFileNode::Data(found_data) => data = Some(found_data),
                },
                None => break,
            }
        }

        let options = options.unwrap_or(FileOptions::default());

        if let Some(data) = data {
            if let FileType::Bytes = options.ftype {
                if !data.chars().all(|c| {
                    c.is_ascii_digit() || 
                    c.to_ascii_lowercase() == 'a'|| 
                    c.to_ascii_lowercase() == 'b'|| 
                    c.to_ascii_lowercase() == 'c'|| 
                    c.to_ascii_lowercase() == 'd'|| 
                    c.to_ascii_lowercase() == 'e'|| 
                    c.to_ascii_lowercase() == 'f'
                }) {
                    return Err(Error::custom("Expected data of byte file to be a hexadecimal number"))
                }
            }

            Ok(Node::File { options, data })
        } else {
            Err(Error::custom("Expected file data"))
        }
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::MapAccess<'de>, 
    {
        let mut dir = HashMap::new();
        
        while let Some((key, node)) = map.next_entry::<String, Node>()? {
            dir.insert(key, node);
        }

        Ok(Node::Directory(dir))
    }
}


#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::parse::FSchema;

    use super::{Node, FileType, FileOptions};

    #[test]
    fn test() {
        let mut root = HashMap::new();
        root.insert("hello".to_string(), Node::File { options: FileOptions{ftype: FileType::Text, mode: None, defer: 0, internal: false}, data: "Hello, World!".to_string() });
        root.insert("hex".to_string(), Node::File { options: FileOptions{ftype: FileType::Bytes, mode: None, defer: 0, internal: false}, data: "00aF".to_string() });

        let mut dir = HashMap::new();
        dir.insert("file".to_string(), Node::File { options: FileOptions::default(), data: "a file".to_string() });

        root.insert("dir".to_string(), Node::Directory(dir));

        let schema = FSchema{root, postbuild: vec![], prebuild: vec![]};
        let json = serde_json::to_string_pretty(&schema).unwrap();
        println!("{}", json);   
        println!("{:?}", serde_json::from_str::<FSchema>(&json).unwrap())
    }
}