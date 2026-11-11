mod support;

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use ::omhl::mechanical::order::order;
use support::sid as id;

#[test]
fn it_should_order_alphabetically_when_there_is_no_after_relation() {
    let ids = [id("checks.c"), id("checks.a"), id("checks.b")];

    let ordered = order(&ids, &[]);

    assert_eq!(ordered, [id("checks.a"), id("checks.b"), id("checks.c")]);
}

#[test]
fn it_should_order_a_check_after_the_one_it_follows() {
    let ids = [id("checks.a"), id("checks.b")];
    let after = [(id("checks.a"), id("checks.b"))];

    let ordered = order(&ids, &after);

    assert_eq!(ordered, [id("checks.b"), id("checks.a")]);
}

#[test]
fn it_should_ignore_an_after_edge_that_points_outside_the_event() {
    let ids = [id("checks.b"), id("checks.a")];
    let after = [
        (id("checks.a"), id("checks.z")),
        (id("checks.y"), id("checks.b")),
    ];

    let ordered = order(&ids, &after);

    assert_eq!(ordered, [id("checks.a"), id("checks.b")]);
}

#[test]
fn it_should_keep_alphabetical_order_inside_the_same_level() {
    let ids = [
        id("checks.d"),
        id("checks.c"),
        id("checks.b"),
        id("checks.a"),
    ];
    let after = [(id("checks.a"), id("checks.b"))];

    let ordered = order(&ids, &after);

    assert_eq!(
        ordered,
        [
            id("checks.b"),
            id("checks.c"),
            id("checks.d"),
            id("checks.a")
        ]
    );
}

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
