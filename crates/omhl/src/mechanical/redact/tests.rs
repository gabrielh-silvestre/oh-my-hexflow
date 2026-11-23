use crate::mechanical::redact::Redactor;

// Todos os segredos abaixo são falsos e só existem para casar com o formato.
const AWS_KEY: &str = "AKIAFAKEFAKEFAKEFAKE";
const GITHUB_TOKEN: &str = "ghp_a1b2a1b2a1b2a1b2a1b2a1b2a1b2a1b2a1b2";
const GITHUB_PAT: &str = "github_pat_fakefakefakefakefakefake";
const GITLAB_TOKEN: &str = "glpat-fakefakefakefakefake";
const NPM_TOKEN: &str = "npm_a1b2a1b2a1b2a1b2a1b2a1b2a1b2a1b2a1b2";
const STRIPE_KEY: &str = "sk_live_fakefakefakefake";
const SLACK_TOKEN: &str = "xoxb-fakefakefake";
const SLACK_WEBHOOK: &str = "https://hooks.slack.com/services/TFAKE/BFAKE/fakefake";
const API_KEY: &str = "sk-fakefakefakefakefake";
const GOOGLE_KEY: &str = "AIzafakefakefakefakefakefakefakefakefak";
const JWT: &str = "eyJfakefake.eyJfakefake.fakefake1";
const PEM_BLOCK: &str =
    "-----BEGIN RSA PRIVATE KEY-----\nZmFrZQ==\nZmFrZQ==\n-----END RSA PRIVATE KEY-----";

const NOT_MASKED: [&str; 7] = [
    "src/token.rs:123:45:",
    "token::TokenKind",
    "token_count=123456",
    "password: String",
    "tokens: Vec<Token>",
    "password = compute_hash(x)",
    "let token = lexer.next_token()?;",
];

fn redacted(text: &str) -> String {
    String::from_utf8(Redactor::new().redact(text.as_bytes())).expect("the output is text")
}

fn corpus() -> Vec<String> {
    let mut lines: Vec<String> = NOT_MASKED.iter().map(|line| (*line).to_owned()).collect();
    lines.extend(
        [
            AWS_KEY,
            GITHUB_TOKEN,
            JWT,
            SLACK_WEBHOOK,
            "API_TOKEN=abcdef123456",
            "API_TOKEN=abcdef123456\r",
            "\"password\": \"hunter22\"",
            "password=hunter22;",
            "password=hunter22 token=abcdefgh",
            "postgres://app:hunter22@db:5432/x",
            "redis://:hunter22@cache:6379/0",
            "Authorization: Bearer abcdefgh12345678",
            "Authorization:",
            "Bearer abcdefghijklmnopqrstuvwxyz",
            "secret = settings_value;",
            "API_TOKEN=[REDACTED]",
            "password=\"[REDACTED]\"",
            "plain text without anything to hide",
            "key=value and token words near the secret keyword",
        ]
        .map(str::to_owned),
    );

    lines
}

#[test]
fn it_should_mask_an_aws_access_key() {
    assert_eq!(
        redacted(&format!("id {AWS_KEY} end")),
        "id [REDACTED:aws-access-key] end"
    );
}

#[test]
fn it_should_mask_a_github_token() {
    assert_eq!(redacted(GITHUB_TOKEN), "[REDACTED:github-token]");
}

#[test]
fn it_should_mask_a_github_fine_grained_token() {
    assert_eq!(redacted(GITHUB_PAT), "[REDACTED:github-token]");
}

#[test]
fn it_should_mask_a_gitlab_token() {
    assert_eq!(redacted(GITLAB_TOKEN), "[REDACTED:gitlab-token]");
}

#[test]
fn it_should_mask_an_npm_token() {
    assert_eq!(redacted(NPM_TOKEN), "[REDACTED:npm-token]");
}

#[test]
fn it_should_mask_a_stripe_key() {
    assert_eq!(redacted(STRIPE_KEY), "[REDACTED:stripe-key]");
}

#[test]
fn it_should_mask_a_slack_token() {
    assert_eq!(redacted(SLACK_TOKEN), "[REDACTED:slack-token]");
}

#[test]
fn it_should_mask_a_slack_webhook_url() {
    assert_eq!(redacted(SLACK_WEBHOOK), "[REDACTED:slack-webhook]");
}

#[test]
fn it_should_mask_an_sk_api_key() {
    assert_eq!(redacted(API_KEY), "[REDACTED:api-key]");
}

#[test]
fn it_should_mask_a_google_api_key() {
    assert_eq!(redacted(GOOGLE_KEY), "[REDACTED:google-api-key]");
}

#[test]
fn it_should_mask_a_jwt() {
    assert_eq!(redacted(JWT), "[REDACTED:jwt]");
}

#[test]
fn it_should_mask_a_private_key_block_across_lines() {
    let stream = format!("before\n{PEM_BLOCK}\nafter\n");

    let masked = redacted(&stream);

    assert_eq!(masked, "before\n[REDACTED:private-key]\nafter\n");
}

#[test]
fn it_should_mask_a_pgp_private_key_block() {
    let stream =
        "-----BEGIN PGP PRIVATE KEY BLOCK-----\nZmFrZQ==\n-----END PGP PRIVATE KEY BLOCK-----\n";

    assert_eq!(redacted(stream), "[REDACTED:private-key]\n");
}

