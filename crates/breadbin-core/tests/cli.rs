use std::process::Command;

#[test]
#[ignore]
fn test_menu_runs_in_tty() {
    // Build the binary first (debug profile)
    let build_status = Command::new("cargo")
        .args(["build", "--bin", "breadbin"])
        .status()
        .expect("failed to execute cargo build");
    assert!(build_status.success(), "cargo build failed");

    // Run the binary with the "menu" subcommand. This test should be executed from an actual TTY.
    let run_status = Command::new("target/debug/breadbin")
        .arg("menu")
        .status()
        .expect("failed to execute breadbin");
    assert!(run_status.success(), "breadbin menu did not exit successfully");
}
