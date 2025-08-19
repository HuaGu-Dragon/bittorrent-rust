use serde_json;
use std::env;

fn decode_bencoded_value(encoded_value: &str) -> anyhow::Result<(serde_json::Value, &str)> {
    match encoded_value.bytes().next() {
        Some(b'i') => {
            if let Some((n, rest)) = encoded_value
                .strip_prefix('i')
                .and_then(|rest| (&*rest).split_once('e'))
            {
                let n = n.parse::<i64>()?;
                return Ok((n.into(), rest));
            }
        }
        Some(b'l') => {
            let mut items = vec![];
            let mut rest = encoded_value.split_at(1).1;
            while !rest.starts_with('e') {
                let (v, reminder) = decode_bencoded_value(rest)?;
                items.push(v);
                rest = reminder;
            }
            return Ok((items.into(), &rest[1..]));
        }
        Some(b'0'..=b'9') => {
            if let Some((len, rest)) = encoded_value.split_once(':') {
                if let Ok(len) = len.parse::<usize>() {
                    return Ok((rest[..len].to_string().into(), &rest[len..]));
                }
            }
        }
        _ => {}
    }
    anyhow::bail!("Invalid bencoded value: {}", encoded_value)
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    let command = &args[1];

    if command == "decode" {
        let encoded_value = &args[2];
        let decoded_value = decode_bencoded_value(encoded_value);
        println!("{}", decoded_value?.0.to_string());
    } else {
        println!("unknown command: {}", args[1])
    }

    Ok(())
}
