use crate::support;

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use ::omhl::mechanical::order::order;
use support::sid as id;

#[test]
fn it_should_not_loop_on_a_cycle() {
    let ids = vec![
        id("checks.d"),
        id("checks.c"),
        id("checks.b"),
        id("checks.a"),
    ];
    let after = vec![
        (id("checks.a"), id("checks.b")),
        (id("checks.b"), id("checks.a")),
        (id("checks.d"), id("checks.a")),
    ];
    let (sender, receiver) = mpsc::channel();

    thread::spawn(move || {
        let _ = sender.send(order(&ids, &after));
    });
    let ordered = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("order must terminate on a cycle");

    assert_eq!(
        ordered,
        [
            id("checks.c"),
            id("checks.a"),
            id("checks.b"),
            id("checks.d")
        ]
    );
}
