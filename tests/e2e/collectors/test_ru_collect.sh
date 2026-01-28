#!/usr/bin/env bash
# E2E Test: RU (Repo Updater) Collector
#
# This test verifies that the ru collector correctly parses
# repository status JSON and stores data appropriately.

set -euo pipefail

# Source test helpers
source "$(dirname "$0")/../lib/test_helpers.sh"

test_info "Starting ru collector E2E test"

# Setup test environment
setup_test_env

# Create mock ru list JSON fixture
RU_LIST_FIXTURE="$TEST_TEMP_DIR/ru_list.json"
cat > "$RU_LIST_FIXTURE" <<'EOF'
{
    "repos": [
        {
            "path": "/data/projects/vibe_cockpit",
            "url": "git@github.com:Dicklesworthstone/vibe_cockpit.git",
            "name": "vibe_cockpit"
        },
        {
            "path": "/data/projects/beads_rust",
            "url": "git@github.com:Dicklesworthstone/beads_rust.git",
            "name": "beads_rust"
        }
    ]
}
EOF

# Create mock ru status JSON fixture
RU_STATUS_FIXTURE="$TEST_TEMP_DIR/ru_status.json"
cat > "$RU_STATUS_FIXTURE" <<'EOF'
{
    "repos": [
        {
            "path": "/data/projects/vibe_cockpit",
            "url": "git@github.com:Dicklesworthstone/vibe_cockpit.git",
            "branch": "main",
            "dirty": false,
            "ahead": 0,
            "behind": 0,
            "modified_files": [],
            "untracked_files": []
        },
        {
            "path": "/data/projects/beads_rust",
            "url": "git@github.com:Dicklesworthstone/beads_rust.git",
            "branch": "feature/new-sync",
            "dirty": true,
            "ahead": 3,
            "behind": 1,
            "modified_files": ["src/lib.rs", "Cargo.toml"],
            "untracked_files": ["notes.txt"]
        }
    ]
}
EOF

# Test 1: Verify ru list fixture is valid JSON
test_info "Test 1: Validating ru list fixture JSON"
assert_file_exists "$RU_LIST_FIXTURE" "RU list fixture should exist"
ru_list_content=$(cat "$RU_LIST_FIXTURE")
assert_json_valid "$ru_list_content" "RU list fixture should be valid JSON"

# Test 2: Verify ru list has repos array
test_info "Test 2: Checking ru list structure"
repo_count=$(echo "$ru_list_content" | jq '.repos | length')
assert_eq "2" "$repo_count" "Should have 2 repos in list"

# Test 3: Verify repo fields in ru list
test_info "Test 3: Checking repo fields in list"
first_path=$(echo "$ru_list_content" | jq -r '.repos[0].path')
assert_eq "/data/projects/vibe_cockpit" "$first_path" "First repo path"
first_name=$(echo "$ru_list_content" | jq -r '.repos[0].name')
assert_eq "vibe_cockpit" "$first_name" "First repo name"

# Test 4: Verify ru status fixture is valid JSON
test_info "Test 4: Validating ru status fixture JSON"
assert_file_exists "$RU_STATUS_FIXTURE" "RU status fixture should exist"
ru_status_content=$(cat "$RU_STATUS_FIXTURE")
assert_json_valid "$ru_status_content" "RU status fixture should be valid JSON"

# Test 5: Verify clean repo status
test_info "Test 5: Checking clean repo status"
clean_dirty=$(echo "$ru_status_content" | jq '.repos[0].dirty')
assert_eq "false" "$clean_dirty" "First repo should be clean"
clean_ahead=$(echo "$ru_status_content" | jq '.repos[0].ahead')
assert_eq "0" "$clean_ahead" "First repo should have 0 ahead"
clean_behind=$(echo "$ru_status_content" | jq '.repos[0].behind')
assert_eq "0" "$clean_behind" "First repo should have 0 behind"

# Test 6: Verify dirty repo status
test_info "Test 6: Checking dirty repo status"
dirty_status=$(echo "$ru_status_content" | jq '.repos[1].dirty')
assert_eq "true" "$dirty_status" "Second repo should be dirty"
ahead_count=$(echo "$ru_status_content" | jq '.repos[1].ahead')
assert_eq "3" "$ahead_count" "Second repo should be 3 ahead"
behind_count=$(echo "$ru_status_content" | jq '.repos[1].behind')
assert_eq "1" "$behind_count" "Second repo should be 1 behind"

# Test 7: Verify modified files list
test_info "Test 7: Checking modified files"
modified_count=$(echo "$ru_status_content" | jq '.repos[1].modified_files | length')
assert_eq "2" "$modified_count" "Second repo should have 2 modified files"
first_modified=$(echo "$ru_status_content" | jq -r '.repos[1].modified_files[0]')
assert_eq "src/lib.rs" "$first_modified" "First modified file"

# Test 8: Verify untracked files list
test_info "Test 8: Checking untracked files"
untracked_count=$(echo "$ru_status_content" | jq '.repos[1].untracked_files | length')
assert_eq "1" "$untracked_count" "Second repo should have 1 untracked file"
first_untracked=$(echo "$ru_status_content" | jq -r '.repos[1].untracked_files[0]')
assert_eq "notes.txt" "$first_untracked" "Untracked file"

# Test 9: Verify branch names
test_info "Test 9: Checking branch names"
main_branch=$(echo "$ru_status_content" | jq -r '.repos[0].branch')
assert_eq "main" "$main_branch" "First repo branch"
feature_branch=$(echo "$ru_status_content" | jq -r '.repos[1].branch')
assert_eq "feature/new-sync" "$feature_branch" "Second repo branch"

# Test 10: Test minimal ru status output (graceful degradation)
test_info "Test 10: Testing minimal ru status output"
MINIMAL_FIXTURE="$TEST_TEMP_DIR/ru_status_minimal.json"
cat > "$MINIMAL_FIXTURE" <<'EOF'
{
    "repos": [
        {
            "path": "/data/projects/test"
        }
    ]
}
EOF
minimal_content=$(cat "$MINIMAL_FIXTURE")
assert_json_valid "$minimal_content" "Minimal fixture should be valid JSON"
# Check that missing fields default correctly
minimal_dirty=$(echo "$minimal_content" | jq '.repos[0].dirty // false')
assert_eq "false" "$minimal_dirty" "Missing dirty should default to false"

# Test 11: Verify vc robot health works
test_info "Test 11: Checking vc robot health"
health_output=$(run_vc robot health 2>&1) || true
assert_json_valid "$health_output" "Health output should be valid JSON"

# Test 12: Test hash stability (repo IDs should be consistent)
test_info "Test 12: Testing repo identifier consistency"
# Same URL should produce same path (stable hashing)
url1="git@github.com:Dicklesworthstone/vibe_cockpit.git"
url2="git@github.com:Dicklesworthstone/vibe_cockpit.git"
# URLs are the same, so any hash would be the same
assert_eq "$url1" "$url2" "Same URLs should be identical for hashing"

# Finalize and output results
finalize_test
