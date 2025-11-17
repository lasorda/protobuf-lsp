use anyhow::Result;
use protobuf_lsp::parser::proto::ProtoParser;

#[tokio::main]
async fn main() -> Result<()> {
    let parser = ProtoParser::new();

    println!("=== Testing Line Number Fix ===");

    // Test with simple test.proto
    println!("\n1. Testing with simple test.proto:");
    let test_content = std::fs::read_to_string("test.proto")?;
    let test_result = parser.parse("test.proto".to_string(), &test_content).await?;

    for msg in &test_result.messages {
        println!("  Message '{}' at line {}", msg.name, msg.line);
    }

    // Check TestMsg1 line number
    if let Some(test_msg1) = test_result.messages.iter().find(|m| m.name == "TestMsg1") {
        if test_msg1.line == 3 {
            println!("  ✅ SUCCESS: TestMsg1 is at correct line 3!");
        } else {
            println!("  ❌ FAILURE: TestMsg1 is at line {}, expected 3", test_msg1.line);
        }
    }

    // Test with real teams.proto file
    println!("\n2. Testing with teams.proto:");
    let teams_path = "/data/mm64/zhihaopan/QQMail/mmsearch2/uxsearch/mmsearchsvscommcardcore/card/proto/teams.proto";
    let teams_content = std::fs::read_to_string(teams_path)?;
    let teams_result = parser.parse("teams.proto".to_string(), &teams_content).await?;

    // Find Teams message
    if let Some(teams) = teams_result.messages.iter().find(|m| m.name == "Teams") {
        println!("  Teams message found:");
        println!("    Line: {}", teams.line);
        println!("    End line: {}", teams.end_line);
        println!("    Character: {}", teams.character);
        println!("    Full name: {}", teams.full_name);

        if teams.line == 48 {
            println!("  ✅ SUCCESS: Teams message is at correct line 48 (0-indexed)!");
        } else {
            println!("  ❌ FAILURE: Teams message is at line {}, expected 48 (0-indexed)", teams.line);
        }

        // Show the actual line at that position for verification
        let lines: Vec<&str> = teams_content.lines().collect();
        if teams.line as usize > 0 && teams.line as usize <= lines.len() {
            println!("    Line {} content: '{}'", teams.line, lines[teams.line as usize - 1]);
        }
    } else {
        println!("  ❌ FAILURE: Teams message not found at top level");

        // Check if it's nested inside other messages
        println!("  Searching for Teams message in nested messages...");
        let mut found_teams = false;
        for msg in &teams_result.messages {
            find_nested_message(msg, "Teams", &mut found_teams);
        }

        if !found_teams {
            // Show first few messages for debugging
            println!("  First 10 messages found:");
            for (i, msg) in teams_result.messages.iter().take(10).enumerate() {
                println!("    {}. '{}' at line {}", i+1, msg.name, msg.line);
            }
        }
    }

    // Test some other key messages from teams.proto
    let test_messages = vec![
        ("AgentCPTeamSchedulesReq", 3),
        ("AgentCPTeamSchedulesResp", 8),
        ("Match", 58),
        ("AgentCPMatchSchedulesResp", 137),
    ];

    println!("\n3. Testing other key messages:");
    for (msg_name, expected_line) in test_messages {
        if let Some(msg) = teams_result.messages.iter().find(|m| m.name == msg_name) {
            if msg.line == expected_line {
                println!("  ✅ {}: line {} (correct)", msg_name, msg.line);
            } else {
                println!("  ❌ {}: line {} (expected {})", msg_name, msg.line, expected_line);
            }
        } else {
            println!("  ❌ {}: not found", msg_name);
        }
    }

    Ok(())
}

fn find_nested_message(msg: &protobuf_lsp::parser::proto::MessageElement, target: &str, found: &mut bool) {
    if msg.name == target {
        println!("  ✅ Found '{}' nested message at line {}", target, msg.line);
        *found = true;
    }

    // Search in nested messages
    for nested in &msg.nested_messages {
        find_nested_message(nested, target, found);
    }
}