#[test]
fn it_should_mask_a_private_key_without_end_up_to_the_end_of_the_stream() {
    let stream = "before\n-----BEGIN PRIVATE KEY-----\nZmFrZQ==\nZmFrZQ==\n";

    assert_eq!(redacted(stream), "before\n[REDACTED:private-key]");
}

#[test]
fn it_should_keep_the_prefix_of_an_authorization_credential() {
    assert_eq!(
        redacted("Authorization: Bearer abcdefgh12345678"),
        "Authorization: Bearer [REDACTED]"
    );
}

#[test]
fn it_should_mask_a_loose_bearer_value() {
    assert_eq!(
        redacted("sent Bearer abcdefghijklmnopqrstuvwxyz"),
        "sent Bearer [REDACTED]"
    );
}

#[test]
fn it_should_keep_the_user_and_the_host_of_a_url_password() {
    assert_eq!(
        redacted("postgres://app:hunter22@db:5432/x"),
        "postgres://app:[REDACTED]@db:5432/x"
    );
}

#[test]
fn it_should_mask_the_password_of_a_url_with_an_empty_user() {
    assert_eq!(
        redacted("redis://:hunter22@cache:6379/0"),
        "redis://:[REDACTED]@cache:6379/0"
    );
}

#[test]
fn it_should_mask_the_value_of_an_assignment() {
    assert_eq!(redacted("API_TOKEN=abcdef123456"), "API_TOKEN=[REDACTED]");
}

#[test]
fn it_should_mask_a_quoted_value_after_a_colon() {
    assert_eq!(
        redacted("\"password\": \"hunter22\""),
        "\"password\": [REDACTED]"
    );
}

#[test]
fn it_should_keep_the_terminator_of_an_unquoted_value() {
    assert_eq!(redacted("password=hunter22;"), "password=[REDACTED];");
}

#[test]
fn it_should_mask_a_value_that_ends_in_carriage_return() {
    assert_eq!(
        redacted("API_TOKEN=abcdef123456\r\n"),
        "API_TOKEN=[REDACTED]\r\n"
    );
}

#[test]
fn it_should_mask_two_assignments_separated_by_one_space() {
    assert_eq!(
        redacted("password=hunter22 token=abcdefgh"),
        "password=[REDACTED] token=[REDACTED]"
    );
}

#[test]
fn it_should_mask_the_known_false_positive_of_a_bare_identifier() {
    assert_eq!(redacted("secret = settings_value;"), "secret = [REDACTED];");
}

#[test]
fn it_should_leave_code_and_diagnostics_untouched() {
    for line in NOT_MASKED {
        assert_eq!(redacted(line), line);
    }
}

#[test]
fn it_should_return_text_without_secrets_byte_for_byte() {
    let text = "warning: unused variable\n  --> src/main.rs:1:1\n\nfinished in 0.1s";

    assert_eq!(redacted(text), text);
}

#[test]
fn it_should_not_change_what_is_already_masked() {
    for line in corpus() {
        let once = redacted(&line);

        assert_eq!(redacted(&once), once, "line: {line:?}");
    }
}

#[test]
fn it_should_not_change_a_quoted_value_that_is_already_masked() {
    assert_eq!(
        redacted("password=\"[REDACTED]\""),
        "password=\"[REDACTED]\""
    );
}

#[test]
fn it_should_use_a_mask_whose_size_does_not_depend_on_the_secret() {
    let short = redacted("password=abcdef");
    let long = redacted("password=abcdefghijklmnopqrstuvwxyz0123456789");

    assert_eq!(short, long);
}

#[test]
fn it_should_give_the_same_result_with_and_without_the_prefilter() {
    let stream = corpus().join("\n");
    let mut without_prefilter = Redactor::new();
    without_prefilter.prefilter = false;

    let with = Redactor::new().redact(stream.as_bytes());
    let without = without_prefilter.redact(stream.as_bytes());

    assert_eq!(with, without);
}

#[test]
fn it_should_mask_a_bearer_line_that_follows_an_authorization_line() {
    let stream = "Authorization:\nBearer abcdefghijklmnopqrstuvwxyz\n";

    let masked = redacted(stream);

    assert_eq!(masked, "Authorization:\nBearer [REDACTED]\n");
}

#[test]
fn it_should_give_the_same_result_by_line_and_over_the_whole_stream() {
    let lines = [
        "Authorization:",
        "Bearer abcdefghijklmnopqrstuvwxyz",
        "API_TOKEN=abcdef123456",
    ];

    let whole = redacted(&lines.join("\n"));

    let by_line: Vec<String> = lines.iter().map(|line| redacted(line)).collect();
    assert_eq!(whole, by_line.join("\n"));
}

#[test]
fn it_should_keep_a_line_without_a_trailing_newline() {
    assert_eq!(
        redacted("a\nAPI_TOKEN=abcdef123456"),
        "a\nAPI_TOKEN=[REDACTED]"
    );
}

#[test]
fn it_should_redact_bytes_that_are_not_utf8_without_failing() {
    let stream = b"\xff\xfe API_TOKEN=abcdef123456\n";

    let masked = Redactor::new().redact(stream);

    assert_eq!(masked, b"\xff\xfe API_TOKEN=[REDACTED]\n");
}
