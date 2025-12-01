from src.scanner.c_regex import Regex
from testing import assert_true, assert_false, TestSuite

fn test_regex_basic() raises:
    var re = Regex("hello")
    assert_true(re.matches("hello world"), "Should match 'hello world'")
    assert_false(re.matches("goodbye"), "Should not match 'goodbye'")

fn test_regex_icase() raises:
    var re = Regex("HeLLo")
    assert_true(re.matches("hello world"), "Should match case-insensitive")
    assert_true(re.matches("HELLO WORLD"), "Should match upper case")

fn main() raises:
    # Discover and run all functions starting with 'test_'
    TestSuite.discover_tests[__functions_in_module()]().run()