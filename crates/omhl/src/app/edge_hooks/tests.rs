use std::time::Duration;

use super::PUSH_WARNING_TIMEOUT;

#[test]
fn it_should_keep_the_gh_limit_of_the_push_warning_below_the_hook_timeout() {
    let hook_timeout = Duration::from_secs(10);

    let limit = PUSH_WARNING_TIMEOUT;

    assert_eq!(limit, Duration::from_secs(5));
    assert!(limit < hook_timeout);
}
