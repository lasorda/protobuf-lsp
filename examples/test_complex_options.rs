use protobuf_lsp::parser::ProtoParser;
use tokio::runtime::Runtime;
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <proto-file>", args[0]);
        return Ok(());
    }

    let file_path = &args[1];
    let content = fs::read_to_string(file_path)?;

    let rt = Runtime::new().unwrap();
    let parser = ProtoParser::new();
    let uri = format!("file://{}", file_path);

    println!("Testing error extraction for: {}", file_path);

    let parse_result = rt.block_on(async {
        parser.parse(uri, &content).await
    });

    match parse_result {
        Ok(proto) => {
            println!("\n--- Parse Result ---");
            println!("Package: {:?}", proto.package);
            println!("Messages: {}", proto.messages.len());
            println!("Total parse errors: {}", proto.parse_errors.len());

            if !proto.parse_errors.is_empty() {
                println!("\n--- Parse Errors ---");
                for (idx, error) in proto.parse_errors.iter().enumerate() {
                    println!("Error {}: Line {}, Col {} - {}",
                        idx + 1,
                        error.line + 1,
                        error.character + 1,
                        error.message
                    );
                }
            } else {
                println!("\n✅ No parse errors found!");
            }
        }
        Err(e) => {
            eprintln!("Parse failed: {}", e);
        }
    }

    Ok(())
}