from src.scanner.walker import hyper_scan, ScanMatch
from pathlib import Path
from testing import assert_true, TestSuite

fn test_walker_scan() raises:
    var root = Path("src")
    # Scan for "Regex" which appears in c_regex.mojo and walker.mojo
    var results = hyper_scan(root, "Regex")

    assert_true(len(results) > 0, "Should find at least one file")

    var found_c_regex = False
    for i in range(len(results)):
        # Access fields directly to avoid implicit copy
        if "c_regex.mojo" in String(results[i].path):
            found_c_regex = True
            # Verify content was captured
            assert_true(len(results[i].content) > 0, "Content should be captured")

    assert_true(found_c_regex, "Should find c_regex.mojo")

fn main() raises:
    TestSuite.discover_tests[__functions_in_module()]().run()