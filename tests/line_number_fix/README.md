# Line Number Fix Test Suite

## Overview
This test suite verifies that the line number parsing fix for the protobuf-lsp server works correctly.

## Background
Original issue: In teams.proto file, the `optional Teams team = 4` at line 62 was incorrectly jumping to line 11 when navigating to definition, instead of the correct line 49 where the Teams message is defined.

## Fix Details
Fixed line number calculation issue in `src/parser/proto.rs` in the `parse_simple` function:
- Replaced `current_line` with `line_number`
- Ensured message, enum, and service definitions use correct line numbers
- Fixed line number parsing for nested messages

## Test Cases

### 1. simple_case
- **Purpose**: Test basic line number parsing functionality
- **File**: `simple_case/test.proto`
- **Verification points**:
  - TestMsg1 at line 2 (0-indexed)
  - TestMsg2 at line 6 (0-indexed)

### 2. teams_proto_case
- **Purpose**: Test line number fix for actual teams.proto file
- **File**: Uses real teams.proto file
- **Verification points**:
  - AgentCPTeamSchedulesReq at line 2
  - AgentCPTeamSchedulesResp at line 7
  - Nested Teams messages at lines 48, 76, 333

## Running Tests
```bash
cargo run --example test_line_numbers
```

## Expected Results
All tests should show ✅ (correct), indicating successful line number parsing fix.

## Maintenance Notes
- Each test case has its own directory
- README.md files detail test purpose and expected results
- Test code is centralized in `test_line_numbers.rs` for easy maintenance