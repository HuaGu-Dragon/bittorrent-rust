use serde_json;
use std::env;

fn decode_bencoded_value(encoded_value: &str) -> serde_json::Value {
    if let Some((len, rest)) = encoded_value.split_once(':') {
        if let Ok(len) = len.parse::<usize>() {
            return serde_json::Value::String(rest[..len].to_string());
        }
    }
    todo!()
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let command = &args[1];

    if command == "decode" {
        let encoded_value = &args[2];
        let decoded_value = decode_bencoded_value(encoded_value);
        println!("{}", decoded_value.to_string());
    } else {
        println!("unknown command: {}", args[1])
    }
}
