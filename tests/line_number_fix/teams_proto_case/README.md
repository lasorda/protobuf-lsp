# Teams.proto Line Number Test Case

## Problem Description
Line number navigation issue in the actual teams.proto file:
- `optional Teams team = 4` at line 62 should jump to the `Teams` message definition at line 49
- But previously incorrectly jumped to line 11

## File Structure
The teams.proto file contains multiple nested Teams messages:
1. Line 49: `Teams` message nested inside `AgentCPTeamSchedulesResp`
2. Line 78: `Teams` message nested inside `AgentCPTeamSchedulesOfflineData`
3. Line 335: `Teams` message nested inside `VsDataResp`

## Test Focus
- Verify that `Teams team = 4` at line 62 correctly jumps to the `Teams` message at line 49
- Verify that line numbers in document symbols are displayed correctly
- Verify that other message line numbers are also correct

## Actual File Used
File location: `/data/mm64/zhihaopan/QQMail/mmsearch2/uxsearch/mmsearchsvscommcardcore/card/proto/teams.proto`

## Expected Results
- Teams message (first): line 48 (0-indexed), corresponding to file line 49
- AgentCPTeamSchedulesReq: line 2 (0-indexed), corresponding to file line 3
- AgentCPTeamSchedulesResp: line 7 (0-indexed), corresponding to file line 8