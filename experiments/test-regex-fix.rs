#!/usr/bin/env rust-script
//! Test that the regex pattern in create-github-release.rs works correctly
//! with Rust's regex crate (which does not support look-ahead).
//!
//! ```cargo
//! [dependencies]
//! regex = "1"
//! ```

use regex::Regex;

fn test_changelog_extraction(version: &str, content: &str) -> Option<String> {
    let escaped_version = regex::escape(version);
    let pattern = format!(r"(?s)## \[{}\][^\n]*\n(.*?)(?:\n## \[|$)", escaped_version);
    let re = Regex::new(&pattern).unwrap();

    re.captures(content).map(|caps| {
        caps.get(1).unwrap().as_str().trim().to_string()
    })
}

fn main() {
    let changelog = r#"# Changelog

## [0.3.0] - 2026-03-24

### Added
- Full Rust implementation
- Parallel processing

## [0.2.0] - 2026-03-20

### Fixed
- Some old fix
"#;

    // Test: extract v0.3.0 section
    let result = test_changelog_extraction("0.3.0", changelog);
    assert!(result.is_some(), "Should find v0.3.0 section");
    let body = result.unwrap();
    assert!(body.contains("Full Rust implementation"), "Should contain the added items");
    assert!(!body.contains("Some old fix"), "Should NOT contain v0.2.0 items");
    println!("PASS: v0.3.0 extraction correct: {:?}", body);

    // Test: extract v0.2.0 section (last section, ends at EOF)
    let result = test_changelog_extraction("0.2.0", changelog);
    assert!(result.is_some(), "Should find v0.2.0 section");
    let body = result.unwrap();
    assert!(body.contains("Some old fix"), "Should contain the fix items");
    assert!(!body.contains("Full Rust implementation"), "Should NOT contain v0.3.0 items");
    println!("PASS: v0.2.0 extraction correct: {:?}", body);

    // Test: non-existent version
    let result = test_changelog_extraction("9.9.9", changelog);
    assert!(result.is_none(), "Should return None for missing version");
    println!("PASS: Missing version returns None");

    // Test: old broken pattern (should fail to compile)
    let escaped = regex::escape("0.3.0");
    let broken_pattern = format!(r"(?s)## \[{}\].*?\n(.*?)(?=\n## \[|$)", escaped);
    let result = Regex::new(&broken_pattern);
    assert!(result.is_err(), "Look-ahead pattern should fail to compile");
    println!("PASS: Old broken pattern correctly fails to compile: {}", result.err().unwrap());

    println!("\nAll tests passed!");
}
