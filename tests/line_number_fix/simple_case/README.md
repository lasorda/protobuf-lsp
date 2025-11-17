# Simple Line Number Test Case

## Problem Description
Test whether basic line number parsing works correctly.

## Test File
- `test.proto`: Simple protobuf file containing two message definitions
- `TestMsg1` should be at line 3 (0-indexed: 2)
- `TestMsg2` should be at line 7 (0-indexed: 6)
- `TestMsg1 test = 1` in `TestMsg2` should jump to the `TestMsg1` definition at line 3

## Expected Results
- TestMsg1: line 2 (0-indexed)
- TestMsg2: line 6 (0-indexed)
- Navigation from TestMsg2.test field should go to TestMsg1 definition