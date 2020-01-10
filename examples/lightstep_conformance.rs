/// This is intended to demonstrate and test lightstep carrier conformance
/// in combination with https://github.com/lightstep/conformance
use distracing::{LightStepTracer, Tracer};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{self, Read};

#[derive(Deserialize, Serialize, Debug, Clone)]
struct InputOutput {
    text_map: HashMap<String, String>,
    binary: String, // base64 encoded
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tracer = LightStepTracer::new();

    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer)?;
    let input: InputOutput = serde_json::from_str(buffer.as_str())?;

    let text_context = tracer.extract_from_http_headers(&input.text_map)?;

    let binary = base64::decode(input.binary.as_bytes())?;
    let binary_context = tracer.extract_from_binary(&binary)?;

    let mut text_map: HashMap<String, String> = HashMap::new();
    tracer.inject_into_text_map(&binary_context, &mut text_map);
    let output = InputOutput {
        text_map,
        binary: base64::encode(&tracer.inject_into_binary(&text_context)),
    };
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
