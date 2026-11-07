use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_omhl"))
        .args(args)
        .output()
        .expect("the omhl binary runs")
}

#[test]
fn it_should_print_the_version_and_exit_zero() {
    let output = run(&["--version"]);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("omhl {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn it_should_print_usage_and_exit_two_for_any_other_argument() {
    let none = run(&[]);
    let bogus = run(&["--bogus"]);

    for output in [none, bogus] {
        assert_eq!(output.status.code(), Some(2));
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("Usage:"),
            "stderr carries the clap usage"
        );
        assert!(output.stdout.is_empty(), "stdout stays empty");
    }
}
