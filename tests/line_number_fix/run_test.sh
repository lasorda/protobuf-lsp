#!/bin/bash

echo "Running Protobuf LSP Line Number Fix Test"
echo "=========================================="

# Run the test
cd /data/mm64/zhihaopan/protobuf-lsp
cargo run --run tests/line_number_fix/test_line_numbers.rs

echo ""
echo "To run individual test cases:"
echo "  1. Simple case: Check basic line number parsing"
echo "  2. Teams case: Check teams.proto specific issues"